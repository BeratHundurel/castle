use gpui::Action;
use serde::Deserialize;

gpui::actions!(
    document_editor,
    [SaveDocumentFile, SaveDocumentFileAs, ToggleDocumentPreview,]
);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub(crate) struct ApplyMarkdownFormat(pub(crate) MarkdownFormat);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub(crate) struct ExpandEmmet;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub(crate) struct EmmetSubmitWrap;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub(crate) struct EmmetCancelWrap;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub(crate) struct ToggleDocumentOutline;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = document_editor, no_json)]
pub(crate) struct VimKeyAction(pub(crate) VimKey);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub(crate) enum VimKey {
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

gpui::actions!(
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
pub(crate) enum MarkdownFormat {
    HeadingOne,
    HeadingTwo,
    HeadingThree,
    Bold,
    Italic,
    InlineCode,
    Link,
    BulletList,
    OrderedList,
    Quote,
    CodeBlock,
}
