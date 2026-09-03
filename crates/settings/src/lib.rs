mod persistence;
mod store;
mod view;

pub use store::{
    AppSettings, DEFAULT_EDITOR_FONT_FAMILY, DEFAULT_FONT_FAMILY, DEFAULT_QUICK_CAPTURE_SHORTCUT,
    DEFAULT_TRAY_SHORTCUT, StoredTab, TabSession, scrollbar_show_key,
};
pub use view::{
    AgentAccess, AgentAccessAvailability, SettingsIntegration, SettingsView, ShortcutReference,
    WorkspaceArchiveActions,
};
