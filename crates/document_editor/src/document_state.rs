use gpui::SharedString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Source,
    Split,
    Preview,
}

impl EditorMode {
    pub(crate) fn from_key(value: &str) -> Self {
        match value {
            "split" => Self::Split,
            "preview" => Self::Preview,
            _ => Self::Source,
        }
    }

    pub(crate) fn shows_source(self) -> bool {
        matches!(self, Self::Source | Self::Split)
    }

    pub(crate) fn shows_preview(self) -> bool {
        matches!(self, Self::Preview | Self::Split)
    }
}

#[cfg(test)]
mod tests {
    use super::EditorMode;

    #[test]
    fn split_mode_is_loaded_and_exposes_both_surfaces() {
        let mode = EditorMode::from_key("split");

        assert_eq!(mode, EditorMode::Split);
        assert!(mode.shows_source());
        assert!(mode.shows_preview());
    }

    #[test]
    fn unknown_mode_keys_still_default_to_source() {
        assert_eq!(EditorMode::from_key("unknown"), EditorMode::Source);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveState {
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

pub const DEFAULT_NOTE: &str = r#"# Untitled note

Start writing Markdown here.
"#;
