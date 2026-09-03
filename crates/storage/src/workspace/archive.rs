use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context as _, Result, bail};
use entity::{
    board, board_label, board_property, board_property_option, board_template, card, entry,
    entry_attachment, entry_checklist_item, entry_label, entry_property_value, note, note_alias,
    note_link, note_link_index_state, project, saved_board_view, workspace_link,
    workspace_link_index_state, workspace_reference_alias,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseTransaction,
    EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub const WORKSPACE_ARCHIVE_FORMAT: &str = "castle-workspace";
pub const WORKSPACE_ARCHIVE_VERSION: u32 = 1;

const MANIFEST_PATH: &str = "manifest.json";
const WORKSPACE_DATA_PATH: &str = "workspace.json";
const SETTINGS_PATH: &str = "settings.json";
const MAX_ARCHIVE_FILE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ARCHIVE_ENTRY_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ARCHIVE_UNCOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 25_000;
const MAX_FILE_NAME_BYTES: usize = 180;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportMode {
    Merge,
    Replace,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub struct WorkspaceArchiveCounts {
    pub projects: usize,
    pub boards: usize,
    pub lists: usize,
    pub entries: usize,
    pub notes: usize,
    pub board_labels: usize,
    pub entry_labels: usize,
    pub entry_attachments: usize,
    pub note_attachments: usize,
    pub checklist_items: usize,
    pub board_properties: usize,
    pub property_options: usize,
    pub property_values: usize,
    pub saved_views: usize,
    pub templates: usize,
    pub note_aliases: usize,
    pub note_links: usize,
    pub workspace_links: usize,
    pub reference_aliases: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceExportSummary {
    pub counts: WorkspaceArchiveCounts,
    pub destination: PathBuf,
    pub missing_attachments: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceImportSummary {
    pub counts: WorkspaceArchiveCounts,
    pub mode: ImportMode,
    pub settings_json: Vec<u8>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveManifest {
    format: String,
    version: u32,
    created_at: i64,
    castle_version: String,
    counts: WorkspaceArchiveCounts,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveData {
    projects: Vec<ArchiveProject>,
    boards: Vec<ArchiveBoard>,
    lists: Vec<ArchiveList>,
    entries: Vec<ArchiveEntry>,
    notes: Vec<ArchiveNote>,
    board_labels: Vec<ArchiveBoardLabel>,
    entry_labels: Vec<ArchiveEntryLabel>,
    entry_attachments: Vec<ArchiveEntryAttachment>,
    note_attachments: Vec<ArchiveNoteAttachment>,
    checklist_items: Vec<ArchiveChecklistItem>,
    board_properties: Vec<ArchiveBoardProperty>,
    property_options: Vec<ArchivePropertyOption>,
    property_values: Vec<ArchivePropertyValue>,
    saved_views: Vec<ArchiveSavedView>,
    templates: Vec<ArchiveTemplate>,
    note_aliases: Vec<ArchiveNoteAlias>,
    note_links: Vec<ArchiveNoteLink>,
    workspace_links: Vec<ArchiveWorkspaceLink>,
    reference_aliases: Vec<ArchiveReferenceAlias>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveProject {
    id: i64,
    name: String,
    archived: bool,
    position: i32,
    deleted_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveBoard {
    id: i64,
    title: String,
    project_id: Option<i64>,
    is_pinned: bool,
    last_opened_at: Option<i64>,
    last_selected_view_id: i64,
    deleted_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveList {
    id: i64,
    title: String,
    board_id: i64,
    position: i32,
    deleted_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveEntry {
    id: i64,
    title: String,
    description: String,
    list_id: i64,
    position: i32,
    due_on: Option<String>,
    reminder_enabled: bool,
    reminder_notified_for: Option<String>,
    deleted_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveNote {
    id: i64,
    title: String,
    project_id: Option<i64>,
    content_path: String,
    created_at: i64,
    updated_at: i64,
    is_pinned: bool,
    last_opened_at: Option<i64>,
    deleted_at: Option<i64>,
    #[serde(skip)]
    content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveBoardLabel {
    id: i64,
    board_id: i64,
    name: String,
    color: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveEntryLabel {
    id: i64,
    entry_id: i64,
    board_label_id: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveEntryAttachment {
    id: i64,
    entry_id: i64,
    file_name: String,
    archive_path: Option<String>,
    #[serde(skip)]
    source_file_name: String,
    #[serde(skip)]
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveNoteAttachment {
    note_id: i64,
    file_name: String,
    archive_path: String,
    #[serde(skip)]
    source_file_name: String,
    #[serde(skip)]
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveChecklistItem {
    id: i64,
    entry_id: i64,
    title: String,
    checked: bool,
    position: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveBoardProperty {
    id: i64,
    board_id: i64,
    name: String,
    kind: String,
    position: i32,
    deleted_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchivePropertyOption {
    id: i64,
    property_id: i64,
    name: String,
    color: String,
    position: i32,
    deleted_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchivePropertyValue {
    entry_id: i64,
    property_id: i64,
    text_value: Option<String>,
    number_value: Option<f64>,
    boolean_value: Option<bool>,
    date_value: Option<String>,
    option_id: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveSavedView {
    id: i64,
    board_id: i64,
    name: String,
    position: i32,
    is_default: bool,
    config_version: i32,
    config_json: String,
    deleted_at: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveTemplate {
    id: i64,
    name: String,
    description: String,
    definition_json: String,
    created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveNoteAlias {
    id: i64,
    note_id: i64,
    alias: String,
    normalized_alias: String,
    created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveNoteLink {
    source_note_id: i64,
    ordinal: i32,
    target_note_id: Option<i64>,
    raw_target: String,
    display_text: Option<String>,
    start_byte: i64,
    end_byte: i64,
    line_number: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveWorkspaceLink {
    source_note_id: Option<i64>,
    source_entry_id: Option<i64>,
    target_note_id: Option<i64>,
    target_board_id: Option<i64>,
    target_list_id: Option<i64>,
    target_entry_id: Option<i64>,
    target_saved_view_id: Option<i64>,
    origin: String,
    ordinal: i32,
    raw_target: Option<String>,
    display_text: Option<String>,
    start_byte: Option<i64>,
    end_byte: Option<i64>,
    line_number: Option<i32>,
    created_at: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ArchiveReferenceAlias {
    id: i64,
    alias: String,
    normalized_alias: String,
    project_id: Option<i64>,
    board_id: Option<i64>,
    list_id: Option<i64>,
    entry_id: Option<i64>,
    saved_view_id: Option<i64>,
    created_at: i64,
}

impl ArchiveData {
    fn counts(&self) -> WorkspaceArchiveCounts {
        WorkspaceArchiveCounts {
            projects: self.projects.len(),
            boards: self.boards.len(),
            lists: self.lists.len(),
            entries: self.entries.len(),
            notes: self.notes.len(),
            board_labels: self.board_labels.len(),
            entry_labels: self.entry_labels.len(),
            entry_attachments: self.entry_attachments.len(),
            note_attachments: self.note_attachments.len(),
            checklist_items: self.checklist_items.len(),
            board_properties: self.board_properties.len(),
            property_options: self.property_options.len(),
            property_values: self.property_values.len(),
            saved_views: self.saved_views.len(),
            templates: self.templates.len(),
            note_aliases: self.note_aliases.len(),
            note_links: self.note_links.len(),
            workspace_links: self.workspace_links.len(),
            reference_aliases: self.reference_aliases.len(),
        }
    }
}

pub async fn export_workspace(
    db: &(impl ConnectionTrait + TransactionTrait<Transaction = DatabaseTransaction>),
    data_dir: &Path,
    settings_json: &[u8],
    destination: &Path,
) -> Result<WorkspaceExportSummary> {
    validate_settings_json(settings_json)?;

    let data_dir_for_snapshot = data_dir.to_path_buf();
    let archive = db
        .transaction::<_, ArchiveData, anyhow::Error>(|transaction| {
            Box::pin(async move { load_archive_data(transaction, &data_dir_for_snapshot).await })
        })
        .await?;
    let counts = archive.counts();
    let missing_attachments = archive
        .entry_attachments
        .iter()
        .filter(|attachment| attachment.archive_path.is_none())
        .count();

    write_archive(
        &archive,
        data_dir,
        settings_json,
        destination,
        counts.clone(),
        missing_attachments,
    )?;

    Ok(WorkspaceExportSummary {
        counts,
        destination: destination.to_path_buf(),
        missing_attachments,
    })
}

async fn load_archive_data(db: &impl ConnectionTrait, data_dir: &Path) -> Result<ArchiveData> {
    let projects = project::Entity::find()
        .order_by_asc(project::Column::Id)
        .all(db)
        .await?;
    let boards = board::Entity::find()
        .order_by_asc(board::Column::Id)
        .all(db)
        .await?;
    let lists = card::Entity::find()
        .order_by_asc(card::Column::Id)
        .all(db)
        .await?;
    let entries = entry::Entity::find()
        .order_by_asc(entry::Column::Id)
        .all(db)
        .await?;
    let notes = note::Entity::find()
        .order_by_asc(note::Column::Id)
        .all(db)
        .await?;
    let board_labels = board_label::Entity::find()
        .order_by_asc(board_label::Column::Id)
        .all(db)
        .await?;
    let entry_labels = entry_label::Entity::find()
        .order_by_asc(entry_label::Column::Id)
        .all(db)
        .await?;
    let entry_attachments = entry_attachment::Entity::find()
        .order_by_asc(entry_attachment::Column::Id)
        .all(db)
        .await?;
    let checklist_items = entry_checklist_item::Entity::find()
        .order_by_asc(entry_checklist_item::Column::Id)
        .all(db)
        .await?;
    let board_properties = board_property::Entity::find()
        .order_by_asc(board_property::Column::Id)
        .all(db)
        .await?;
    let property_options = board_property_option::Entity::find()
        .order_by_asc(board_property_option::Column::Id)
        .all(db)
        .await?;
    let property_values = entry_property_value::Entity::find()
        .order_by_asc(entry_property_value::Column::EntryId)
        .order_by_asc(entry_property_value::Column::PropertyId)
        .all(db)
        .await?;
    let saved_views = saved_board_view::Entity::find()
        .order_by_asc(saved_board_view::Column::Id)
        .all(db)
        .await?;
    let templates = board_template::Entity::find()
        .order_by_asc(board_template::Column::Id)
        .all(db)
        .await?;
    let note_aliases = note_alias::Entity::find()
        .order_by_asc(note_alias::Column::Id)
        .all(db)
        .await?;
    let note_links = note_link::Entity::find()
        .order_by_asc(note_link::Column::SourceNoteId)
        .order_by_asc(note_link::Column::Ordinal)
        .all(db)
        .await?;
    let workspace_links = workspace_link::Entity::find()
        .order_by_asc(workspace_link::Column::Id)
        .all(db)
        .await?;
    let reference_aliases = workspace_reference_alias::Entity::find()
        .order_by_asc(workspace_reference_alias::Column::Id)
        .all(db)
        .await?;

    let note_attachments = notes
        .iter()
        .map(|note| load_note_attachments(data_dir, note.id))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    let mut used_entry_file_names = HashMap::<i64, HashSet<String>>::new();
    let archive_entry_attachments = entry_attachments
        .iter()
        .map(|attachment| {
            let used_names = used_entry_file_names
                .entry(attachment.entry_id)
                .or_default();
            let file_name = unique_portable_file_name(&attachment.file_name, used_names);
            let archive_path = if validate_file_name(&attachment.file_name).is_ok() {
                let source_path = data_dir
                    .join("attachments")
                    .join("entries")
                    .join(attachment.entry_id.to_string())
                    .join(&attachment.file_name);
                match fs::symlink_metadata(&source_path) {
                    Ok(metadata) if metadata.file_type().is_file() => Some(format!(
                        "attachments/entries/n{}/a{}-{}",
                        attachment.entry_id, attachment.id, file_name
                    )),
                    _ => None,
                }
            } else {
                None
            };
            ArchiveEntryAttachment {
                id: attachment.id,
                entry_id: attachment.entry_id,
                file_name,
                archive_path,
                source_file_name: attachment.file_name.clone(),
                bytes: Vec::new(),
            }
        })
        .collect::<Vec<_>>();

    let archive_notes = notes
        .iter()
        .map(|stored_note| {
            let extension = note_extension(stored_note.file_path.as_deref());
            let content_path = format!(
                "notes/{}-n{}.{}",
                portable_file_component(&stored_note.title, "note"),
                stored_note.id,
                extension
            );
            let content = rewrite_all_attachment_references(
                &stored_note.cached_content,
                data_dir,
                &note_attachments,
                &archive_entry_attachments,
                true,
            );
            ArchiveNote {
                id: stored_note.id,
                title: stored_note.title.clone(),
                project_id: stored_note.project_id,
                content_path,
                created_at: stored_note.created_at,
                updated_at: stored_note.updated_at,
                is_pinned: stored_note.is_pinned,
                last_opened_at: stored_note.last_opened_at,
                deleted_at: stored_note.deleted_at,
                content,
            }
        })
        .collect::<Vec<_>>();

    let archive_entries = entries
        .iter()
        .map(|stored_entry| ArchiveEntry {
            id: stored_entry.id,
            title: stored_entry.title.clone(),
            description: rewrite_all_attachment_references(
                &stored_entry.description,
                data_dir,
                &note_attachments,
                &archive_entry_attachments,
                false,
            ),
            list_id: stored_entry.card_id,
            position: stored_entry.position,
            due_on: stored_entry.due_on.clone(),
            reminder_enabled: stored_entry.reminder_enabled,
            reminder_notified_for: stored_entry.reminder_notified_for.clone(),
            deleted_at: stored_entry.deleted_at,
        })
        .collect::<Vec<_>>();

    Ok(ArchiveData {
        projects: projects
            .into_iter()
            .map(|model| ArchiveProject {
                id: model.id,
                name: model.name,
                archived: model.archived,
                position: model.position,
                deleted_at: model.deleted_at,
            })
            .collect(),
        boards: boards
            .into_iter()
            .map(|model| ArchiveBoard {
                id: model.id,
                title: model.title,
                project_id: model.project_id,
                is_pinned: model.is_pinned,
                last_opened_at: model.last_opened_at,
                last_selected_view_id: model.last_selected_view_id,
                deleted_at: model.deleted_at,
            })
            .collect(),
        lists: lists
            .into_iter()
            .map(|model| ArchiveList {
                id: model.id,
                title: model.title,
                board_id: model.board_id,
                position: model.position,
                deleted_at: model.deleted_at,
            })
            .collect(),
        entries: archive_entries,
        notes: archive_notes,
        board_labels: board_labels
            .into_iter()
            .map(|model| ArchiveBoardLabel {
                id: model.id,
                board_id: model.board_id,
                name: model.name,
                color: model.color,
            })
            .collect(),
        entry_labels: entry_labels
            .into_iter()
            .map(|model| ArchiveEntryLabel {
                id: model.id,
                entry_id: model.entry_id,
                board_label_id: model.board_label_id,
            })
            .collect(),
        entry_attachments: archive_entry_attachments,
        note_attachments,
        checklist_items: checklist_items
            .into_iter()
            .map(|model| ArchiveChecklistItem {
                id: model.id,
                entry_id: model.entry_id,
                title: model.title,
                checked: model.checked,
                position: model.position,
            })
            .collect(),
        board_properties: board_properties
            .into_iter()
            .map(|model| ArchiveBoardProperty {
                id: model.id,
                board_id: model.board_id,
                name: model.name,
                kind: model.kind,
                position: model.position,
                deleted_at: model.deleted_at,
            })
            .collect(),
        property_options: property_options
            .into_iter()
            .map(|model| ArchivePropertyOption {
                id: model.id,
                property_id: model.property_id,
                name: model.name,
                color: model.color,
                position: model.position,
                deleted_at: model.deleted_at,
            })
            .collect(),
        property_values: property_values
            .into_iter()
            .map(|model| ArchivePropertyValue {
                entry_id: model.entry_id,
                property_id: model.property_id,
                text_value: model.text_value,
                number_value: model.number_value,
                boolean_value: model.boolean_value,
                date_value: model.date_value,
                option_id: model.option_id,
            })
            .collect(),
        saved_views: saved_views
            .into_iter()
            .map(|model| ArchiveSavedView {
                id: model.id,
                board_id: model.board_id,
                name: model.name,
                position: model.position,
                is_default: model.is_default,
                config_version: model.config_version,
                config_json: model.config_json,
                deleted_at: model.deleted_at,
            })
            .collect(),
        templates: templates
            .into_iter()
            .map(|model| ArchiveTemplate {
                id: model.id,
                name: model.name,
                description: model.description,
                definition_json: model.definition_json,
                created_at: model.created_at,
            })
            .collect(),
        note_aliases: note_aliases
            .into_iter()
            .map(|model| ArchiveNoteAlias {
                id: model.id,
                note_id: model.note_id,
                alias: model.alias,
                normalized_alias: model.normalized_alias,
                created_at: model.created_at,
            })
            .collect(),
        note_links: note_links
            .into_iter()
            .map(|model| ArchiveNoteLink {
                source_note_id: model.source_note_id,
                ordinal: model.ordinal,
                target_note_id: model.target_note_id,
                raw_target: model.raw_target,
                display_text: model.display_text,
                start_byte: model.start_byte,
                end_byte: model.end_byte,
                line_number: model.line_number,
            })
            .collect(),
        workspace_links: workspace_links
            .into_iter()
            .map(|model| ArchiveWorkspaceLink {
                source_note_id: model.source_note_id,
                source_entry_id: model.source_entry_id,
                target_note_id: model.target_note_id,
                target_board_id: model.target_board_id,
                target_list_id: model.target_card_id,
                target_entry_id: model.target_entry_id,
                target_saved_view_id: model.target_saved_view_id,
                origin: model.origin,
                ordinal: model.ordinal,
                raw_target: model.raw_target,
                display_text: model.display_text,
                start_byte: model.start_byte,
                end_byte: model.end_byte,
                line_number: model.line_number,
                created_at: model.created_at,
            })
            .collect(),
        reference_aliases: reference_aliases
            .into_iter()
            .map(|model| ArchiveReferenceAlias {
                id: model.id,
                alias: model.alias,
                normalized_alias: model.normalized_alias,
                project_id: model.project_id,
                board_id: model.board_id,
                list_id: model.list_id,
                entry_id: model.card_id,
                saved_view_id: model.saved_view_id,
                created_at: model.created_at,
            })
            .collect(),
    })
}

fn load_note_attachments(data_dir: &Path, note_id: i64) -> Result<Vec<ArchiveNoteAttachment>> {
    let directory = data_dir.join("attachments").join(note_id.to_string());
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("could not read {}", directory.display()));
        }
    };

    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.with_context(|| format!("could not read {}", directory.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("could not inspect {}", entry.path().display()))?;
        if file_type.is_file() {
            files.push(entry);
        }
    }
    files.sort_by_cached_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());

    let mut used_names = HashSet::new();
    Ok(files
        .into_iter()
        .map(|entry| {
            let source_file_name = entry.file_name().to_string_lossy().into_owned();
            let file_name = unique_portable_file_name(&source_file_name, &mut used_names);
            ArchiveNoteAttachment {
                note_id,
                archive_path: format!("attachments/notes/n{note_id}/{file_name}"),
                file_name,
                source_file_name,
                bytes: Vec::new(),
            }
        })
        .collect())
}

fn rewrite_all_attachment_references(
    content: &str,
    data_dir: &Path,
    note_attachments: &[ArchiveNoteAttachment],
    entry_attachments: &[ArchiveEntryAttachment],
    note_document: bool,
) -> String {
    let mut rewritten = content.to_string();
    for attachment in note_attachments {
        let replacement = if note_document {
            format!("../{}", attachment.archive_path)
        } else {
            attachment.archive_path.clone()
        };
        rewritten = replace_attachment_reference_variants(
            &rewritten,
            data_dir,
            "attachments",
            attachment.note_id,
            &attachment.source_file_name,
            &replacement,
        );
    }
    for attachment in entry_attachments {
        if let Some(archive_path) = &attachment.archive_path {
            let replacement = if note_document {
                format!("../{archive_path}")
            } else {
                archive_path.clone()
            };
            rewritten = replace_attachment_reference_variants(
                &rewritten,
                data_dir,
                "attachments/entries",
                attachment.entry_id,
                &attachment.source_file_name,
                &replacement,
            );
        }
    }
    rewritten
}

fn replace_attachment_reference_variants(
    content: &str,
    data_dir: &Path,
    relative_root: &str,
    source_id: i64,
    source_file_name: &str,
    replacement: &str,
) -> String {
    let forward_name = source_file_name.replace('\\', "/");
    let back_name = source_file_name.replace('/', "\\");
    let relative_forward = format!("{relative_root}/{source_id}/{forward_name}");
    let relative_back = format!(
        "{}\\{}\\{}",
        relative_root.replace('/', "\\"),
        source_id,
        back_name
    );
    let absolute_base = data_dir
        .join(relative_root.replace('/', std::path::MAIN_SEPARATOR_STR))
        .join(source_id.to_string());
    let absolute_forward = format!(
        "{}/{}",
        absolute_base.to_string_lossy().replace('\\', "/"),
        forward_name
    );
    let absolute_back = format!(
        "{}\\{}",
        absolute_base.to_string_lossy().replace('/', "\\"),
        back_name
    );

    [
        absolute_forward,
        absolute_back,
        relative_forward,
        relative_back,
    ]
    .into_iter()
    .fold(content.to_string(), |value, needle| {
        replace_path_token(&value, &needle, replacement)
    })
}

fn replace_path_token(content: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return content.to_string();
    }

    let mut rewritten = String::with_capacity(content.len());
    let mut cursor = 0;
    while let Some(relative_start) = content[cursor..].find(needle) {
        let start = cursor + relative_start;
        let end = start + needle.len();
        let preceding = content[..start].chars().next_back();
        let following = content[end..].chars().next();
        if preceding.is_some_and(is_filename_continuation)
            || following.is_some_and(is_path_continuation)
        {
            rewritten.push_str(&content[cursor..end]);
            cursor = end;
            continue;
        }

        let token_start = path_token_start(content, start);
        rewritten.push_str(&content[cursor..token_start]);
        rewritten.push_str(replacement);
        cursor = end;
    }
    rewritten.push_str(&content[cursor..]);
    rewritten
}

fn is_path_continuation(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | '.' | '/' | '\\')
}

fn is_filename_continuation(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-' | '.')
}

fn path_token_start(content: &str, end: usize) -> usize {
    let mut start = end;
    while start > 0 {
        let Some((character_start, character)) = content[..start].char_indices().next_back() else {
            break;
        };
        if character.is_whitespace()
            || matches!(
                character,
                '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | '='
            )
        {
            break;
        }
        start = character_start;
    }
    start
}

fn note_extension(file_path: Option<&str>) -> &'static str {
    let extension = file_path
        .and_then(|path| Path::new(path).extension())
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("markdown") => "markdown",
        Some("txt") => "txt",
        Some("json") => "json",
        _ => "md",
    }
}

fn portable_file_component(value: &str, fallback: &str) -> String {
    let mut component = value
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' if character.is_ascii() => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    component = component.trim().trim_matches('.').to_string();
    if component.is_empty() {
        component = fallback.to_string();
    }
    if component.len() > MAX_FILE_NAME_BYTES {
        let end = component
            .char_indices()
            .take_while(|(index, character)| {
                index.saturating_add(character.len_utf8()) <= MAX_FILE_NAME_BYTES
            })
            .map(|(index, character)| index + character.len_utf8())
            .last()
            .unwrap_or(0);
        component.truncate(end);
        component = component.trim_matches('.').to_string();
    }
    if component.is_empty() {
        component = fallback.to_string();
    }
    if is_reserved_windows_file_name(&component) {
        component.insert(0, '_');
    }
    component
}

fn portable_file_name(value: &str) -> String {
    portable_file_component(value, "attachment")
}

fn unique_portable_file_name(value: &str, used_names: &mut HashSet<String>) -> String {
    let base = portable_file_name(value);
    if used_names.insert(base.clone()) {
        return base;
    }

    let path = Path::new(&base);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("attachment");
    let extension = path.extension().and_then(|extension| extension.to_str());
    let mut index = 2usize;
    loop {
        let candidate = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        if used_names.insert(candidate.clone()) {
            return candidate;
        }
        index = index.saturating_add(1);
    }
}

fn validate_settings_json(settings_json: &[u8]) -> Result<()> {
    if settings_json.len() as u64 > MAX_ARCHIVE_ENTRY_BYTES {
        bail!("settings.json is too large");
    }
    let value: serde_json::Value =
        serde_json::from_slice(settings_json).context("settings.json is not valid JSON")?;
    if !value.is_object() {
        bail!("settings.json must contain a JSON object");
    }
    Ok(())
}

fn write_archive(
    data: &ArchiveData,
    data_dir: &Path,
    settings_json: &[u8],
    destination: &Path,
    counts: WorkspaceArchiveCounts,
    _missing_attachments: usize,
) -> Result<()> {
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty());
    if let Some(parent) = parent
        && !parent.is_dir()
    {
        bail!(
            "archive destination directory {} does not exist",
            parent.display()
        );
    }
    if destination.exists() && !fs::metadata(destination)?.is_file() {
        bail!(
            "archive destination {} is not a regular file",
            destination.display()
        );
    }

    let mut last_error = None;
    for attempt in 0..8_u32 {
        let temporary_path = temporary_archive_path(destination, attempt);
        let file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                last_error = Some(anyhow::Error::from(error));
                break;
            }
        };

        let result = write_archive_file(
            file,
            &temporary_path,
            data,
            data_dir,
            settings_json,
            destination,
            counts.clone(),
        );
        if result.is_ok() {
            return Ok(());
        }
        last_error = result.err();
        let _ = fs::remove_file(&temporary_path);
        break;
    }

    match last_error {
        Some(error) => Err(error),
        None => bail!("could not allocate a temporary archive path"),
    }
}

fn write_archive_file(
    file: File,
    temporary_path: &Path,
    data: &ArchiveData,
    data_dir: &Path,
    settings_json: &[u8],
    destination: &Path,
    counts: WorkspaceArchiveCounts,
) -> Result<()> {
    let manifest = ArchiveManifest {
        format: WORKSPACE_ARCHIVE_FORMAT.to_string(),
        version: WORKSPACE_ARCHIVE_VERSION,
        created_at: now_ts(),
        castle_version: env!("CARGO_PKG_VERSION").to_string(),
        counts,
    };
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut archive = ZipWriter::new(file);

    write_json_entry(&mut archive, MANIFEST_PATH, &manifest, options)?;
    write_json_entry(&mut archive, WORKSPACE_DATA_PATH, data, options)?;
    archive.start_file(SETTINGS_PATH, options)?;
    archive.write_all(settings_json)?;

    for note in &data.notes {
        check_export_file_size(Path::new(&note.content_path), note.content.len() as u64)?;
        archive.start_file(&note.content_path, options)?;
        archive.write_all(note.content.as_bytes())?;
    }

    for attachment in &data.note_attachments {
        let source_path = data_dir
            .join("attachments")
            .join(attachment.note_id.to_string())
            .join(&attachment.source_file_name);
        let bytes = read_export_file(&source_path)?;
        archive.start_file(&attachment.archive_path, options)?;
        archive.write_all(&bytes)?;
    }

    for attachment in &data.entry_attachments {
        let Some(archive_path) = &attachment.archive_path else {
            continue;
        };
        let source_path = data_dir
            .join("attachments")
            .join("entries")
            .join(attachment.entry_id.to_string())
            .join(&attachment.source_file_name);
        let bytes = read_export_file(&source_path)?;
        archive.start_file(archive_path, options)?;
        archive.write_all(&bytes)?;
    }

    archive.finish()?;
    let archive_size = fs::metadata(temporary_path)
        .with_context(|| format!("could not inspect {}", temporary_path.display()))?
        .len();
    if archive_size > MAX_ARCHIVE_FILE_BYTES {
        bail!("exported workspace archive is too large");
    }
    replace_archive_file(temporary_path, destination)
}

fn write_json_entry<T: Serialize>(
    archive: &mut ZipWriter<File>,
    path: &str,
    value: &T,
    options: SimpleFileOptions,
) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    check_export_file_size(Path::new(path), bytes.len() as u64)?;
    archive.start_file(path, options)?;
    archive.write_all(&bytes)?;
    Ok(())
}

fn read_export_file(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("could not inspect {}", path.display()))?;
    if !metadata.file_type().is_file() {
        bail!("{} is not a regular file", path.display());
    }
    check_export_file_size(path, metadata.len())?;
    let bytes = fs::read(path).with_context(|| format!("could not read {}", path.display()))?;
    check_export_file_size(path, bytes.len() as u64)?;
    Ok(bytes)
}

fn check_export_file_size(path: &Path, size: u64) -> Result<()> {
    if size > MAX_ARCHIVE_ENTRY_BYTES {
        bail!("{} is larger than the archive file limit", path.display());
    }
    Ok(())
}

fn temporary_archive_path(destination: &Path, attempt: u32) -> PathBuf {
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("castle-workspace.zip");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temporary_name = format!(".{file_name}.{timestamp}.{attempt}.tmp");
    destination
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(temporary_name)
}

fn replace_archive_file(temporary_path: &Path, destination: &Path) -> Result<()> {
    if !destination.exists() {
        return fs::rename(temporary_path, destination).with_context(|| {
            format!(
                "could not move workspace archive into {}",
                destination.display()
            )
        });
    }

    let backup_path = temporary_path.with_extension("backup");
    fs::rename(destination, &backup_path).with_context(|| {
        format!(
            "could not replace existing workspace archive {}",
            destination.display()
        )
    })?;

    match fs::rename(temporary_path, destination) {
        Ok(()) => {
            let _ = fs::remove_file(backup_path);
            Ok(())
        }
        Err(error) => {
            let restore_error = fs::rename(&backup_path, destination).err();
            match restore_error {
                Some(restore_error) => Err(error).context(format!(
                    "could not install workspace archive and could not restore the previous file: {restore_error}"
                )),
                None => Err(error).context("could not install workspace archive"),
            }
        }
    }
}

fn read_archive(archive_path: &Path) -> Result<(ArchiveData, Vec<u8>)> {
    let metadata = fs::metadata(archive_path)
        .with_context(|| format!("could not inspect {}", archive_path.display()))?;
    if !metadata.is_file() {
        bail!("{} is not a file", archive_path.display());
    }
    if metadata.len() > MAX_ARCHIVE_FILE_BYTES {
        bail!("workspace archive is too large");
    }

    let file = File::open(archive_path)
        .with_context(|| format!("could not open {}", archive_path.display()))?;
    let mut zip = ZipArchive::new(file).context("workspace archive is not a valid ZIP file")?;
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        bail!("workspace archive contains too many files");
    }

    let mut entries = HashMap::<String, Vec<u8>>::new();
    let mut total_uncompressed = 0_u64;
    for index in 0..zip.len() {
        let entry = zip.by_index(index).context("could not read a ZIP entry")?;
        let name = entry.name().to_string();
        validate_archive_path(&name)?;
        if name.ends_with('/') {
            bail!("workspace archive contains a directory entry");
        }
        if !is_supported_archive_entry(&name) {
            bail!("workspace archive contains an unsupported file: {name}");
        }
        if entries.contains_key(&name) {
            bail!("workspace archive contains duplicate file: {name}");
        }
        let declared_size = entry.size();
        if declared_size > MAX_ARCHIVE_ENTRY_BYTES {
            bail!("workspace archive entry {name} is too large");
        }
        total_uncompressed = total_uncompressed.saturating_add(declared_size);
        if total_uncompressed > MAX_ARCHIVE_UNCOMPRESSED_BYTES {
            bail!("workspace archive expands beyond the supported size");
        }
        let mut bytes = Vec::new();
        entry
            .take(MAX_ARCHIVE_ENTRY_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .with_context(|| format!("could not read workspace archive entry {name}"))?;
        if bytes.len() as u64 > MAX_ARCHIVE_ENTRY_BYTES {
            bail!("workspace archive entry {name} is too large");
        }
        total_uncompressed = total_uncompressed
            .saturating_sub(declared_size)
            .saturating_add(bytes.len() as u64);
        if total_uncompressed > MAX_ARCHIVE_UNCOMPRESSED_BYTES {
            bail!("workspace archive expands beyond the supported size");
        }
        entries.insert(name, bytes);
    }

    let manifest = parse_archive_json::<ArchiveManifest>(&entries, MANIFEST_PATH)?;
    if manifest.format != WORKSPACE_ARCHIVE_FORMAT {
        bail!("this file is not a Castle workspace archive");
    }
    if manifest.version != WORKSPACE_ARCHIVE_VERSION {
        bail!(
            "unsupported Castle workspace archive version {}; expected {}",
            manifest.version,
            WORKSPACE_ARCHIVE_VERSION
        );
    }

    let mut data = parse_archive_json::<ArchiveData>(&entries, WORKSPACE_DATA_PATH)?;
    let settings_json = entries
        .get(SETTINGS_PATH)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("workspace archive is missing settings.json"))?;
    validate_settings_json(&settings_json)?;
    if manifest.counts != data.counts() {
        bail!("workspace archive manifest counts do not match its data");
    }
    validate_archive_data(&data, &entries)?;

    for note in &mut data.notes {
        let bytes = entries
            .get(&note.content_path)
            .ok_or_else(|| anyhow::anyhow!("workspace archive is missing {}", note.content_path))?;
        note.content = String::from_utf8(bytes.clone())
            .with_context(|| format!("{} is not valid UTF-8", note.content_path))?;
    }
    for attachment in &mut data.note_attachments {
        attachment.bytes = entries
            .get(&attachment.archive_path)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!("workspace archive is missing {}", attachment.archive_path)
            })?;
    }
    for attachment in &mut data.entry_attachments {
        if let Some(path) = &attachment.archive_path {
            attachment.bytes = entries
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("workspace archive is missing {path}"))?;
        }
    }

    Ok((data, settings_json))
}

fn parse_archive_json<T: for<'de> Deserialize<'de>>(
    entries: &HashMap<String, Vec<u8>>,
    path: &str,
) -> Result<T> {
    let bytes = entries
        .get(path)
        .ok_or_else(|| anyhow::anyhow!("workspace archive is missing {path}"))?;
    serde_json::from_slice(bytes).with_context(|| format!("{path} is not valid JSON"))
}

fn is_supported_archive_entry(path: &str) -> bool {
    path == MANIFEST_PATH
        || path == WORKSPACE_DATA_PATH
        || path == SETTINGS_PATH
        || path.starts_with("notes/")
        || path.starts_with("attachments/notes/")
        || path.starts_with("attachments/entries/")
}

fn validate_archive_path(path: &str) -> Result<()> {
    if path.is_empty() || path.contains('\\') || path.contains('\0') || path.starts_with('/') {
        bail!("workspace archive contains an unsafe path: {path:?}");
    }
    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." || component.contains(':')
        {
            bail!("workspace archive contains an unsafe path: {path:?}");
        }
    }
    Ok(())
}

fn validate_archive_data(data: &ArchiveData, entries: &HashMap<String, Vec<u8>>) -> Result<()> {
    let project_ids = validate_id_set("project", data.projects.iter().map(|item| item.id))?;
    let board_ids = validate_id_set("board", data.boards.iter().map(|item| item.id))?;
    let list_ids = validate_id_set("list", data.lists.iter().map(|item| item.id))?;
    let entry_ids = validate_id_set("entry", data.entries.iter().map(|item| item.id))?;
    let note_ids = validate_id_set("note", data.notes.iter().map(|item| item.id))?;
    let board_label_ids =
        validate_id_set("board label", data.board_labels.iter().map(|item| item.id))?;
    let _ = validate_id_set("entry label", data.entry_labels.iter().map(|item| item.id))?;
    let _ = validate_id_set(
        "entry attachment",
        data.entry_attachments.iter().map(|item| item.id),
    )?;
    let _ = validate_id_set(
        "checklist item",
        data.checklist_items.iter().map(|item| item.id),
    )?;
    let property_ids = validate_id_set(
        "board property",
        data.board_properties.iter().map(|item| item.id),
    )?;
    let option_ids = validate_id_set(
        "property option",
        data.property_options.iter().map(|item| item.id),
    )?;
    let view_ids = validate_id_set("saved view", data.saved_views.iter().map(|item| item.id))?;
    let _ = validate_id_set("template", data.templates.iter().map(|item| item.id))?;
    let alias_ids = validate_id_set("note alias", data.note_aliases.iter().map(|item| item.id))?;
    let _ = validate_id_set(
        "reference alias",
        data.reference_aliases.iter().map(|item| item.id),
    )?;

    for board in &data.boards {
        validate_optional_reference("board project", board.project_id, &project_ids)?;
        if board.last_selected_view_id != 0 {
            require_reference(
                "board selected view",
                board.last_selected_view_id,
                &view_ids,
            )?;
        }
    }
    for list in &data.lists {
        require_reference("list board", list.board_id, &board_ids)?;
    }
    for item in &data.entries {
        require_reference("entry list", item.list_id, &list_ids)?;
    }
    let mut note_content_paths = HashSet::new();
    for note in &data.notes {
        validate_optional_reference("note project", note.project_id, &project_ids)?;
        validate_archive_path(&note.content_path)?;
        if !note.content_path.starts_with("notes/") {
            bail!("note {} has a content path outside notes/", note.id);
        }
        if !note_content_paths.insert(&note.content_path) {
            bail!(
                "workspace archive contains duplicate note content path {}",
                note.content_path
            );
        }
        if !entries.contains_key(&note.content_path) {
            bail!("workspace archive is missing {}", note.content_path);
        }
    }
    for label in &data.board_labels {
        require_reference("board label board", label.board_id, &board_ids)?;
    }
    for label in &data.entry_labels {
        require_reference("entry label entry", label.entry_id, &entry_ids)?;
        require_reference(
            "entry label board label",
            label.board_label_id,
            &board_label_ids,
        )?;
    }
    let mut attachment_paths = HashSet::new();
    for attachment in &data.entry_attachments {
        require_reference("entry attachment entry", attachment.entry_id, &entry_ids)?;
        validate_file_name(&attachment.file_name)?;
        if let Some(path) = &attachment.archive_path {
            validate_archive_path(path)?;
            if !path.starts_with(&format!("attachments/entries/n{}/", attachment.entry_id)) {
                bail!(
                    "entry attachment {} has a path outside its entry",
                    attachment.id
                );
            }
            if !attachment_paths.insert(path) {
                bail!("workspace archive contains duplicate attachment {path}");
            }
            if !entries.contains_key(path) {
                bail!("workspace archive is missing {path}");
            }
        }
    }
    for attachment in &data.note_attachments {
        require_reference("note attachment note", attachment.note_id, &note_ids)?;
        validate_file_name(&attachment.file_name)?;
        validate_archive_path(&attachment.archive_path)?;
        if !attachment
            .archive_path
            .starts_with(&format!("attachments/notes/n{}/", attachment.note_id))
        {
            bail!(
                "note attachment for note {} has a path outside its note",
                attachment.note_id
            );
        }
        if !attachment_paths.insert(&attachment.archive_path) {
            bail!(
                "workspace archive contains duplicate attachment {}",
                attachment.archive_path
            );
        }
        if !entries.contains_key(&attachment.archive_path) {
            bail!("workspace archive is missing {}", attachment.archive_path);
        }
    }
    for item in &data.checklist_items {
        require_reference("checklist entry", item.entry_id, &entry_ids)?;
    }
    for property in &data.board_properties {
        require_reference("property board", property.board_id, &board_ids)?;
    }
    for option in &data.property_options {
        require_reference(
            "property option property",
            option.property_id,
            &property_ids,
        )?;
    }
    for value in &data.property_values {
        require_reference("property value entry", value.entry_id, &entry_ids)?;
        require_reference("property value property", value.property_id, &property_ids)?;
        validate_optional_reference("property value option", value.option_id, &option_ids)?;
    }
    for view in &data.saved_views {
        require_reference("saved view board", view.board_id, &board_ids)?;
    }
    for alias in &data.note_aliases {
        require_reference("note alias note", alias.note_id, &note_ids)?;
    }
    let mut note_link_keys = HashSet::new();
    for link in &data.note_links {
        require_reference("note link source", link.source_note_id, &note_ids)?;
        validate_optional_reference("note link target", link.target_note_id, &note_ids)?;
        if !note_link_keys.insert((link.source_note_id, link.ordinal)) {
            bail!(
                "workspace archive contains duplicate note link {}:{}",
                link.source_note_id,
                link.ordinal
            );
        }
    }
    let mut workspace_link_keys = HashSet::new();
    for link in &data.workspace_links {
        let source_count = usize::from(link.source_note_id.is_some())
            + usize::from(link.source_entry_id.is_some());
        if source_count != 1 {
            bail!("workspace link must have exactly one source");
        }
        validate_optional_reference("workspace link source note", link.source_note_id, &note_ids)?;
        validate_optional_reference(
            "workspace link source entry",
            link.source_entry_id,
            &entry_ids,
        )?;

        let target_count = usize::from(link.target_note_id.is_some())
            + usize::from(link.target_board_id.is_some())
            + usize::from(link.target_list_id.is_some())
            + usize::from(link.target_entry_id.is_some());
        if target_count != 1 {
            bail!("workspace link must have exactly one target");
        }
        validate_optional_reference("workspace link target note", link.target_note_id, &note_ids)?;
        validate_optional_reference(
            "workspace link target board",
            link.target_board_id,
            &board_ids,
        )?;
        validate_optional_reference("workspace link target list", link.target_list_id, &list_ids)?;
        validate_optional_reference(
            "workspace link target entry",
            link.target_entry_id,
            &entry_ids,
        )?;
        if link.target_saved_view_id.is_some() && link.target_board_id.is_none() {
            bail!("workspace link saved view target must also target a board");
        }
        validate_optional_reference(
            "workspace link saved view",
            link.target_saved_view_id,
            &view_ids,
        )?;
        if link.origin != "manual"
            && link.origin != "note_wikilink"
            && link.origin != "entry_wikilink"
            && link.origin != "embed"
        {
            bail!("workspace link has an unsupported origin");
        }
        let source = (link.source_note_id, link.source_entry_id);
        if !workspace_link_keys.insert((source, link.ordinal, link.origin.clone())) {
            bail!("workspace archive contains a duplicate workspace link");
        }
    }
    for alias in &data.reference_aliases {
        let target_count = usize::from(alias.project_id.is_some())
            + usize::from(alias.board_id.is_some())
            + usize::from(alias.list_id.is_some())
            + usize::from(alias.entry_id.is_some())
            + usize::from(alias.saved_view_id.is_some());
        if target_count != 1 {
            bail!("reference alias must have exactly one target");
        }
        validate_optional_reference("reference alias project", alias.project_id, &project_ids)?;
        validate_optional_reference("reference alias board", alias.board_id, &board_ids)?;
        validate_optional_reference("reference alias list", alias.list_id, &list_ids)?;
        validate_optional_reference("reference alias entry", alias.entry_id, &entry_ids)?;
        validate_optional_reference("reference alias saved view", alias.saved_view_id, &view_ids)?;
        let _ = alias_ids.contains(&alias.id);
    }

    Ok(())
}

fn validate_id_set(kind: &str, ids: impl IntoIterator<Item = i64>) -> Result<HashSet<i64>> {
    let mut result = HashSet::new();
    for id in ids {
        if id <= 0 {
            bail!("{kind} ids must be positive");
        }
        if !result.insert(id) {
            bail!("workspace archive contains duplicate {kind} id {id}");
        }
    }
    Ok(result)
}

fn require_reference(kind: &str, id: i64, valid_ids: &HashSet<i64>) -> Result<()> {
    if !valid_ids.contains(&id) {
        bail!("{kind} references missing id {id}");
    }
    Ok(())
}

fn validate_optional_reference(
    kind: &str,
    id: Option<i64>,
    valid_ids: &HashSet<i64>,
) -> Result<()> {
    if let Some(id) = id {
        require_reference(kind, id, valid_ids)?;
    }
    Ok(())
}

fn validate_file_name(file_name: &str) -> Result<()> {
    if file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.len() > MAX_FILE_NAME_BYTES
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name.contains('\0')
        || file_name.ends_with('.')
        || file_name.ends_with(' ')
        || portable_file_name(file_name) != file_name
        || is_reserved_windows_file_name(file_name)
    {
        bail!("workspace archive contains an unsafe attachment file name");
    }
    Ok(())
}

fn is_reserved_windows_file_name(file_name: &str) -> bool {
    let stem = file_name
        .split_once('.')
        .map_or(file_name, |(stem, _)| stem)
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

pub async fn import_workspace(
    db: &(impl ConnectionTrait + TransactionTrait<Transaction = DatabaseTransaction>),
    data_dir: &Path,
    archive_path: &Path,
    mode: ImportMode,
) -> Result<WorkspaceImportSummary> {
    let (data, settings_json) = read_archive(archive_path)?;
    let transaction = db.begin().await?;
    let mut created_paths = Vec::new();
    let result = import_into_transaction(
        &transaction,
        data_dir,
        data,
        settings_json,
        mode,
        &mut created_paths,
    )
    .await;

    match result {
        Ok(summary) => match transaction.commit().await {
            Ok(()) => Ok(summary),
            Err(error) => {
                cleanup_created_files(&created_paths);
                Err(error.into())
            }
        },
        Err(error) => {
            let _ = transaction.rollback().await;
            cleanup_created_files(&created_paths);
            Err(error)
        }
    }
}

async fn import_into_transaction(
    transaction: &DatabaseTransaction,
    data_dir: &Path,
    data: ArchiveData,
    settings_json: Vec<u8>,
    mode: ImportMode,
    created_paths: &mut Vec<PathBuf>,
) -> Result<WorkspaceImportSummary> {
    if mode == ImportMode::Replace {
        clear_workspace(transaction).await?;
    }

    let counts = data.counts();
    let mut project_ids = HashMap::new();
    for item in &data.projects {
        let inserted = project::ActiveModel {
            name: Set(item.name.clone()),
            folder_path: Set(None),
            archived: Set(item.archived),
            position: Set(item.position),
            deleted_at: Set(item.deleted_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
        project_ids.insert(item.id, inserted.id);
    }

    let mut board_ids = HashMap::new();
    for item in &data.boards {
        let inserted = board::ActiveModel {
            title: Set(item.title.clone()),
            project_id: Set(mapped_optional(
                &project_ids,
                item.project_id,
                "board project",
            )?),
            is_pinned: Set(item.is_pinned),
            last_opened_at: Set(item.last_opened_at),
            last_selected_view_id: Set(0),
            deleted_at: Set(item.deleted_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
        board_ids.insert(item.id, inserted.id);
    }

    let mut list_ids = HashMap::new();
    for item in &data.lists {
        let inserted = card::ActiveModel {
            title: Set(item.title.clone()),
            board_id: Set(mapped_id(&board_ids, item.board_id, "list board")?),
            position: Set(item.position),
            deleted_at: Set(item.deleted_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
        list_ids.insert(item.id, inserted.id);
    }

    let mut entry_ids = HashMap::new();
    for item in &data.entries {
        let inserted = entry::ActiveModel {
            title: Set(item.title.clone()),
            description: Set(String::new()),
            card_id: Set(mapped_id(&list_ids, item.list_id, "entry list")?),
            position: Set(item.position),
            due_on: Set(item.due_on.clone()),
            reminder_enabled: Set(item.reminder_enabled),
            reminder_notified_for: Set(item.reminder_notified_for.clone()),
            deleted_at: Set(item.deleted_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
        entry_ids.insert(item.id, inserted.id);
    }

    let mut note_ids = HashMap::new();
    for item in &data.notes {
        let inserted = note::ActiveModel {
            title: Set(item.title.clone()),
            project_id: Set(mapped_optional(
                &project_ids,
                item.project_id,
                "note project",
            )?),
            file_path: Set(None),
            file_managed_by_app: Set(true),
            cached_content: Set(String::new()),
            file_missing_since: Set(None),
            created_at: Set(item.created_at),
            updated_at: Set(item.updated_at),
            is_pinned: Set(item.is_pinned),
            last_opened_at: Set(item.last_opened_at),
            deleted_at: Set(item.deleted_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
        note_ids.insert(item.id, inserted.id);
    }

    let mut note_attachment_replacements = HashMap::new();
    for attachment in &data.note_attachments {
        let target_note_id = mapped_id(&note_ids, attachment.note_id, "note attachment note")?;
        let target_file_name = write_imported_file(
            &data_dir
                .join("attachments")
                .join(target_note_id.to_string()),
            &attachment.file_name,
            &attachment.bytes,
            created_paths,
        )?;
        note_attachment_replacements.insert(
            attachment.archive_path.clone(),
            format!("attachments/{target_note_id}/{target_file_name}"),
        );
    }

    let mut entry_attachment_names = HashMap::new();
    let mut entry_attachment_replacements = HashMap::new();
    for attachment in &data.entry_attachments {
        let target_entry_id = mapped_id(&entry_ids, attachment.entry_id, "entry attachment entry")?;
        let target_file_name = if attachment.archive_path.is_some() {
            write_imported_file(
                &data_dir
                    .join("attachments")
                    .join("entries")
                    .join(target_entry_id.to_string()),
                &attachment.file_name,
                &attachment.bytes,
                created_paths,
            )?
        } else {
            portable_file_name(&attachment.file_name)
        };
        entry_attachment_names.insert(attachment.id, target_file_name.clone());
        if let Some(archive_path) = &attachment.archive_path {
            entry_attachment_replacements.insert(
                archive_path.clone(),
                format!("attachments/entries/{target_entry_id}/{target_file_name}"),
            );
        }
    }

    for item in &data.notes {
        let target_note_id = mapped_id(&note_ids, item.id, "note")?;
        let content = rewrite_archive_references(
            &item.content,
            &note_attachment_replacements,
            &entry_attachment_replacements,
            true,
        );
        let file_name = Path::new(&item.content_path)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(portable_file_name)
            .unwrap_or_else(|| format!("note-{target_note_id}.md"));
        let actual_file_name = write_imported_file(
            &data_dir.join("notes"),
            &file_name,
            content.as_bytes(),
            created_paths,
        )?;
        let file_path = data_dir.join("notes").join(actual_file_name);
        note::ActiveModel {
            id: Set(target_note_id),
            file_path: Set(Some(file_path.to_string_lossy().into_owned())),
            file_managed_by_app: Set(true),
            cached_content: Set(content),
            file_missing_since: Set(None),
            ..Default::default()
        }
        .update(transaction)
        .await?;
    }

    for item in &data.entries {
        let target_entry_id = mapped_id(&entry_ids, item.id, "entry")?;
        let description = rewrite_archive_references(
            &item.description,
            &note_attachment_replacements,
            &entry_attachment_replacements,
            false,
        );
        entry::ActiveModel {
            id: Set(target_entry_id),
            description: Set(description),
            ..Default::default()
        }
        .update(transaction)
        .await?;
    }

    let mut board_label_ids = HashMap::new();
    for item in &data.board_labels {
        let inserted = board_label::ActiveModel {
            board_id: Set(mapped_id(&board_ids, item.board_id, "board label board")?),
            name: Set(item.name.clone()),
            color: Set(item.color.clone()),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
        board_label_ids.insert(item.id, inserted.id);
    }

    for item in &data.entry_labels {
        entry_label::ActiveModel {
            entry_id: Set(mapped_id(&entry_ids, item.entry_id, "entry label entry")?),
            board_label_id: Set(mapped_id(
                &board_label_ids,
                item.board_label_id,
                "entry label board label",
            )?),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
    }

    for item in &data.entry_attachments {
        entry_attachment::ActiveModel {
            entry_id: Set(mapped_id(
                &entry_ids,
                item.entry_id,
                "entry attachment entry",
            )?),
            file_name: Set(entry_attachment_names
                .get(&item.id)
                .cloned()
                .unwrap_or_else(|| portable_file_name(&item.file_name))),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
    }

    for item in &data.checklist_items {
        entry_checklist_item::ActiveModel {
            entry_id: Set(mapped_id(&entry_ids, item.entry_id, "checklist entry")?),
            title: Set(item.title.clone()),
            checked: Set(item.checked),
            position: Set(item.position),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
    }

    let mut property_ids = HashMap::new();
    for item in &data.board_properties {
        let inserted = board_property::ActiveModel {
            board_id: Set(mapped_id(&board_ids, item.board_id, "property board")?),
            name: Set(item.name.clone()),
            kind: Set(item.kind.clone()),
            position: Set(item.position),
            deleted_at: Set(item.deleted_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
        property_ids.insert(item.id, inserted.id);
    }

    let mut option_ids = HashMap::new();
    for item in &data.property_options {
        let inserted = board_property_option::ActiveModel {
            property_id: Set(mapped_id(
                &property_ids,
                item.property_id,
                "property option property",
            )?),
            name: Set(item.name.clone()),
            color: Set(item.color.clone()),
            position: Set(item.position),
            deleted_at: Set(item.deleted_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
        option_ids.insert(item.id, inserted.id);
    }

    for item in &data.property_values {
        entry_property_value::ActiveModel {
            entry_id: Set(mapped_id(
                &entry_ids,
                item.entry_id,
                "property value entry",
            )?),
            property_id: Set(mapped_id(
                &property_ids,
                item.property_id,
                "property value property",
            )?),
            text_value: Set(item.text_value.clone()),
            number_value: Set(item.number_value),
            boolean_value: Set(item.boolean_value),
            date_value: Set(item.date_value.clone()),
            option_id: Set(mapped_optional(
                &option_ids,
                item.option_id,
                "property value option",
            )?),
        }
        .insert(transaction)
        .await?;
    }

    let mut view_ids = HashMap::new();
    for item in &data.saved_views {
        let inserted = saved_board_view::ActiveModel {
            board_id: Set(mapped_id(&board_ids, item.board_id, "saved view board")?),
            name: Set(item.name.clone()),
            position: Set(item.position),
            is_default: Set(item.is_default),
            config_version: Set(item.config_version),
            config_json: Set(item.config_json.clone()),
            deleted_at: Set(item.deleted_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
        view_ids.insert(item.id, inserted.id);
    }

    for item in &data.boards {
        if item.last_selected_view_id == 0 {
            continue;
        }
        board::ActiveModel {
            id: Set(mapped_id(&board_ids, item.id, "board")?),
            last_selected_view_id: Set(mapped_id(
                &view_ids,
                item.last_selected_view_id,
                "board selected view",
            )?),
            ..Default::default()
        }
        .update(transaction)
        .await?;
    }

    for item in &data.templates {
        board_template::ActiveModel {
            name: Set(item.name.clone()),
            description: Set(item.description.clone()),
            definition_json: Set(item.definition_json.clone()),
            created_at: Set(item.created_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
    }

    for item in &data.note_aliases {
        note_alias::ActiveModel {
            note_id: Set(mapped_id(&note_ids, item.note_id, "note alias note")?),
            alias: Set(item.alias.clone()),
            normalized_alias: Set(item.normalized_alias.clone()),
            created_at: Set(item.created_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
    }

    for item in &data.note_links {
        note_link::ActiveModel {
            source_note_id: Set(mapped_id(
                &note_ids,
                item.source_note_id,
                "note link source",
            )?),
            ordinal: Set(item.ordinal),
            target_note_id: Set(mapped_optional(
                &note_ids,
                item.target_note_id,
                "note link target",
            )?),
            raw_target: Set(item.raw_target.clone()),
            display_text: Set(item.display_text.clone()),
            start_byte: Set(item.start_byte),
            end_byte: Set(item.end_byte),
            line_number: Set(item.line_number),
        }
        .insert(transaction)
        .await?;
    }

    for item in &data.reference_aliases {
        workspace_reference_alias::ActiveModel {
            alias: Set(item.alias.clone()),
            normalized_alias: Set(item.normalized_alias.clone()),
            project_id: Set(mapped_optional(
                &project_ids,
                item.project_id,
                "reference alias project",
            )?),
            board_id: Set(mapped_optional(
                &board_ids,
                item.board_id,
                "reference alias board",
            )?),
            list_id: Set(mapped_optional(
                &list_ids,
                item.list_id,
                "reference alias list",
            )?),
            card_id: Set(mapped_optional(
                &entry_ids,
                item.entry_id,
                "reference alias entry",
            )?),
            saved_view_id: Set(mapped_optional(
                &view_ids,
                item.saved_view_id,
                "reference alias saved view",
            )?),
            created_at: Set(item.created_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
    }

    for item in &data.workspace_links {
        workspace_link::ActiveModel {
            source_note_id: Set(mapped_optional(
                &note_ids,
                item.source_note_id,
                "workspace link source note",
            )?),
            source_entry_id: Set(mapped_optional(
                &entry_ids,
                item.source_entry_id,
                "workspace link source entry",
            )?),
            target_note_id: Set(mapped_optional(
                &note_ids,
                item.target_note_id,
                "workspace link target note",
            )?),
            target_board_id: Set(mapped_optional(
                &board_ids,
                item.target_board_id,
                "workspace link target board",
            )?),
            target_card_id: Set(mapped_optional(
                &list_ids,
                item.target_list_id,
                "workspace link target list",
            )?),
            target_entry_id: Set(mapped_optional(
                &entry_ids,
                item.target_entry_id,
                "workspace link target entry",
            )?),
            target_saved_view_id: Set(mapped_optional(
                &view_ids,
                item.target_saved_view_id,
                "workspace link saved view",
            )?),
            origin: Set(item.origin.clone()),
            ordinal: Set(item.ordinal),
            raw_target: Set(item.raw_target.clone()),
            display_text: Set(item.display_text.clone()),
            start_byte: Set(item.start_byte),
            end_byte: Set(item.end_byte),
            line_number: Set(item.line_number),
            created_at: Set(item.created_at),
            ..Default::default()
        }
        .insert(transaction)
        .await?;
    }

    refresh_imported_link_offsets(
        transaction,
        &data,
        &note_ids,
        &entry_ids,
        &note_attachment_replacements,
        &entry_attachment_replacements,
    )
    .await?;

    for item in &data.notes {
        note_link_index_state::ActiveModel {
            note_id: Set(mapped_id(&note_ids, item.id, "note link index note")?),
            indexed_updated_at: Set(item.updated_at),
        }
        .insert(transaction)
        .await?;
        workspace_link_index_state::ActiveModel {
            source_kind: Set("note".to_string()),
            source_id: Set(mapped_id(&note_ids, item.id, "workspace link index note")?),
            indexed_content: Set(rewrite_archive_references(
                &item.content,
                &note_attachment_replacements,
                &entry_attachment_replacements,
                true,
            )),
        }
        .insert(transaction)
        .await?;
    }
    for item in &data.entries {
        workspace_link_index_state::ActiveModel {
            source_kind: Set("entry".to_string()),
            source_id: Set(mapped_id(
                &entry_ids,
                item.id,
                "workspace link index entry",
            )?),
            indexed_content: Set(rewrite_archive_references(
                &item.description,
                &note_attachment_replacements,
                &entry_attachment_replacements,
                false,
            )),
        }
        .insert(transaction)
        .await?;
    }

    crate::workspace::search::rebuild_search_index(transaction)
        .await
        .context("could not rebuild the workspace search index")?;
    touch_change_revision(transaction).await?;

    let mut warnings = Vec::new();
    for attachment in &data.entry_attachments {
        if attachment.archive_path.is_none() {
            warnings.push(format!(
                "Attachment '{}' for entry {} was not present in the export.",
                attachment.file_name, attachment.entry_id
            ));
        }
    }

    Ok(WorkspaceImportSummary {
        counts,
        mode,
        settings_json,
        warnings,
    })
}

#[derive(Clone, Debug)]
struct ImportedLinkPosition {
    raw_target: String,
    display_text: Option<String>,
    start_byte: i64,
    end_byte: i64,
    line_number: i32,
}

fn parsed_link_positions(content: &str, workspace_only: bool) -> Vec<ImportedLinkPosition> {
    let embed_ranges = crate::board::projection::parse_board_view_embeds(content)
        .into_iter()
        .map(|embed| embed.start_byte..embed.end_byte)
        .collect::<Vec<_>>();
    crate::note::links::parse_wikilinks(content)
        .into_iter()
        .filter(|link| {
            (!workspace_only || crate::workspace::links::is_workspace_target(&link.raw_target))
                && !embed_ranges
                    .iter()
                    .any(|range| range.contains(&link.start_byte))
        })
        .map(|link| ImportedLinkPosition {
            raw_target: link.raw_target,
            display_text: link.display_text,
            start_byte: link.start_byte as i64,
            end_byte: link.end_byte as i64,
            line_number: link.line_number as i32,
        })
        .collect()
}

fn parsed_embed_positions(content: &str) -> Vec<ImportedLinkPosition> {
    crate::board::projection::parse_board_view_embeds(content)
        .into_iter()
        .map(|embed| ImportedLinkPosition {
            raw_target: embed.raw_target,
            display_text: embed.display_text,
            start_byte: embed.start_byte as i64,
            end_byte: embed.end_byte as i64,
            line_number: embed.line_number as i32,
        })
        .collect()
}

async fn refresh_imported_link_offsets(
    transaction: &DatabaseTransaction,
    data: &ArchiveData,
    note_ids: &HashMap<i64, i64>,
    entry_ids: &HashMap<i64, i64>,
    note_attachment_replacements: &HashMap<String, String>,
    entry_attachment_replacements: &HashMap<String, String>,
) -> Result<()> {
    for item in &data.notes {
        let note_id = mapped_id(note_ids, item.id, "note link source")?;
        let content = rewrite_archive_references(
            &item.content,
            note_attachment_replacements,
            entry_attachment_replacements,
            true,
        );
        let note_links = note_link::Entity::find()
            .filter(note_link::Column::SourceNoteId.eq(note_id))
            .order_by_asc(note_link::Column::Ordinal)
            .all(transaction)
            .await?;
        let positions = parsed_link_positions(&content, false);
        for link in note_links {
            let Some(index) = usize::try_from(link.ordinal).ok() else {
                continue;
            };
            let Some(position) = positions.get(index) else {
                continue;
            };
            let mut active = link.into_active_model();
            active.raw_target = Set(position.raw_target.clone());
            active.display_text = Set(position.display_text.clone());
            active.start_byte = Set(position.start_byte);
            active.end_byte = Set(position.end_byte);
            active.line_number = Set(position.line_number);
            active.update(transaction).await?;
        }

        refresh_workspace_link_offsets(
            transaction,
            Some(note_id),
            None,
            "note_wikilink",
            &parsed_link_positions(&content, true),
        )
        .await?;
        refresh_workspace_link_offsets(
            transaction,
            Some(note_id),
            None,
            "embed",
            &parsed_embed_positions(&content),
        )
        .await?;
    }

    for item in &data.entries {
        let entry_id = mapped_id(entry_ids, item.id, "entry link source")?;
        let description = rewrite_archive_references(
            &item.description,
            note_attachment_replacements,
            entry_attachment_replacements,
            false,
        );
        refresh_workspace_link_offsets(
            transaction,
            None,
            Some(entry_id),
            "entry_wikilink",
            &parsed_link_positions(&description, false),
        )
        .await?;
    }

    Ok(())
}

async fn refresh_workspace_link_offsets(
    transaction: &DatabaseTransaction,
    source_note_id: Option<i64>,
    source_entry_id: Option<i64>,
    origin: &str,
    positions: &[ImportedLinkPosition],
) -> Result<()> {
    let mut query =
        workspace_link::Entity::find().filter(workspace_link::Column::Origin.eq(origin));
    query = match (source_note_id, source_entry_id) {
        (Some(note_id), None) => query.filter(workspace_link::Column::SourceNoteId.eq(note_id)),
        (None, Some(entry_id)) => query.filter(workspace_link::Column::SourceEntryId.eq(entry_id)),
        _ => return Ok(()),
    };

    for link in query
        .order_by_asc(workspace_link::Column::Ordinal)
        .all(transaction)
        .await?
    {
        let Some(index) = usize::try_from(link.ordinal).ok() else {
            continue;
        };
        let Some(position) = positions.get(index) else {
            continue;
        };
        let mut active = link.into_active_model();
        active.raw_target = Set(Some(position.raw_target.clone()));
        active.display_text = Set(position.display_text.clone());
        active.start_byte = Set(Some(position.start_byte));
        active.end_byte = Set(Some(position.end_byte));
        active.line_number = Set(Some(position.line_number));
        active.update(transaction).await?;
    }

    Ok(())
}

fn mapped_id(map: &HashMap<i64, i64>, source_id: i64, kind: &str) -> Result<i64> {
    map.get(&source_id)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("{kind} references missing id {source_id}"))
}

fn mapped_optional(
    map: &HashMap<i64, i64>,
    source_id: Option<i64>,
    kind: &str,
) -> Result<Option<i64>> {
    source_id.map(|id| mapped_id(map, id, kind)).transpose()
}

fn rewrite_archive_references(
    content: &str,
    note_replacements: &HashMap<String, String>,
    entry_replacements: &HashMap<String, String>,
    note_document: bool,
) -> String {
    let mut rewritten = content.to_string();
    for (archive_path, replacement) in note_replacements.iter().chain(entry_replacements.iter()) {
        let path = if note_document {
            format!("../{archive_path}")
        } else {
            archive_path.clone()
        };
        rewritten = replace_path_token(&rewritten, &path, replacement);
        rewritten = replace_path_token(&rewritten, &path.replace('/', "\\"), replacement);
        if note_document {
            rewritten = replace_path_token(&rewritten, archive_path, replacement);
            rewritten =
                replace_path_token(&rewritten, &archive_path.replace('/', "\\"), replacement);
        }
    }
    rewritten
}

fn write_imported_file(
    directory: &Path,
    preferred_name: &str,
    bytes: &[u8],
    created_paths: &mut Vec<PathBuf>,
) -> Result<String> {
    if bytes.len() as u64 > MAX_ARCHIVE_ENTRY_BYTES {
        bail!("imported file is larger than the archive file limit");
    }
    fs::create_dir_all(directory)
        .with_context(|| format!("could not create {}", directory.display()))?;
    let preferred_name = portable_file_name(preferred_name);
    let path = Path::new(&preferred_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("attachment");
    let extension = path.extension().and_then(|extension| extension.to_str());

    for index in 0..10_000_usize {
        let file_name = if index == 0 {
            preferred_name.clone()
        } else {
            match extension {
                Some(extension) => format!("{stem}-{index}.{extension}"),
                None => format!("{stem}-{index}"),
            }
        };
        let target = directory.join(&file_name);
        let mut file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("could not create {}", target.display()));
            }
        };
        created_paths.push(target);
        if let Err(error) = file.write_all(bytes) {
            return Err(error).context("could not write imported workspace file");
        }
        return Ok(file_name);
    }

    bail!("could not find a free file name in {}", directory.display())
}

fn cleanup_created_files(created_paths: &[PathBuf]) {
    for path in created_paths.iter().rev() {
        let _ = fs::remove_file(path);
    }
}

async fn clear_workspace(db: &impl ConnectionTrait) -> Result<()> {
    for table in [
        "workspace_reference_alias",
        "workspace_link",
        "workspace_link_index_state",
        "note_link",
        "note_link_index_state",
        "entry_property_value",
        "entry_label",
        "entry_attachment",
        "entry_checklist_item",
        "saved_board_view",
        "board_property_option",
        "board_property",
        "entry",
        "card",
        "note_alias",
        "board_label",
        "note",
        "board",
        "project",
        "board_template",
    ] {
        db.execute_unprepared(&format!("DELETE FROM {table}"))
            .await?;
    }
    Ok(())
}

async fn touch_change_revision(db: &impl ConnectionTrait) -> Result<()> {
    db.execute_unprepared(
        "UPDATE castle_change_revision
         SET revision = revision + 1,
             board_revision = board_revision + 1,
             note_revision = note_revision + 1,
             link_revision = link_revision + 1
         WHERE id = 1",
    )
    .await?;
    Ok(())
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{
        ActiveModelTrait, ActiveValue::Set, ConnectionTrait, Database, EntityTrait, PaginatorTrait,
    };

    #[test]
    fn attachment_reference_rewrite_respects_path_boundaries() {
        let mut replacements = HashMap::new();
        replacements.insert(
            "attachments/entries/n1/a1-image.png".to_string(),
            "attachments/entries/4/image.png".to_string(),
        );
        let content = "![backup](attachments/entries/n1/a1-image.png.bak) ![image](attachments/entries/n1/a1-image.png)";

        assert_eq!(
            rewrite_archive_references(content, &HashMap::new(), &replacements, false),
            "![backup](attachments/entries/n1/a1-image.png.bak) ![image](attachments/entries/4/image.png)"
        );
    }

    #[tokio::test]
    async fn full_workspace_archive_round_trips_into_a_clean_installation() -> Result<()> {
        let source_db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&source_db, None).await?;
        let source_dir = tempfile::tempdir()?;
        let source_notes_dir = source_dir.path().join("notes");
        fs::create_dir_all(&source_notes_dir)?;

        let project = project::ActiveModel {
            name: Set("Product".to_string()),
            archived: Set(false),
            position: Set(3),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        let board = board::ActiveModel {
            title: Set("Launch plan".to_string()),
            project_id: Set(Some(project.id)),
            is_pinned: Set(true),
            last_opened_at: Set(Some(20)),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        let list = card::ActiveModel {
            title: Set("In progress".to_string()),
            board_id: Set(board.id),
            position: Set(1),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        let entry = entry::ActiveModel {
            title: Set("Ship export".to_string()),
            description: Set(String::new()),
            card_id: Set(list.id),
            position: Set(2),
            due_on: Set(Some("2026-09-03".to_string())),
            reminder_enabled: Set(true),
            reminder_notified_for: Set(Some("2026-09-02".to_string())),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;

        let first_note_file = source_notes_dir.join("first.md");
        let second_note_file = source_notes_dir.join("second.md");
        let first_note = note::ActiveModel {
            title: Set("First note".to_string()),
            project_id: Set(Some(project.id)),
            file_path: Set(Some(first_note_file.to_string_lossy().into_owned())),
            file_managed_by_app: Set(true),
            cached_content: Set(String::new()),
            file_missing_since: Set(None),
            created_at: Set(10),
            updated_at: Set(11),
            is_pinned: Set(true),
            last_opened_at: Set(Some(12)),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        let second_note = note::ActiveModel {
            title: Set("Second note".to_string()),
            project_id: Set(None),
            file_path: Set(Some(second_note_file.to_string_lossy().into_owned())),
            file_managed_by_app: Set(false),
            cached_content: Set("Second body".to_string()),
            file_missing_since: Set(Some(13)),
            created_at: Set(14),
            updated_at: Set(15),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;

        let note_attachment_dir = source_dir
            .path()
            .join("attachments")
            .join(first_note.id.to_string());
        fs::create_dir_all(&note_attachment_dir)?;
        let note_attachment_path = note_attachment_dir.join("image.png");
        fs::write(&note_attachment_path, b"note image")?;
        let first_content = format!(
            "Intro ![image](<{}>) [[Second]]",
            note_attachment_path.to_string_lossy().replace('\\', "/")
        );
        fs::write(&first_note_file, &first_content)?;
        fs::write(&second_note_file, "Second body")?;
        note::ActiveModel {
            id: Set(first_note.id),
            cached_content: Set(first_content.clone()),
            ..Default::default()
        }
        .update(&source_db)
        .await?;

        let entry_attachment_dir = source_dir
            .path()
            .join("attachments")
            .join("entries")
            .join(entry.id.to_string());
        fs::create_dir_all(&entry_attachment_dir)?;
        fs::write(entry_attachment_dir.join("card.png"), b"entry image")?;
        let entry_description = format!("![card](attachments/entries/{}/card.png)", entry.id);
        entry::ActiveModel {
            id: Set(entry.id),
            description: Set(entry_description),
            ..Default::default()
        }
        .update(&source_db)
        .await?;

        let label = board_label::ActiveModel {
            board_id: Set(board.id),
            name: Set("Important".to_string()),
            color: Set("red".to_string()),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        entry_label::ActiveModel {
            entry_id: Set(entry.id),
            board_label_id: Set(label.id),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        entry_checklist_item::ActiveModel {
            entry_id: Set(entry.id),
            title: Set("Verify archive".to_string()),
            checked: Set(true),
            position: Set(4),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        let entry_attachment = entry_attachment::ActiveModel {
            entry_id: Set(entry.id),
            file_name: Set("card.png".to_string()),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;

        let property = board_property::ActiveModel {
            board_id: Set(board.id),
            name: Set("Status".to_string()),
            kind: Set("select".to_string()),
            position: Set(0),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        let option = board_property_option::ActiveModel {
            property_id: Set(property.id),
            name: Set("Ready".to_string()),
            color: Set("green".to_string()),
            position: Set(0),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        entry_property_value::ActiveModel {
            entry_id: Set(entry.id),
            property_id: Set(property.id),
            text_value: Set(Some("ready".to_string())),
            number_value: Set(Some(2.5)),
            boolean_value: Set(Some(true)),
            date_value: Set(Some("2026-09-03".to_string())),
            option_id: Set(Some(option.id)),
        }
        .insert(&source_db)
        .await?;
        let view = saved_board_view::ActiveModel {
            board_id: Set(board.id),
            name: Set("Planning".to_string()),
            position: Set(0),
            is_default: Set(true),
            config_version: Set(2),
            config_json: Set("{\"group\":\"status\"}".to_string()),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        board::ActiveModel {
            id: Set(board.id),
            last_selected_view_id: Set(view.id),
            ..Default::default()
        }
        .update(&source_db)
        .await?;

        board_template::ActiveModel {
            name: Set("Launch template".to_string()),
            description: Set("Reusable launch board".to_string()),
            definition_json: Set("{\"columns\":[]}".to_string()),
            created_at: Set(16),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        note_alias::ActiveModel {
            note_id: Set(first_note.id),
            alias: Set("First".to_string()),
            normalized_alias: Set("first".to_string()),
            created_at: Set(17),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        note_link::ActiveModel {
            source_note_id: Set(first_note.id),
            ordinal: Set(0),
            target_note_id: Set(Some(second_note.id)),
            raw_target: Set("Second".to_string()),
            display_text: Set(Some("Second".to_string())),
            start_byte: Set(33),
            end_byte: Set(43),
            line_number: Set(1),
        }
        .insert(&source_db)
        .await?;
        workspace_link::ActiveModel {
            source_note_id: Set(Some(first_note.id)),
            target_board_id: Set(Some(board.id)),
            target_saved_view_id: Set(Some(view.id)),
            origin: Set("manual".to_string()),
            ordinal: Set(0),
            raw_target: Set(Some("Launch plan".to_string())),
            display_text: Set(Some("Launch plan".to_string())),
            start_byte: Set(None),
            end_byte: Set(None),
            line_number: Set(None),
            created_at: Set(18),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        workspace_reference_alias::ActiveModel {
            alias: Set("In progress".to_string()),
            normalized_alias: Set("in progress".to_string()),
            list_id: Set(Some(list.id)),
            created_at: Set(19),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;

        let settings_json = br#"{"theme_name":"Gruvbox Dark","show_sidebar":false}"#;
        let archive_path = source_dir.path().join("workspace.castle.zip");
        let export =
            export_workspace(&source_db, source_dir.path(), settings_json, &archive_path).await?;
        assert_eq!(export.missing_attachments, 0);
        assert_eq!(
            export.counts,
            WorkspaceArchiveCounts {
                projects: 1,
                boards: 1,
                lists: 1,
                entries: 1,
                notes: 2,
                board_labels: 1,
                entry_labels: 1,
                entry_attachments: 1,
                note_attachments: 1,
                checklist_items: 1,
                board_properties: 1,
                property_options: 1,
                property_values: 1,
                saved_views: 1,
                templates: 1,
                note_aliases: 1,
                note_links: 1,
                workspace_links: 1,
                reference_aliases: 1,
            }
        );

        let zip_file = File::open(&archive_path)?;
        let mut zip = ZipArchive::new(zip_file)?;
        assert!(zip.by_name(MANIFEST_PATH).is_ok());
        assert!(zip.by_name(WORKSPACE_DATA_PATH).is_ok());
        assert!(zip.by_name(SETTINGS_PATH).is_ok());
        assert!(
            zip.by_name(&format!("attachments/notes/n{}/image.png", first_note.id))
                .is_ok()
        );
        assert!(
            zip.by_name(&format!(
                "attachments/entries/n{}/a{}-card.png",
                entry.id, entry_attachment.id
            ))
            .is_ok()
        );
        let mut workspace_json = String::new();
        zip.by_name(WORKSPACE_DATA_PATH)?
            .read_to_string(&mut workspace_json)?;
        assert!(workspace_json.contains("attachments/entries/"));
        let note_path = zip
            .file_names()
            .find(|path| path.starts_with("notes/"))
            .map(str::to_string)
            .context("workspace archive should contain a Markdown note")?;
        let mut note_archive_content = String::new();
        zip.by_name(&note_path)?
            .read_to_string(&mut note_archive_content)?;
        assert!(note_archive_content.contains("../attachments/notes/"));

        let target_db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&target_db, None).await?;
        let target_dir = tempfile::tempdir()?;
        project::ActiveModel {
            name: Set("Starter workspace".to_string()),
            ..Default::default()
        }
        .insert(&target_db)
        .await?;

        let imported = import_workspace(
            &target_db,
            target_dir.path(),
            &archive_path,
            ImportMode::Replace,
        )
        .await?;
        assert_eq!(imported.mode, ImportMode::Replace);
        assert_eq!(imported.settings_json, settings_json);
        assert!(imported.warnings.is_empty());
        assert_eq!(imported.counts, export.counts);

        let projects = project::Entity::find().all(&target_db).await?;
        let boards = board::Entity::find().all(&target_db).await?;
        let lists = card::Entity::find().all(&target_db).await?;
        let entries = entry::Entity::find().all(&target_db).await?;
        let notes = note::Entity::find()
            .order_by_asc(note::Column::Id)
            .all(&target_db)
            .await?;
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "Product");
        assert_eq!(projects[0].folder_path, None);
        assert_eq!(boards.len(), 1);
        assert_eq!(lists.len(), 1);
        assert_eq!(entries.len(), 1);
        assert_eq!(notes.len(), 2);
        assert_eq!(
            entries[0].description,
            format!("![card](attachments/entries/{}/card.png)", entries[0].id)
        );
        assert_eq!(
            notes[0].cached_content,
            format!(
                "Intro ![image](<attachments/{}/image.png>) [[Second]]",
                notes[0].id
            )
        );
        assert!(notes[0].file_managed_by_app);
        let imported_note_path = notes[0]
            .file_path
            .as_ref()
            .map(PathBuf::from)
            .context("imported note should have a file path")?;
        assert_eq!(
            fs::read(imported_note_path)?,
            notes[0].cached_content.as_bytes()
        );
        assert_eq!(
            fs::read(
                target_dir
                    .path()
                    .join("attachments")
                    .join(notes[0].id.to_string())
                    .join("image.png")
            )?,
            b"note image"
        );
        assert_eq!(
            fs::read(
                target_dir
                    .path()
                    .join("attachments")
                    .join("entries")
                    .join(entries[0].id.to_string())
                    .join("card.png")
            )?,
            b"entry image"
        );
        assert!(
            board::Entity::find_by_id(boards[0].id)
                .one(&target_db)
                .await?
                .context("imported board should exist")?
                .last_selected_view_id
                > 0
        );
        assert_eq!(board_label::Entity::find().count(&target_db).await?, 1);
        assert_eq!(entry_label::Entity::find().count(&target_db).await?, 1);
        assert_eq!(
            entry_checklist_item::Entity::find()
                .count(&target_db)
                .await?,
            1
        );
        assert_eq!(entry_attachment::Entity::find().count(&target_db).await?, 1);
        assert_eq!(board_property::Entity::find().count(&target_db).await?, 1);
        assert_eq!(
            board_property_option::Entity::find()
                .count(&target_db)
                .await?,
            1
        );
        assert_eq!(
            entry_property_value::Entity::find()
                .count(&target_db)
                .await?,
            1
        );
        assert_eq!(saved_board_view::Entity::find().count(&target_db).await?, 1);
        assert_eq!(board_template::Entity::find().count(&target_db).await?, 1);
        assert_eq!(note_alias::Entity::find().count(&target_db).await?, 1);
        assert_eq!(note_link::Entity::find().count(&target_db).await?, 1);
        let imported_note_link = note_link::Entity::find()
            .all(&target_db)
            .await?
            .into_iter()
            .next()
            .context("imported note link should exist")?;
        let link_start = notes[0]
            .cached_content
            .find("[[Second]]")
            .context("imported note link should be present in the note")?;
        assert_eq!(imported_note_link.start_byte, link_start as i64);
        assert_eq!(
            imported_note_link.end_byte,
            (link_start + "[[Second]]".len()) as i64
        );
        assert_eq!(workspace_link::Entity::find().count(&target_db).await?, 1);
        assert_eq!(
            workspace_reference_alias::Entity::find()
                .count(&target_db)
                .await?,
            1
        );
        assert_eq!(
            note_link_index_state::Entity::find()
                .count(&target_db)
                .await?,
            2
        );
        assert_eq!(
            workspace_link_index_state::Entity::find()
                .count(&target_db)
                .await?,
            3
        );
        let search_results =
            crate::workspace::search::search_workspace(&target_db, "Ship export", 16).await?;
        assert!(
            search_results
                .iter()
                .any(|result| result.title == "Ship export")
        );
        Ok(())
    }

    #[tokio::test]
    async fn replace_import_removes_existing_content_and_merge_keeps_it() -> Result<()> {
        let source_db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&source_db, None).await?;
        let source_dir = tempfile::tempdir()?;
        project::ActiveModel {
            name: Set("Imported".to_string()),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        let source_archive = source_dir.path().join("workspace.zip");
        export_workspace(&source_db, source_dir.path(), br#"{}"#, &source_archive).await?;

        let target_db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&target_db, None).await?;
        let target_dir = tempfile::tempdir()?;
        project::ActiveModel {
            name: Set("Existing".to_string()),
            ..Default::default()
        }
        .insert(&target_db)
        .await?;
        import_workspace(
            &target_db,
            target_dir.path(),
            &source_archive,
            ImportMode::Merge,
        )
        .await?;
        assert_eq!(project::Entity::find().count(&target_db).await?, 2);

        import_workspace(
            &target_db,
            target_dir.path(),
            &source_archive,
            ImportMode::Replace,
        )
        .await?;
        let projects = project::Entity::find().all(&target_db).await?;
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "Imported");
        Ok(())
    }

    #[tokio::test]
    async fn import_rejects_zip_path_traversal_before_mutating_the_database() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let archive_path = directory.path().join("unsafe.zip");
        let file = File::create(&archive_path)?;
        let mut zip = ZipWriter::new(file);
        zip.start_file(
            "../escape",
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
        )?;
        zip.write_all(b"no")?;
        zip.finish()?;

        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let data_dir = tempfile::tempdir()?;
        let result =
            import_workspace(&db, data_dir.path(), &archive_path, ImportMode::Replace).await;
        assert!(result.is_err());
        assert_eq!(project::Entity::find().count(&db).await?, 0);
        Ok(())
    }

    #[tokio::test]
    async fn failed_import_rolls_back_database_and_created_files() -> Result<()> {
        let source_db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&source_db, None).await?;
        let source_dir = tempfile::tempdir()?;
        note::ActiveModel {
            title: Set("Imported".to_string()),
            cached_content: Set("Portable body".to_string()),
            created_at: Set(1),
            updated_at: Set(1),
            ..Default::default()
        }
        .insert(&source_db)
        .await?;
        let source_archive = source_dir.path().join("workspace.zip");
        export_workspace(&source_db, source_dir.path(), br#"{}"#, &source_archive).await?;

        let target_db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&target_db, None).await?;
        project::ActiveModel {
            name: Set("Existing".to_string()),
            ..Default::default()
        }
        .insert(&target_db)
        .await?;
        target_db
            .execute_unprepared(
                "CREATE TRIGGER fail_workspace_import_note_update
                 AFTER UPDATE OF file_path ON note
                 BEGIN
                     SELECT RAISE(ABORT, 'import test failure');
                 END;",
            )
            .await?;
        let target_dir = tempfile::tempdir()?;

        let result = import_workspace(
            &target_db,
            target_dir.path(),
            &source_archive,
            ImportMode::Replace,
        )
        .await;

        assert!(result.is_err());
        let projects = project::Entity::find().all(&target_db).await?;
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].name, "Existing");
        assert_eq!(note::Entity::find().count(&target_db).await?, 0);
        assert!(
            !target_dir
                .path()
                .join("notes")
                .join("Imported-n1.md")
                .exists()
        );
        Ok(())
    }
}
