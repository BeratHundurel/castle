use gpui::Action;
use serde::Deserialize;

gpui::actions!(
    markdown_editor,
    [SaveMarkdownFile, SaveMarkdownFileAs, ToggleEditorMode,]
);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = markdown_editor, no_json)]
pub(crate) struct ApplyMarkdownFormat(pub(crate) MarkdownFormat);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = markdown_editor, no_json)]
pub(crate) struct ExpandEmmet;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = markdown_editor, no_json)]
pub(crate) struct EmmetSubmitWrap;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = markdown_editor, no_json)]
pub(crate) struct EmmetCancelWrap;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = markdown_editor, no_json)]
pub(crate) struct ToggleDocumentOutline;

gpui::actions!(
    markdown_outline,
    [OutlinePrevious, OutlineNext, OutlineOpen, OutlineClose]
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
