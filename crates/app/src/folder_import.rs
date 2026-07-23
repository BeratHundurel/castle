use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use entity::{note, note::Entity as Note, project, project::Entity as Project};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, TransactionTrait,
};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DOCUMENTS: usize = 5_000;

const IGNORED_DIRECTORIES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    ".cache",
    ".next",
    ".nuxt",
    ".turbo",
    ".venv",
    "__pycache__",
    "bower_components",
    "build",
    "coverage",
    "dist",
    "node_modules",
    "out",
    "target",
    "vendor",
    "venv",
];

#[derive(Debug)]
pub(crate) struct FolderDocument {
    title: String,
    path: PathBuf,
    content: String,
}

#[derive(Debug)]
pub(crate) struct FolderScan {
    root: PathBuf,
    project_name: String,
    documents: Vec<FolderDocument>,
    skipped_files: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FolderImportResult {
    pub(crate) project_name: String,
    pub(crate) inserted: usize,
    pub(crate) updated: usize,
    pub(crate) skipped: usize,
    pub(crate) created_project: bool,
}

fn has_supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "markdown" | "txt" | "json"
            )
        })
}

fn should_ignore_directory(name: &str) -> bool {
    let lowercase_name = name.to_ascii_lowercase();
    name.starts_with('.') || IGNORED_DIRECTORIES.contains(&lowercase_name.as_str())
}

pub(crate) fn scan_folder(root: &Path) -> Result<FolderScan> {
    let root = root
        .canonicalize()
        .with_context(|| format!("Could not resolve {}", root.display()))?;

    if !root.is_dir() {
        bail!("{} is not a folder", root.display());
    }

    let project_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Folder project")
        .to_string();

    let mut pending = vec![root.clone()];
    let mut candidates = Vec::new();
    let mut skipped_files = 0usize;

    while let Some(directory) = pending.pop() {
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(_) => {
                skipped_files = skipped_files.saturating_add(1);
                continue;
            }
        };

        for entry in entries {
            let Ok(entry) = entry else {
                skipped_files = skipped_files.saturating_add(1);
                continue;
            };
            let Ok(file_type) = entry.file_type() else {
                skipped_files = skipped_files.saturating_add(1);
                continue;
            };
            let path = entry.path();

            if file_type.is_dir() {
                let name = entry.file_name();
                if name.to_str().is_some_and(should_ignore_directory) {
                    continue;
                }
                pending.push(path);
            } else if file_type.is_file() && has_supported_extension(&path) {
                candidates.push(path);
            }
        }
    }

    candidates.sort_by_cached_key(|path| path.to_string_lossy().to_ascii_lowercase());

    let mut documents = Vec::new();
    let mut total_bytes = 0u64;
    for path in candidates {
        if documents.len() >= MAX_DOCUMENTS {
            skipped_files = skipped_files.saturating_add(1);
            continue;
        }

        let Ok(metadata) = fs::metadata(&path) else {
            skipped_files = skipped_files.saturating_add(1);
            continue;
        };

        let file_bytes = metadata.len();
        if file_bytes > MAX_FILE_BYTES || total_bytes.saturating_add(file_bytes) > MAX_TOTAL_BYTES {
            skipped_files = skipped_files.saturating_add(1);
            continue;
        }

        let Ok(content) = fs::read_to_string(&path) else {
            skipped_files = skipped_files.saturating_add(1);
            continue;
        };

        let title = path
            .strip_prefix(&root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");

        total_bytes = total_bytes.saturating_add(file_bytes);
        documents.push(FolderDocument {
            title,
            path,
            content,
        });
    }

    Ok(FolderScan {
        root,
        project_name,
        documents,
        skipped_files,
    })
}

