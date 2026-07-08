use gpui::SharedString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorMode {
    Split,
    Source,
    Preview,
}

impl EditorMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Split => "split",
            Self::Source => "source",
            Self::Preview => "preview",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "split" => Self::Split,
            "preview" => Self::Preview,
            _ => Self::Source,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SaveState {
    Saved,
    Dirty,
    Saving,
    Missing,
    Error(SharedString),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentStats {
    pub lines: usize,
    pub words: usize,
    pub characters: usize,
}

impl DocumentStats {
    pub fn from_text(text: &str) -> Self {
        let mut lines = 0usize;
        let mut words = 0usize;
        let mut characters = 0usize;
        let mut in_word = false;

        for ch in text.chars() {
            characters = characters.saturating_add(1);
            if ch == '\n' {
                lines = lines.saturating_add(1);
            }
            if ch.is_whitespace() {
                in_word = false;
            } else if !in_word {
                words = words.saturating_add(1);
                in_word = true;
            }
        }

        if !text.is_empty() && !text.ends_with('\n') {
            lines = lines.saturating_add(1);
        }

        Self {
            lines: lines.max(1),
            words,
            characters,
        }
    }
}

pub(crate) const DEFAULT_NOTE: &str = r#"# Untitled note

Start writing Markdown here.
"#;
