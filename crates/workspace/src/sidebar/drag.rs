use crate::{WorkspaceDragInfo, WorkspaceDragKind};
use gpui::SharedString;

use super::content_item::SidebarContentItem;

pub(super) fn project_drag_info(
    id: u32,
    source_index: usize,
    title: SharedString,
    item_count: usize,
) -> WorkspaceDragInfo {
    WorkspaceDragInfo::new(
        WorkspaceDragKind::Project { id, source_index },
        title,
        "Project",
        format!(
            "{} {}",
            item_count,
            if item_count == 1 { "item" } else { "items" }
        ),
        gpui_component::IconName::FolderOpen,
    )
}

pub(super) fn content_drag_info(
    item: &SidebarContentItem,
    origin: SharedString,
) -> WorkspaceDragInfo {
    let kind = match item {
        SidebarContentItem::Board { id, project_id, .. } => WorkspaceDragKind::Board {
            id: *id,
            project_id: *project_id,
        },
        SidebarContentItem::Note { id, project_id, .. } => WorkspaceDragKind::Note {
            id: *id,
            project_id: *project_id,
        },
    };
    WorkspaceDragInfo::new(
        kind,
        item.title(),
        item.kind_label(),
        format!("From {origin}"),
        item.icon(),
    )
}
