use gpui_kit::{App, AsKeystroke as _, Global, KeyBinding, SharedString};
use settings::ShortcutReference;

use command_palette::{
    CloseCommandPaletteAction, CommandPaletteAction, OpenWorkspaceSearchAction,
    SelectNextCommandPaletteItem, SelectPrevCommandPaletteItem, SwitchThemeAction,
};
use document_editor::action::{
    ApplyMarkdownFormat, EmmetCancelWrap, EmmetSubmitWrap, ExpandEmmet, FormatDocument,
    MarkdownFormat, MoveLineDown, MoveLineUp, OutlineClose, OutlineLeft, OutlineNext, OutlineOpen,
    OutlinePrevious, OutlineRight, SaveDocumentFile, SaveDocumentFileAs, ToggleDocumentOutline,
    ToggleDocumentPreview, ToggleFocusMode, ToggleTask, ToggleTypewriterScrolling, ToggleZenMode,
    VimKey, VimKeyAction,
};
use shell::{CycleNextTab, CyclePrevTab, OpenSettingsAction, ToggleSidebarAction};

struct ShortcutRegistry(Vec<ShortcutReference>);

impl Global for ShortcutRegistry {}

pub fn shortcuts(cx: &App) -> &[ShortcutReference] {
    &cx.global::<ShortcutRegistry>().0
}

