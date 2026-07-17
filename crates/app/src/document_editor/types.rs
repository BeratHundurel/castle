use gpui::SharedString;
use gpui_component::highlighter::Language;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentKind {
    Markdown,
    Json,
    PlainText,
}

impl DocumentKind {
    pub(crate) fn from_path(path: Option<&Path>) -> Self {
        let Some(path) = path else {
            return Self::Markdown;
        };
        let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
            return Self::PlainText;
        };

        match extension.to_ascii_lowercase().as_str() {
            "md" | "markdown" => Self::Markdown,
            "json" => Self::Json,
            _ => Self::PlainText,
        }
    }

    pub(crate) fn language(self) -> Language {
        match self {
            Self::Markdown => Language::Markdown,
            Self::Json => Language::Json,
            Self::PlainText => Language::Plain,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Json => "JSON",
            Self::PlainText => "Plain Text",
        }
    }

    pub(crate) fn menu_label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown (.md)",
            Self::Json => "JSON (.json)",
            Self::PlainText => "Plain Text (.txt)",
        }
    }

    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
            Self::PlainText => "txt",
        }
    }

    pub(crate) fn supports_outline(self) -> bool {
        !matches!(self, Self::PlainText)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorMode {
    Source,
    Preview,
}

impl EditorMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Preview => "preview",
        }
    }

    pub(crate) fn from_str(value: &str) -> Self {
        match value {
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

#[cfg(test)]
mod tests {
    use super::DocumentKind;
    use gpui_component::highlighter::Language;
    use std::path::Path;

    #[test]
    fn classifies_document_paths_case_insensitively() {
        assert_eq!(
            DocumentKind::from_path(Some(Path::new("note.MD"))),
            DocumentKind::Markdown
        );
        assert_eq!(
            DocumentKind::from_path(Some(Path::new("note.markdown"))),
            DocumentKind::Markdown
        );
        assert_eq!(
            DocumentKind::from_path(Some(Path::new("data.JSON"))),
            DocumentKind::Json
        );
        assert_eq!(
            DocumentKind::from_path(Some(Path::new("notes.txt"))),
            DocumentKind::PlainText
        );
        assert_eq!(
            DocumentKind::from_path(Some(Path::new("LICENSE"))),
            DocumentKind::PlainText
        );
    }

    #[test]
    fn treats_pathless_notes_as_markdown() {
        assert_eq!(DocumentKind::from_path(None), DocumentKind::Markdown);
    }

    #[test]
    fn selects_the_matching_highlighter_and_controls() {
        assert_eq!(DocumentKind::Markdown.language(), Language::Markdown);
        assert_eq!(DocumentKind::Json.language(), Language::Json);
        assert_eq!(DocumentKind::PlainText.language(), Language::Plain);
        assert!(DocumentKind::Markdown.supports_outline());
        assert!(DocumentKind::Json.supports_outline());
        assert!(!DocumentKind::PlainText.supports_outline());
        assert_eq!(DocumentKind::Markdown.extension(), "md");
        assert_eq!(DocumentKind::Json.extension(), "json");
        assert_eq!(DocumentKind::PlainText.extension(), "txt");
    }
}
