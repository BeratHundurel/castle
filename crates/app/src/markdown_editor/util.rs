use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

pub(crate) fn suggested_file_name(title: &str) -> String {
    let stem = if title.trim().is_empty() {
        "untitled"
    } else {
        title.trim()
    };
    let mut file_name = String::with_capacity(stem.len() + 3);

    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            file_name.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() {
            file_name.push('-');
        }
    }

    if file_name.is_empty() {
        file_name.push_str("untitled");
    }
    if !file_name.ends_with(".md") {
        file_name.push_str(".md");
    }
    file_name
}

pub(crate) fn unique_note_path(dir: PathBuf, title: &str) -> PathBuf {
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

pub(crate) fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}