pub fn init(cx: &mut App) {
    let bindings = default_bindings();

    let shortcuts = bindings
        .iter()
        .filter(|binding| !binding.action().as_any().is::<VimKeyAction>())
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
    let mut bindings = vec![
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
            "cmd-alt-4",
            ApplyMarkdownFormat(MarkdownFormat::HeadingFour),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-4",
            ApplyMarkdownFormat(MarkdownFormat::HeadingFour),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-alt-5",
            ApplyMarkdownFormat(MarkdownFormat::HeadingFive),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-5",
            ApplyMarkdownFormat(MarkdownFormat::HeadingFive),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-alt-6",
            ApplyMarkdownFormat(MarkdownFormat::HeadingSix),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-6",
            ApplyMarkdownFormat(MarkdownFormat::HeadingSix),
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
        KeyBinding::new(
            "cmd-shift-x",
            ApplyMarkdownFormat(MarkdownFormat::Strikethrough),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-x",
            ApplyMarkdownFormat(MarkdownFormat::Strikethrough),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-shift-h",
            ApplyMarkdownFormat(MarkdownFormat::Highlight),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-h",
            ApplyMarkdownFormat(MarkdownFormat::Highlight),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-shift-9",
            ApplyMarkdownFormat(MarkdownFormat::Task),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-9",
            ApplyMarkdownFormat(MarkdownFormat::Task),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-alt-f",
            ApplyMarkdownFormat(MarkdownFormat::Footnote),
            Some("MarkdownSource"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-f",
            ApplyMarkdownFormat(MarkdownFormat::Footnote),
            Some("MarkdownSource"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-space", ToggleTask, Some("MarkdownSource")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-space", ToggleTask, Some("MarkdownSource")),
        KeyBinding::new("alt-up", MoveLineUp, Some("DocumentEditor")),
        KeyBinding::new("alt-down", MoveLineDown, Some("DocumentEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-s", SaveDocumentFile, Some("DocumentEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-s", SaveDocumentFile, Some("DocumentEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-s", SaveDocumentFileAs, Some("DocumentEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-s", SaveDocumentFileAs, Some("DocumentEditor")),
        KeyBinding::new("alt-shift-f", FormatDocument, Some("DocumentEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-alt-m", ToggleFocusMode, Some("DocumentEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-alt-m", ToggleFocusMode, Some("DocumentEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-alt-w",
            ToggleTypewriterScrolling,
            Some("DocumentEditor"),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-alt-w",
            ToggleTypewriterScrolling,
            Some("DocumentEditor"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-alt-z", ToggleZenMode, Some("DocumentEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-alt-z", ToggleZenMode, Some("DocumentEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-v", ToggleDocumentPreview, Some("DocumentEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-v",
            ToggleDocumentPreview,
            Some("DocumentEditor"),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-v", ToggleDocumentPreview, Some("TextView")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-v", ToggleDocumentPreview, Some("TextView")),
        KeyBinding::new("enter", EmmetSubmitWrap, Some("EmmetInput")),
        KeyBinding::new("escape", EmmetCancelWrap, Some("EmmetInput")),
        KeyBinding::new(
            "ctrl-shift-o",
            ToggleDocumentOutline,
            Some("DocumentEditor"),
        ),
        KeyBinding::new("up", OutlinePrevious, Some("DocumentOutline")),
        KeyBinding::new("down", OutlineNext, Some("DocumentOutline")),
        KeyBinding::new("left", OutlineLeft, Some("DocumentOutline")),
        KeyBinding::new("right", OutlineRight, Some("DocumentOutline")),
        KeyBinding::new("enter", OutlineOpen, Some("DocumentOutline")),
        KeyBinding::new("escape", OutlineClose, Some("DocumentOutline")),
    ];

    const VIM_CONTEXT: &str = "vim_mode == normal || vim_mode == visual";
    bindings.extend([
        KeyBinding::new("0", VimKeyAction(VimKey::Digit(0)), Some(VIM_CONTEXT)),
        KeyBinding::new("1", VimKeyAction(VimKey::Digit(1)), Some(VIM_CONTEXT)),
        KeyBinding::new("2", VimKeyAction(VimKey::Digit(2)), Some(VIM_CONTEXT)),
        KeyBinding::new("3", VimKeyAction(VimKey::Digit(3)), Some(VIM_CONTEXT)),
        KeyBinding::new("4", VimKeyAction(VimKey::Digit(4)), Some(VIM_CONTEXT)),
        KeyBinding::new("5", VimKeyAction(VimKey::Digit(5)), Some(VIM_CONTEXT)),
        KeyBinding::new("6", VimKeyAction(VimKey::Digit(6)), Some(VIM_CONTEXT)),
        KeyBinding::new("7", VimKeyAction(VimKey::Digit(7)), Some(VIM_CONTEXT)),
        KeyBinding::new("8", VimKeyAction(VimKey::Digit(8)), Some(VIM_CONTEXT)),
        KeyBinding::new("9", VimKeyAction(VimKey::Digit(9)), Some(VIM_CONTEXT)),
        KeyBinding::new("h", VimKeyAction(VimKey::Left), Some(VIM_CONTEXT)),
        KeyBinding::new("left", VimKeyAction(VimKey::Left), Some(VIM_CONTEXT)),
        KeyBinding::new("j", VimKeyAction(VimKey::Down), Some(VIM_CONTEXT)),
        KeyBinding::new("down", VimKeyAction(VimKey::Down), Some(VIM_CONTEXT)),
        KeyBinding::new("k", VimKeyAction(VimKey::Up), Some(VIM_CONTEXT)),
        KeyBinding::new("up", VimKeyAction(VimKey::Up), Some(VIM_CONTEXT)),
        KeyBinding::new("l", VimKeyAction(VimKey::Right), Some(VIM_CONTEXT)),
        KeyBinding::new("right", VimKeyAction(VimKey::Right), Some(VIM_CONTEXT)),
        KeyBinding::new("w", VimKeyAction(VimKey::WordForward), Some(VIM_CONTEXT)),
        KeyBinding::new("b", VimKeyAction(VimKey::WordBackward), Some(VIM_CONTEXT)),
        KeyBinding::new("e", VimKeyAction(VimKey::WordEnd), Some(VIM_CONTEXT)),
        KeyBinding::new("f", VimKeyAction(VimKey::FindForward), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "shift-f",
            VimKeyAction(VimKey::FindBackward),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("t", VimKeyAction(VimKey::TillForward), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "shift-t",
            VimKeyAction(VimKey::TillBackward),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new(";", VimKeyAction(VimKey::RepeatFind), Some(VIM_CONTEXT)),
        KeyBinding::new(
            ",",
            VimKeyAction(VimKey::RepeatFindReverse),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new(
            "enter",
            VimKeyAction(VimKey::LiteralEnter),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("tab", VimKeyAction(VimKey::LiteralTab), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "space",
            VimKeyAction(VimKey::LiteralSpace),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new(
            "shift-w",
            VimKeyAction(VimKey::BigWordForward),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new(
            "shift-b",
            VimKeyAction(VimKey::BigWordBackward),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new(
            "shift-e",
            VimKeyAction(VimKey::BigWordEnd),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("^", VimKeyAction(VimKey::FirstNonBlank), Some(VIM_CONTEXT)),
        KeyBinding::new("$", VimKeyAction(VimKey::LineEnd), Some(VIM_CONTEXT)),
        KeyBinding::new("g", VimKeyAction(VimKey::Go), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "shift-g",
            VimKeyAction(VimKey::DocumentEnd),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("i", VimKeyAction(VimKey::Insert), Some(VIM_CONTEXT)),
        KeyBinding::new("a", VimKeyAction(VimKey::Append), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "shift-i",
            VimKeyAction(VimKey::InsertLineStart),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new(
            "shift-a",
            VimKeyAction(VimKey::AppendLineEnd),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("o", VimKeyAction(VimKey::OpenBelow), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "shift-o",
            VimKeyAction(VimKey::OpenAbove),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("v", VimKeyAction(VimKey::Visual), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "shift-v",
            VimKeyAction(VimKey::VisualLine),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("\"", VimKeyAction(VimKey::DoubleQuote), Some(VIM_CONTEXT)),
        KeyBinding::new("'", VimKeyAction(VimKey::SingleQuote), Some(VIM_CONTEXT)),
        KeyBinding::new("`", VimKeyAction(VimKey::Backtick), Some(VIM_CONTEXT)),
        KeyBinding::new("(", VimKeyAction(VimKey::Parenthesis), Some(VIM_CONTEXT)),
        KeyBinding::new(
            ")",
            VimKeyAction(VimKey::ParenthesisClose),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("[", VimKeyAction(VimKey::Bracket), Some(VIM_CONTEXT)),
        KeyBinding::new("]", VimKeyAction(VimKey::BracketClose), Some(VIM_CONTEXT)),
        KeyBinding::new("{", VimKeyAction(VimKey::Brace), Some(VIM_CONTEXT)),
        KeyBinding::new("}", VimKeyAction(VimKey::BraceClose), Some(VIM_CONTEXT)),
        KeyBinding::new("x", VimKeyAction(VimKey::DeleteChar), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "shift-x",
            VimKeyAction(VimKey::DeletePreviousChar),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("s", VimKeyAction(VimKey::SubstituteChar), Some(VIM_CONTEXT)),
        KeyBinding::new("r", VimKeyAction(VimKey::ReplaceChar), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "shift-s",
            VimKeyAction(VimKey::SubstituteLine),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("d", VimKeyAction(VimKey::Delete), Some(VIM_CONTEXT)),
        KeyBinding::new("y", VimKeyAction(VimKey::Yank), Some(VIM_CONTEXT)),
        KeyBinding::new("shift-y", VimKeyAction(VimKey::YankLine), Some(VIM_CONTEXT)),
        KeyBinding::new("c", VimKeyAction(VimKey::Change), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "shift-j",
            VimKeyAction(VimKey::JoinLines),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new(
            "shift-d",
            VimKeyAction(VimKey::DeleteToLineEnd),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new(
            "shift-c",
            VimKeyAction(VimKey::ChangeToLineEnd),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("p", VimKeyAction(VimKey::PasteAfter), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "shift-p",
            VimKeyAction(VimKey::PasteBefore),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("u", VimKeyAction(VimKey::Undo), Some(VIM_CONTEXT)),
        KeyBinding::new("ctrl-r", VimKeyAction(VimKey::Redo), Some(VIM_CONTEXT)),
        KeyBinding::new(
            ".",
            VimKeyAction(VimKey::RepeatLastChange),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("escape", VimKeyAction(VimKey::Escape), Some(VIM_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-z", VimKeyAction(VimKey::Undo), Some(VIM_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-z", VimKeyAction(VimKey::Undo), Some(VIM_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-z", VimKeyAction(VimKey::Redo), Some(VIM_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-z",
            VimKeyAction(VimKey::Redo),
            Some(VIM_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-f", VimKeyAction(VimKey::Search), Some(VIM_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-f", VimKeyAction(VimKey::Search), Some(VIM_CONTEXT)),
    ]);

    bindings
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
            MarkdownFormat::HeadingFour => "HeadingFour",
            MarkdownFormat::HeadingFive => "HeadingFive",
            MarkdownFormat::HeadingSix => "HeadingSix",
            MarkdownFormat::Bold => "Bold",
            MarkdownFormat::Italic => "Italic",
            MarkdownFormat::InlineCode => "InlineCode",
            MarkdownFormat::Link => "Link",
            MarkdownFormat::Task => "Task",
            MarkdownFormat::Footnote => "Footnote",
            MarkdownFormat::Strikethrough => "Strikethrough",
            MarkdownFormat::Highlight => "Highlight",
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

    #[test]
    fn smart_markdown_shortcuts_are_registered() {
        let bindings = default_bindings();
        let formats = [
            MarkdownFormat::HeadingFour,
            MarkdownFormat::HeadingFive,
            MarkdownFormat::HeadingSix,
            MarkdownFormat::Task,
            MarkdownFormat::Footnote,
            MarkdownFormat::Strikethrough,
            MarkdownFormat::Highlight,
        ];

        for format in formats {
            assert!(bindings.iter().any(|binding| {
                binding
                    .action()
                    .as_any()
                    .downcast_ref::<ApplyMarkdownFormat>()
                    .is_some_and(|action| action.0 == format)
            }));
        }
        assert!(
            bindings
                .iter()
                .any(|binding| binding.action().as_any().is::<MoveLineUp>())
        );
        assert!(
            bindings
                .iter()
                .any(|binding| binding.action().as_any().is::<MoveLineDown>())
        );
        assert!(
            bindings
                .iter()
                .any(|binding| binding.action().as_any().is::<ToggleTask>())
        );
        let focus_mode = bindings
            .iter()
            .find(|binding| binding.action().as_any().is::<ToggleFocusMode>())
            .expect("focus mode binding should be registered");
        let typewriter_scrolling = bindings
            .iter()
            .find(|binding| binding.action().as_any().is::<ToggleTypewriterScrolling>())
            .expect("typewriter scrolling binding should be registered");
        assert_eq!(
            focus_mode.keystrokes(),
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd-alt-m"
                } else {
                    "ctrl-alt-m"
                },
                ToggleFocusMode,
                Some("DocumentEditor"),
            )
            .keystrokes()
        );
        assert_eq!(
            typewriter_scrolling.keystrokes(),
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd-alt-w"
                } else {
                    "ctrl-alt-w"
                },
                ToggleTypewriterScrolling,
                Some("DocumentEditor"),
            )
            .keystrokes()
        );
        let zen_mode = bindings
            .iter()
            .find(|binding| binding.action().as_any().is::<ToggleZenMode>())
            .expect("zen mode binding should be registered");
        assert_eq!(
            zen_mode.keystrokes(),
            KeyBinding::new(
                if cfg!(target_os = "macos") {
                    "cmd-alt-z"
                } else {
                    "ctrl-alt-z"
                },
                ToggleZenMode,
                Some("DocumentEditor"),
            )
            .keystrokes()
        );

        let toggle_task = bindings
            .iter()
            .find(|binding| binding.action().as_any().is::<ToggleTask>())
            .expect("task toggle binding should be registered");
        let tray_shortcut = KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-alt-space"
            } else {
                "ctrl-alt-space"
            },
            ToggleTask,
            Some("MarkdownSource"),
        );
        assert!(
            !toggle_task
                .keystrokes()
                .starts_with(tray_shortcut.keystrokes())
        );

        let expected_shortcut = KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-shift-space"
            } else {
                "ctrl-shift-space"
            },
            ToggleTask,
            Some("MarkdownSource"),
        );
        assert_eq!(toggle_task.keystrokes(), expected_shortcut.keystrokes());
    }
}
