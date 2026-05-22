use gpui::SharedString;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorMode {
    Split,
    Source,
    Preview,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum SaveState {
    Saved,
    Dirty,
    Saving,
    Missing,
    Error(SharedString),
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DocumentStats {
    pub(crate) lines: usize,
    pub(crate) words: usize,
    pub(crate) characters: usize,
}

impl DocumentStats {
    pub(crate) fn from_text(text: &str) -> Self {
        Self {
            lines: text.lines().count().max(1),
            words: text.split_whitespace().count(),
            characters: text.chars().count(),
        }
    }
}

pub(crate) const DEFAULT_NOTE: &str = r#"# Untitled note

Start writing Markdown here.
"#;
