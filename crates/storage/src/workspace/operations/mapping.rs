use super::*;

pub(super) fn note_summary(note: note::Model, projects: &HashMap<i64, String>) -> NoteSummary {
    NoteSummary {
        id: note.id,
        title: note.title,
        project_id: note.project_id,
        project_name: note
            .project_id
            .and_then(|project_id| projects.get(&project_id).cloned()),
        is_pinned: note.is_pinned,
        updated_at: note.updated_at,
    }
}

pub(super) fn label_detail(label: board_label::Model) -> LabelDetail {
    LabelDetail {
        id: label.id,
        board_id: label.board_id,
        name: label.name,
        color: label.color,
    }
}

pub(crate) fn property_definition_detail(
    property: crate::board::properties::PropertyDefinition,
) -> BoardPropertyDefinitionDetail {
    BoardPropertyDefinitionDetail {
        id: property.id,
        board_id: property.board_id,
        name: property.name,
        kind: property.kind.as_str().to_string(),
        position: property.position,
        options: property
            .options
            .into_iter()
            .map(property_option_detail)
            .collect(),
    }
}

pub(crate) fn property_option_detail(
    option: crate::board::properties::PropertyOption,
) -> BoardPropertyOptionDetail {
    BoardPropertyOptionDetail {
        id: option.id,
        name: option.name,
        color: option.color,
        position: option.position,
    }
}

pub(crate) fn property_value_detail(
    value: crate::board::properties::PropertyValue,
) -> BoardPropertyValueDetail {
    match value {
        crate::board::properties::PropertyValue::Text(value) => {
            BoardPropertyValueDetail::Text(value)
        }
        crate::board::properties::PropertyValue::Number(value) => {
            BoardPropertyValueDetail::Number(value)
        }
        crate::board::properties::PropertyValue::Checkbox(value) => {
            BoardPropertyValueDetail::Checkbox(value)
        }
        crate::board::properties::PropertyValue::Date(value) => {
            BoardPropertyValueDetail::Date(value)
        }
        crate::board::properties::PropertyValue::Select(value) => {
            BoardPropertyValueDetail::Select(value)
        }
        crate::board::properties::PropertyValue::Url(value) => BoardPropertyValueDetail::Url(value),
    }
}

pub(crate) fn storage_property_value(
    value: BoardPropertyValueDetail,
) -> crate::board::properties::PropertyValue {
    match value {
        BoardPropertyValueDetail::Text(value) => {
            crate::board::properties::PropertyValue::Text(value)
        }
        BoardPropertyValueDetail::Number(value) => {
            crate::board::properties::PropertyValue::Number(value)
        }
        BoardPropertyValueDetail::Checkbox(value) => {
            crate::board::properties::PropertyValue::Checkbox(value)
        }
        BoardPropertyValueDetail::Date(value) => {
            crate::board::properties::PropertyValue::Date(value)
        }
        BoardPropertyValueDetail::Select(value) => {
            crate::board::properties::PropertyValue::Select(value)
        }
        BoardPropertyValueDetail::Url(value) => crate::board::properties::PropertyValue::Url(value),
    }
}

pub(super) fn note_link_detail(link: crate::note::links::NoteLinkReference) -> NoteLinkDetail {
    NoteLinkDetail {
        source_note_id: link.source_note_id,
        source_title: link.source_title,
        source_project_name: link.source_project_name,
        target_note_id: link.target_note_id,
        target_title: link.target_title,
        target_project_name: link.target_project_name,
        target_kind: None,
        raw_target: link.raw_target,
        display_text: link.display_text,
        start_byte: link.start_byte,
        end_byte: link.end_byte,
        line_number: link.line_number,
    }
}

pub(super) fn unresolved_link_detail(
    link: crate::note::links::UnresolvedLinkReference,
) -> NoteLinkDetail {
    NoteLinkDetail {
        source_note_id: link.source_note_id,
        source_title: link.source_title,
        source_project_name: link.source_project_name,
        target_note_id: None,
        target_title: None,
        target_project_name: None,
        target_kind: link.target_kind.map(|kind| kind.as_str().to_string()),
        raw_target: link.raw_target,
        display_text: link.display_text,
        start_byte: link.start_byte,
        end_byte: link.end_byte,
        line_number: link.line_number,
    }
}

pub(super) fn workspace_origin_label(
    origin: crate::workspace::links::WorkspaceLinkOrigin,
) -> &'static str {
    match origin {
        crate::workspace::links::WorkspaceLinkOrigin::Manual => "manual",
        crate::workspace::links::WorkspaceLinkOrigin::Wikilink => "wikilink",
        crate::workspace::links::WorkspaceLinkOrigin::Embed => "embed",
    }
}

pub(super) fn related_item_detail(
    entry: crate::workspace::links::WorkspaceCatalogEntry,
    origins: Vec<String>,
) -> RelatedItemDetail {
    RelatedItemDetail {
        kind: entry.item.kind.as_str().to_string(),
        id: entry.item.id,
        title: entry.title.clone(),
        breadcrumb: entry.breadcrumb(),
        stable_link: entry.stable_link(),
        origins,
    }
}

pub(super) fn related_note_detail(note: crate::workspace::links::RelatedNote) -> RelatedItemDetail {
    let item = crate::workspace::links::WorkspaceItemRef {
        kind: crate::workspace::links::WorkspaceItemKind::Note,
        id: note.note_id,
    };
    RelatedItemDetail {
        kind: item.kind.as_str().to_string(),
        id: item.id,
        title: note.title.clone(),
        breadcrumb: note
            .project_name
            .as_ref()
            .map(|project| format!("{project} / {}", note.title))
            .unwrap_or_else(|| note.title.clone()),
        stable_link: crate::workspace::links::stable_workspace_link(item, &note.title),
        origins: note
            .origins
            .into_iter()
            .map(workspace_origin_label)
            .map(str::to_string)
            .collect(),
    }
}

pub(super) fn label_record_detail(label: crate::board::LabelRecord) -> LabelDetail {
    LabelDetail {
        id: i64::from(label.id),
        board_id: i64::from(label.board_id),
        name: label.name,
        color: label.color,
    }
}

pub(super) fn entry_record_detail(
    entry: crate::board::BoardCardRecord,
    list_title: &str,
    board: &board::Model,
    project_name: Option<String>,
) -> EntryDetail {
    EntryDetail {
        id: i64::from(entry.id),
        title: entry.title,
        description: entry.description,
        due_on: entry.due_on,
        reminder_enabled: entry.reminder_enabled,
        position: entry.position,
        list_id: i64::from(entry.card_id),
        list_title: list_title.to_string(),
        board_id: board.id,
        board_title: board.title.clone(),
        project_id: board.project_id,
        project_name,
        labels: entry.labels.into_iter().map(label_record_detail).collect(),
        checklist_items: entry
            .checklist_items
            .into_iter()
            .map(|item| ChecklistItemDetail {
                id: i64::from(item.id),
                title: item.title,
                checked: item.checked,
                position: item.position,
            })
            .collect(),
        attachments: entry
            .attachments
            .into_iter()
            .map(|attachment| AttachmentDetail {
                id: i64::from(attachment.id),
                file_name: attachment.file_name,
            })
            .collect(),
        related_items: entry
            .related_notes
            .into_iter()
            .map(related_note_detail)
            .collect(),
    }
}
