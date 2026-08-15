use std::path::{Path, PathBuf};

pub(crate) fn suggested_file_name(title: &str) -> String {
    storage::workspace::suggested_note_file_name(title, "md")
}

pub(crate) fn suggested_save_as_file_name(path: Option<&Path>, title: &str) -> String {
    path.and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| suggested_file_name(title))
}

pub(crate) fn suggested_save_as_file_name_with_extension(
    path: Option<&Path>,
    title: &str,
    extension: &str,
) -> String {
    let mut file_name = PathBuf::from(suggested_save_as_file_name(path, title));
    file_name.set_extension(extension);
    file_name.to_string_lossy().into_owned()
}

pub fn unique_note_path(dir: PathBuf, title: &str) -> PathBuf {
    let file_name = suggested_file_name(title);
    let candidate = dir.join(&file_name);
    if !candidate.exists() {
        return candidate;
    }

    let stem = Path::new(&file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("untitled");

    let extension = Path::new(&file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("md");

    for index in 2.. {
        let candidate = dir.join(format!("{stem}-{index}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    dir.join(file_name)
}

#[cfg(test)]
mod tests {
    use super::{
        suggested_file_name, suggested_save_as_file_name,
        suggested_save_as_file_name_with_extension,
    };
    use std::path::Path;

    #[test]
    fn save_as_preserves_the_current_filename() {
        assert_eq!(
            suggested_save_as_file_name(Some(Path::new("notes/config.JSON")), "Config"),
            "config.JSON"
        );
        assert_eq!(suggested_save_as_file_name(None, "New Note"), "new-note.md");
        assert_eq!(suggested_file_name(""), "untitled.md");
        assert_eq!(
            suggested_save_as_file_name_with_extension(
                Some(Path::new("notes/config.MD")),
                "Config",
                "json"
            ),
            "config.json"
        );
    }
}