pub(crate) async fn import_folder(
    db: &DatabaseConnection,
    scan: FolderScan,
) -> Result<FolderImportResult> {
    Ok(db
        .transaction::<_, FolderImportResult, anyhow::Error>(|txn| {
            Box::pin(async move {
                let folder_path = scan.root.to_string_lossy().into_owned();
                let existing_project = Project::find()
                    .filter(project::Column::FolderPath.eq(folder_path.clone()))
                    .one(txn)
                    .await?;

                let (project_id, project_name, created_project) = if let Some(existing) =
                    existing_project
                {
                    if existing.deleted_at.is_some() {
                        bail!(
                            "{} is already a project in Trash; restore it before adding it again",
                            scan.root.display()
                        );
                    }
                    if existing.archived {
                        bail!("{} is already an archived project", scan.root.display());
                    }
                    (existing.id, existing.name, false)
                } else {
                    let position = Project::find().count(txn).await? as i32;
                    let inserted = project::ActiveModel {
                        name: Set(scan.project_name),
                        folder_path: Set(Some(folder_path)),
                        archived: Set(false),
                        position: Set(position),
                        ..Default::default()
                    }
                    .insert(txn)
                    .await?;
                    (inserted.id, inserted.name, true)
                };

                let mut inserted_count = 0usize;
                let mut updated_count = 0usize;
                let mut skipped_count = scan.skipped_files;

                for document in scan.documents {
                    let path_string = document.path.to_string_lossy().into_owned();
                    let existing_note = Note::find()
                        .filter(note::Column::FilePath.eq(path_string.clone()))
                        .one(txn)
                        .await?;

                    if let Some(existing) = existing_note {
                        if existing.deleted_at.is_some()
                            || existing.project_id.is_some_and(|id| id != project_id)
                        {
                            skipped_count = skipped_count.saturating_add(1);
                            continue;
                        }

                        note::ActiveModel {
                            id: Set(existing.id),
                            title: Set(document.title),
                            project_id: Set(Some(project_id)),
                            file_path: Set(Some(path_string)),
                            file_managed_by_app: Set(false),
                            cached_content: Set(document.content),
                            file_missing_since: Set(None),
                            updated_at: Set(crate::document_editor::now_ts()),
                            ..Default::default()
                        }
                        .update(txn)
                        .await?;
                        updated_count = updated_count.saturating_add(1);
                    } else {
                        let now = crate::document_editor::now_ts();
                        note::ActiveModel {
                            title: Set(document.title),
                            project_id: Set(Some(project_id)),
                            file_path: Set(Some(path_string)),
                            file_managed_by_app: Set(false),
                            cached_content: Set(document.content),
                            file_missing_since: Set(None),
                            created_at: Set(now),
                            updated_at: Set(now),
                            ..Default::default()
                        }
                        .insert(txn)
                        .await?;
                        inserted_count = inserted_count.saturating_add(1);
                    }
                }

                Ok(FolderImportResult {
                    project_name,
                    inserted: inserted_count,
                    updated: updated_count,
                    skipped: skipped_count,
                    created_project,
                })
            })
        })
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{Database, EntityTrait};

    #[test]
    fn scan_discovers_supported_text_and_prunes_generated_directories() -> Result<()> {
        let directory = tempfile::tempdir()?;
        fs::create_dir_all(directory.path().join("docs"))?;
        fs::create_dir_all(directory.path().join("node_modules/package"))?;
        fs::create_dir_all(directory.path().join("NODE_MODULES/package"))?;
        fs::create_dir_all(directory.path().join("target/debug"))?;
        fs::create_dir_all(directory.path().join(".git"))?;
        fs::write(directory.path().join("README.md"), "# Read me")?;
        fs::write(directory.path().join("docs/notes.TXT"), "notes")?;
        fs::write(directory.path().join("data.json"), "{\"ok\":true}")?;
        fs::write(directory.path().join("image.png"), "not an image")?;
        fs::write(
            directory.path().join("node_modules/package/readme.md"),
            "ignored",
        )?;
        fs::write(
            directory
                .path()
                .join("NODE_MODULES/package/also-ignored.md"),
            "ignored",
        )?;
        fs::write(directory.path().join("target/debug/build.txt"), "ignored")?;
        fs::write(directory.path().join(".git/config.txt"), "ignored")?;

        let scan = scan_folder(directory.path())?;
        let titles = scan
            .documents
            .iter()
            .map(|document| document.title.as_str())
            .collect::<Vec<_>>();

        assert_eq!(titles, vec!["data.json", "docs/notes.TXT", "README.md"]);
        assert_eq!(scan.skipped_files, 0);

        Ok(())
    }

    #[tokio::test]
    async fn importing_the_same_folder_refreshes_without_duplicates() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;
        let directory = tempfile::tempdir()?;
        let note_path = directory.path().join("notes.md");
        fs::write(&note_path, "first")?;

        let first = import_folder(&db, scan_folder(directory.path())?).await?;
        assert!(first.created_project);
        assert_eq!(first.inserted, 1);

        fs::write(&note_path, "second")?;
        let second = import_folder(&db, scan_folder(directory.path())?).await?;
        assert!(!second.created_project);
        assert_eq!(second.inserted, 0);
        assert_eq!(second.updated, 1);

        let projects = Project::find().all(&db).await?;
        let notes = Note::find().all(&db).await?;
        assert_eq!(projects.len(), 1);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].cached_content, "second");
        assert!(!notes[0].file_managed_by_app);

        Ok(())
    }
}
