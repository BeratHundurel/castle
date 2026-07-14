use gpui::{App, AsKeystroke as _, Global, KeyBinding, Keystroke, SharedString};

use crate::app_shell::{CycleNextTab, CyclePrevTab, OpenSettingsAction, ToggleSidebarAction};
use crate::command_palette::{
    CloseCommandPaletteAction, CommandPaletteAction, OpenWorkspaceSearchAction,
    SelectNextCommandPaletteItem, SelectPrevCommandPaletteItem, SwitchThemeAction,
};
use crate::markdown_editor::action::{
    ApplyMarkdownFormat, EmmetCancelWrap, EmmetSubmitWrap, ExpandEmmet, MarkdownFormat,
    OutlineClose, OutlineNext, OutlineOpen, OutlinePrevious, SaveMarkdownFile, SaveMarkdownFileAs,
    ToggleDocumentOutline, ToggleEditorMode,
};

#[derive(Clone)]
pub(crate) struct ShortcutReference {
    pub(crate) action: SharedString,
    pub(crate) context: SharedString,
    pub(crate) keystrokes: Vec<Keystroke>,
}

struct ShortcutRegistry(Vec<ShortcutReference>);

impl Global for ShortcutRegistry {}

pub(crate) fn shortcuts(cx: &App) -> &[ShortcutReference] {
    &cx.global::<ShortcutRegistry>().0
}

pub fn init(cx: &mut App) {
    let bindings = default_bindings();

    let shortcuts = bindings
        .iter()
        .map(|binding| ShortcutReference {
            action: shortcut_action_name(binding),
            context: binding
                .predicate()
                .map(|predicate| predicate.to_string().into())
                .unwrap_or_else(|| "Global".into()),
            keystrokes: binding
                .keystrokes()
                .iter()
                .map(|stroke| stroke.as_keystroke().clone())
                .collect(),
        })
        .collect();

    cx.set_global(ShortcutRegistry(shortcuts));
    cx.bind_keys(bindings);
}

fn default_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("ctrl-tab", CycleNextTab, Some("AppShell")),
        KeyBinding::new("ctrl-shift-tab", CyclePrevTab, Some("AppShell")),
        KeyBinding::new("ctrl-p", CommandPaletteAction, Some("AppShell")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-,", OpenSettingsAction, Some("AppShell")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-,", OpenSettingsAction, Some("AppShell")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-f", OpenWorkspaceSearchAction, Some("AppShell")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-f", OpenWorkspaceSearchAction, Some("AppShell")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-alt-t", SwitchThemeAction, Some("AppShell")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-alt-t", SwitchThemeAction, Some("AppShell")),
        KeyBinding::new("escape", CloseCommandPaletteAction, Some("AppShell")),
        KeyBinding::new("escape", CloseCommandPaletteAction, Some("CommandPalette")),
        KeyBinding::new("up", SelectPrevCommandPaletteItem, Some("CommandPalette")),
        KeyBinding::new("down", SelectNextCommandPaletteItem, Some("CommandPalette")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-b", ToggleSidebarAction, Some("AppShell")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-b", ToggleSidebarAction, Some("AppShell")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-alt-e", ExpandEmmet, Some("MarkdownSource")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-alt-e", ExpandEmmet, Some("MarkdownSource")),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-b",
            ApplyMarkdownFormat(MarkdownFormat::Bold),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-b",
            ApplyMarkdownFormat(MarkdownFormat::Bold),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-i",
            ApplyMarkdownFormat(MarkdownFormat::Italic),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-i",
            ApplyMarkdownFormat(MarkdownFormat::Italic),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-k",
            ApplyMarkdownFormat(MarkdownFormat::Link),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-k",
            ApplyMarkdownFormat(MarkdownFormat::Link),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-e",
            ApplyMarkdownFormat(MarkdownFormat::InlineCode),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-e",
            ApplyMarkdownFormat(MarkdownFormat::InlineCode),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-alt-1",
            ApplyMarkdownFormat(MarkdownFormat::HeadingOne),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-1",
            ApplyMarkdownFormat(MarkdownFormat::HeadingOne),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-alt-2",
            ApplyMarkdownFormat(MarkdownFormat::HeadingTwo),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-2",
            ApplyMarkdownFormat(MarkdownFormat::HeadingTwo),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-alt-3",
            ApplyMarkdownFormat(MarkdownFormat::HeadingThree),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-3",
            ApplyMarkdownFormat(MarkdownFormat::HeadingThree),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-shift-7",
            ApplyMarkdownFormat(MarkdownFormat::OrderedList),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-7",
            ApplyMarkdownFormat(MarkdownFormat::OrderedList),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-shift-8",
            ApplyMarkdownFormat(MarkdownFormat::BulletList),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-8",
            ApplyMarkdownFormat(MarkdownFormat::BulletList),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-shift-.",
            ApplyMarkdownFormat(MarkdownFormat::Quote),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-.",
            ApplyMarkdownFormat(MarkdownFormat::Quote),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-alt-c",
            ApplyMarkdownFormat(MarkdownFormat::CodeBlock),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-c",
            ApplyMarkdownFormat(MarkdownFormat::CodeBlock),
            Some("MarkdownSource"),
        ),
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
        KeyBinding::new(
            "ctrl-shift-o",
            ToggleDocumentOutline,
            Some("MarkdownEditor"),
        ),
        KeyBinding::new("up", OutlinePrevious, Some("MarkdownOutline")),
        KeyBinding::new("down", OutlineNext, Some("MarkdownOutline")),
        KeyBinding::new("enter", OutlineOpen, Some("MarkdownOutline")),
        KeyBinding::new("escape", OutlineClose, Some("MarkdownOutline")),
    ]
}

