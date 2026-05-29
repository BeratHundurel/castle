use gpui::{App, KeyBinding};

use crate::app_shell::{
    CloseCommandPaletteAction, CommandPaletteAction, CycleNextTab, CyclePrevTab,
    SelectNextCommandPaletteItem, SelectPrevCommandPaletteItem, ToggleSidebarAction,
};
use crate::markdown_editor::action::{
    EmmetCancelWrap, EmmetSubmitWrap, ExpandEmmet, SaveMarkdownFile, SaveMarkdownFileAs,
    ToggleEditorMode,
};

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-tab", CycleNextTab, Some("AppShell")),
        KeyBinding::new("ctrl-shift-tab", CyclePrevTab, Some("AppShell")),
        KeyBinding::new("ctrl-p", CommandPaletteAction, Some("AppShell")),
        KeyBinding::new("escape", CloseCommandPaletteAction, Some("AppShell")),
        KeyBinding::new("escape", CloseCommandPaletteAction, Some("CommandPalette")),
        KeyBinding::new("up", SelectPrevCommandPaletteItem, Some("CommandPalette")),
        KeyBinding::new("down", SelectNextCommandPaletteItem, Some("CommandPalette")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-b", ToggleSidebarAction, Some("AppShell")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-b", ToggleSidebarAction, Some("AppShell")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-alt-e", ExpandEmmet, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-alt-e", ExpandEmmet, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-s", SaveMarkdownFile, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-s", SaveMarkdownFile, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-s", SaveMarkdownFileAs, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-s", SaveMarkdownFileAs, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-v", ToggleEditorMode, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-v", ToggleEditorMode, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-v", ToggleEditorMode, Some("TextView")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-v", ToggleEditorMode, Some("TextView")),
        KeyBinding::new("enter", EmmetSubmitWrap, Some("EmmetInput")),
        KeyBinding::new("escape", EmmetCancelWrap, Some("EmmetInput")),
    ]);
}
