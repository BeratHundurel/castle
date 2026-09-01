#![recursion_limit = "256"]

mod document_kind;
mod drag;
mod navigation;
mod request_tracker;
mod sidebar;
mod wikilinks;

pub use document_kind::DocumentKind;
pub use drag::{WorkspaceDragInfo, WorkspaceDragKind};
pub use navigation::{
    WorkspaceNavigationHandler, WorkspaceNavigationTarget, weak_navigation_handler,
};
pub use request_tracker::RequestTracker;
pub use sidebar::{ActiveItem, SidebarEvent, SidebarView};
pub use wikilinks::{
    WikiLinkCompletionProvider, WikiLinkPreviewPlugin, WorkspaceReferenceCompletionProvider,
    workspace_navigation_target,
};
