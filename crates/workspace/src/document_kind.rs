use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Markdown,
    Json,
    PlainText,
}

impl DocumentKind {
    pub fn from_path(path: Option<&Path>) -> Self {
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

    pub fn label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Json => "JSON",
            Self::PlainText => "Plain Text",
        }
    }

    pub fn menu_label(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown (.md)",
            Self::Json => "JSON (.json)",
            Self::PlainText => "Plain Text (.txt)",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
            Self::PlainText => "txt",
        }
    }

    pub fn supports_outline(self) -> bool {
        !matches!(self, Self::PlainText)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::DocumentKind;

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
    fn exposes_document_controls_without_editor_dependencies() {
        assert!(DocumentKind::Markdown.supports_outline());
        assert!(DocumentKind::Json.supports_outline());
        assert!(!DocumentKind::PlainText.supports_outline());
        assert_eq!(DocumentKind::Markdown.extension(), "md");
        assert_eq!(DocumentKind::Json.extension(), "json");
        assert_eq!(DocumentKind::PlainText.extension(), "txt");
    }
}