fn shortcut_action_name(binding: &KeyBinding) -> SharedString {
    if let Some(action) = binding
        .action()
        .as_any()
        .downcast_ref::<ApplyMarkdownFormat>()
    {
        return humanize_identifier(match action.0 {
            MarkdownFormat::HeadingOne => "HeadingOne",
            MarkdownFormat::HeadingTwo => "HeadingTwo",
            MarkdownFormat::HeadingThree => "HeadingThree",
            MarkdownFormat::Bold => "Bold",
            MarkdownFormat::Italic => "Italic",
            MarkdownFormat::InlineCode => "InlineCode",
            MarkdownFormat::Link => "Link",
            MarkdownFormat::BulletList => "BulletList",
            MarkdownFormat::OrderedList => "OrderedList",
            MarkdownFormat::Quote => "Quote",
            MarkdownFormat::CodeBlock => "CodeBlock",
        });
    }

    let name = binding
        .action()
        .name()
        .rsplit("::")
        .next()
        .unwrap_or(binding.action().name())
        .strip_suffix("Action")
        .unwrap_or_else(|| {
            binding
                .action()
                .name()
                .rsplit("::")
                .next()
                .unwrap_or(binding.action().name())
        });

    humanize_identifier(name)
}

pub(crate) fn humanize_identifier(value: &str) -> SharedString {
    let mut label = String::with_capacity(value.len() + 4);
    let mut previous_is_lowercase = false;

    for character in value.chars() {
        if character == '_' || character == '-' {
            if !label.ends_with(' ') {
                label.push(' ');
            }
            previous_is_lowercase = false;
            continue;
        }

        if character.is_uppercase() && previous_is_lowercase {
            label.push(' ');
        }
        label.push(character);
        previous_is_lowercase = character.is_lowercase();
    }

    label.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_shortcut_does_not_shadow_markdown_link() {
        let bindings = default_bindings();
        let theme = bindings
            .iter()
            .find(|binding| binding.action().as_any().is::<SwitchThemeAction>())
            .expect("theme binding should be registered");
        let link = bindings
            .iter()
            .find(|binding| {
                binding
                    .action()
                    .as_any()
                    .downcast_ref::<ApplyMarkdownFormat>()
                    .is_some_and(|action| action.0 == MarkdownFormat::Link)
            })
            .expect("markdown link binding should be registered");

        assert!(!theme.keystrokes().starts_with(link.keystrokes()));
    }
}
