use gpui_kit::{Action, KeyBinding};
use serde::Deserialize;

gpui_kit::actions!(
    document_editor,
    [
        FormatDocument,
        SaveDocumentFile,
        SaveDocumentFileAs,
        ToggleDocumentPreview,
        CreateCardFromSelectionAction,
        InsertBoardViewAction,
        MoveLineUp,
        MoveLineDown,
        ToggleTask,
        ToggleFocusMode,
        ToggleTypewriterScrolling,
        ToggleZenMode,
        ToggleZenStatusBar,
    ]
);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub struct ApplyMarkdownFormat(pub MarkdownFormat);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub struct ExpandEmmet;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub struct EmmetSubmitWrap;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub struct EmmetCancelWrap;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub struct ToggleDocumentOutline;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub struct VimKeyAction(pub VimKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum VimKey {
    Digit(u8),
    Left,
    Down,
    Up,
    Right,
    WordForward,
    WordBackward,
    WordEnd,
    BigWordForward,
    BigWordBackward,
    BigWordEnd,
    FindForward,
    FindBackward,
    TillForward,
    TillBackward,
    RepeatFind,
    RepeatFindReverse,
    LiteralEnter,
    LiteralTab,
    LiteralSpace,
    LineStart,
    FirstNonBlank,
    LineEnd,
    Go,
    DocumentEnd,
    Insert,
    Append,
    InsertLineStart,
    AppendLineEnd,
    OpenBelow,
    OpenAbove,
    Visual,
    VisualLine,
    DoubleQuote,
    SingleQuote,
    Backtick,
    Parenthesis,
    ParenthesisClose,
    Bracket,
    BracketClose,
    Brace,
    BraceClose,
    DeleteChar,
    DeletePreviousChar,
    SubstituteChar,
    SubstituteLine,
    ReplaceChar,
    YankLine,
    JoinLines,
    Delete,
    Yank,
    Change,
    DeleteToLineEnd,
    ChangeToLineEnd,
    PasteAfter,
    PasteBefore,
    Undo,
    Redo,
    RepeatLastChange,
    Search,
    Escape,
}

gpui_kit::actions!(
    document_outline,
    [
        OutlinePrevious,
        OutlineNext,
        OutlineLeft,
        OutlineRight,
        OutlineOpen,
        OutlineClose
    ]
);

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum MarkdownFormat {
    HeadingOne,
    HeadingTwo,
    HeadingThree,
    HeadingFour,
    HeadingFive,
    HeadingSix,
    Bold,
    Italic,
    InlineCode,
    Link,
    Task,
    Footnote,
    Strikethrough,
    Highlight,
    BulletList,
    OrderedList,
    Quote,
    CodeBlock,
}

pub fn vim_key_bindings() -> Vec<KeyBinding> {
    const VIM_CONTEXT: &str = "vim_mode == normal || vim_mode == visual";
    let commands = [
        ("0", VimKey::Digit(0)),
        ("1", VimKey::Digit(1)),
        ("2", VimKey::Digit(2)),
        ("3", VimKey::Digit(3)),
        ("4", VimKey::Digit(4)),
        ("5", VimKey::Digit(5)),
        ("6", VimKey::Digit(6)),
        ("7", VimKey::Digit(7)),
        ("8", VimKey::Digit(8)),
        ("9", VimKey::Digit(9)),
        ("h", VimKey::Left),
        ("left", VimKey::Left),
        ("j", VimKey::Down),
        ("down", VimKey::Down),
        ("k", VimKey::Up),
        ("up", VimKey::Up),
        ("l", VimKey::Right),
        ("right", VimKey::Right),
        ("w", VimKey::WordForward),
        ("b", VimKey::WordBackward),
        ("e", VimKey::WordEnd),
        ("f", VimKey::FindForward),
        ("shift-f", VimKey::FindBackward),
        ("t", VimKey::TillForward),
        ("shift-t", VimKey::TillBackward),
        (";", VimKey::RepeatFind),
        (",", VimKey::RepeatFindReverse),
        ("enter", VimKey::LiteralEnter),
        ("tab", VimKey::LiteralTab),
        ("space", VimKey::LiteralSpace),
        ("shift-w", VimKey::BigWordForward),
        ("shift-b", VimKey::BigWordBackward),
        ("shift-e", VimKey::BigWordEnd),
        ("^", VimKey::FirstNonBlank),
        ("$", VimKey::LineEnd),
        ("g", VimKey::Go),
        ("shift-g", VimKey::DocumentEnd),
        ("i", VimKey::Insert),
        ("a", VimKey::Append),
        ("shift-i", VimKey::InsertLineStart),
        ("shift-a", VimKey::AppendLineEnd),
        ("o", VimKey::OpenBelow),
        ("shift-o", VimKey::OpenAbove),
        ("v", VimKey::Visual),
        ("shift-v", VimKey::VisualLine),
        ("\"", VimKey::DoubleQuote),
        ("'", VimKey::SingleQuote),
        ("`", VimKey::Backtick),
        ("(", VimKey::Parenthesis),
        (")", VimKey::ParenthesisClose),
        ("[", VimKey::Bracket),
        ("]", VimKey::BracketClose),
        ("{", VimKey::Brace),
        ("}", VimKey::BraceClose),
        ("x", VimKey::DeleteChar),
        ("shift-x", VimKey::DeletePreviousChar),
        ("s", VimKey::SubstituteChar),
        ("r", VimKey::ReplaceChar),
        ("shift-s", VimKey::SubstituteLine),
        ("d", VimKey::Delete),
        ("y", VimKey::Yank),
        ("shift-y", VimKey::YankLine),
        ("c", VimKey::Change),
        ("shift-j", VimKey::JoinLines),
        ("shift-d", VimKey::DeleteToLineEnd),
        ("shift-c", VimKey::ChangeToLineEnd),
        ("p", VimKey::PasteAfter),
        ("shift-p", VimKey::PasteBefore),
        ("u", VimKey::Undo),
        ("ctrl-r", VimKey::Redo),
        (".", VimKey::RepeatLastChange),
        ("escape", VimKey::Escape),
    ];
    let mut bindings = commands
        .into_iter()
        .map(|(key, command)| KeyBinding::new(key, VimKeyAction(command), Some(VIM_CONTEXT)))
        .collect::<Vec<_>>();
    #[cfg(target_os = "macos")]
    bindings.extend([
        KeyBinding::new("cmd-z", VimKeyAction(VimKey::Undo), Some(VIM_CONTEXT)),
        KeyBinding::new("cmd-shift-z", VimKeyAction(VimKey::Redo), Some(VIM_CONTEXT)),
        KeyBinding::new("cmd-f", VimKeyAction(VimKey::Search), Some(VIM_CONTEXT)),
    ]);
    #[cfg(not(target_os = "macos"))]
    bindings.extend([
        KeyBinding::new("ctrl-z", VimKeyAction(VimKey::Undo), Some(VIM_CONTEXT)),
        KeyBinding::new(
            "ctrl-shift-z",
            VimKeyAction(VimKey::Redo),
            Some(VIM_CONTEXT),
        ),
        KeyBinding::new("ctrl-f", VimKeyAction(VimKey::Search), Some(VIM_CONTEXT)),
    ]);
    bindings
}
