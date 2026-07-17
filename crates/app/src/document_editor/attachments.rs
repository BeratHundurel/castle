use gpui::{
    App, ClipboardEntry, Context, ImageFormat, IntoElement, ObjectFit, Styled as _,
    StyledImage as _, Window, div, img, relative,
};
use gpui_component::input::Paste;
use gpui_component::text::{
    MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast::Node,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write as _},
    ops::Range,
    path::{Component, Path, PathBuf},
};

use crate::DB;

use super::{DocumentEditorView, DocumentKind};

enum ImageImport {
    Clipboard {
        bytes: Vec<u8>,
        extension: &'static str,
    },
    File(PathBuf),
}

struct ImportedImage {
    alt: String,
    path: PathBuf,
}

#[derive(Clone)]
struct LocalImagePreview {
    path: PathBuf,
}

#[derive(Clone)]
pub(super) struct LocalImagePlugin {
    data_dir: PathBuf,
    document_dir: Option<PathBuf>,
}

impl LocalImagePlugin {
    pub(super) fn new(data_dir: PathBuf, document_path: Option<&Path>) -> Self {
        Self {
            data_dir,
            document_dir: document_path.and_then(Path::parent).map(Path::to_path_buf),
        }
    }
}

impl MarkdownPlugin for LocalImagePlugin {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "castle-local-image"
    }

    fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let Node::Paragraph(paragraph) = node else {
            return None;
        };
        let [Node::Image(image)] = paragraph.children.as_slice() else {
            return None;
        };
        let path =
            resolve_local_image_path(&image.url, &self.data_dir, self.document_dir.as_deref())?;

        Some(
            MarkdownNode::new(self.name(), LocalImagePreview { path })
                .text(image.alt.clone())
                .markdown(cx.node_source(node).unwrap_or_default()),
        )
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, _: &mut App) -> impl IntoElement {
        let Some(image) = node.data::<LocalImagePreview>() else {
            return div().into_any_element();
        };

        img(image.path.clone())
            .object_fit(ObjectFit::Contain)
            .max_w(relative(1.))
            .into_any_element()
    }
}

fn resolve_local_image_path(
    url: &str,
    data_dir: &Path,
    document_dir: Option<&Path>,
) -> Option<PathBuf> {
    let path = PathBuf::from(url);
    if path.is_absolute() {
        return Some(path);
    }

    let components = path.components().collect::<Vec<_>>();
    if components.is_empty()
        || components
            .iter()
            .any(|component| !matches!(component, Component::CurDir | Component::Normal(_)))
    {
        return None;
    }

    let first = components.iter().find_map(|component| match component {
        Component::Normal(value) => Some(*value),
        Component::CurDir => None,
        _ => None,
    })?;
    let base_dir = if first == std::ffi::OsStr::new("attachments") {
        data_dir
    } else {
        document_dir?
    };

    Some(base_dir.join(path))
}

impl DocumentEditorView {
    pub(super) fn on_action_paste(
        &mut self,
        _: &Paste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind != DocumentKind::Markdown {
            return;
        }
        let Some(clipboard) = cx.read_from_clipboard() else {
            return;
        };

        let imports = clipboard
            .entries()
            .iter()
            .flat_map(|entry| match entry {
                ClipboardEntry::Image(image) => vec![ImageImport::Clipboard {
                    bytes: image.bytes.clone(),
                    extension: image.format.extension(),
                }],
                ClipboardEntry::ExternalPaths(paths) => paths
                    .paths()
                    .iter()
                    .filter(|path| is_supported_image(path))
                    .cloned()
                    .map(ImageImport::File)
                    .collect(),
                ClipboardEntry::String(_) => Vec::new(),
            })
            .collect::<Vec<_>>();

        if imports.is_empty() {
            return;
        }

        cx.stop_propagation();

        let attachment_dir = cx
            .global::<DB>()
            .data_dir
            .join("attachments")
            .join(self.note_id.to_string());
        let background_executor = cx.background_executor().clone();

        cx.spawn_in(window, async move |this, window| {
            let results = background_executor
                .spawn(async move { import_images(imports, attachment_dir) })
                .await;

            this.update_in(window, |this, window, cx| {
                let mut imported = Vec::new();
                let mut errors = Vec::new();

                for result in results {
                    match result {
                        Ok(image) => imported.push(image),
                        Err(error) => errors.push(error),
                    }
                }

                if imported.is_empty() {
                    if let Some(error) = errors.into_iter().next() {
                        this.save_state = super::SaveState::Error(error.into());
                        cx.notify();
                    }
                    return;
                }

                for error in errors {
                    eprintln!("{error}");
                }

                let markdown = imported
                    .iter()
                    .map(markdown_image)
                    .collect::<Vec<_>>()
                    .join("\n\n");

                this.editor.update(cx, |editor, cx| {
                    let selected_range = editor.selected_range();
                    let current_text = editor.text().to_string();
                    let insertion =
                        markdown_for_insertion(&markdown, &current_text, selected_range);
                    editor.replace(insertion, window, cx);
                    editor.focus(window, cx);
                });
            })
            .ok();
        })
        .detach();
    }
}

fn import_images(
    imports: Vec<ImageImport>,
    attachment_dir: PathBuf,
) -> Vec<Result<ImportedImage, String>> {
    if let Err(error) = fs::create_dir_all(&attachment_dir) {
        return vec![Err(format!(
            "Failed to create image attachment directory {}: {error}",
            attachment_dir.display()
        ))];
    }

    imports
        .into_iter()
        .map(|import| import_image(import, &attachment_dir))
        .collect()
}

fn import_image(import: ImageImport, attachment_dir: &Path) -> Result<ImportedImage, String> {
    match import {
        ImageImport::Clipboard { bytes, extension } => {
            let path =
                write_unique_attachment(attachment_dir, "pasted-image", extension, move |file| {
                    file.write_all(&bytes)
                })
                .map_err(|error| format!("Failed to save pasted image: {error}"))?;
            Ok(ImportedImage {
                alt: "Pasted image".to_string(),
                path,
            })
        }
        ImageImport::File(source) => {
            let extension = normalized_image_extension(&source)
                .ok_or_else(|| format!("Unsupported image file: {}", source.display()))?;
            let stem = source
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(sanitize_file_stem)
                .filter(|stem| !stem.is_empty())
                .unwrap_or_else(|| "image".to_string());
            let mut source_file = File::open(&source)
                .map_err(|error| format!("Failed to open image {}: {error}", source.display()))?;
            let path = write_unique_attachment(attachment_dir, &stem, extension, move |file| {
                io::copy(&mut source_file, file).map(|_| ())
            })
            .map_err(|error| format!("Failed to copy image {}: {error}", source.display()))?;

            Ok(ImportedImage {
                alt: source
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("Image")
                    .to_string(),
                path,
            })
        }
    }
}

fn write_unique_attachment<F>(
    directory: &Path,
    stem: &str,
    extension: &str,
    writer: F,
) -> io::Result<PathBuf>
where
    F: FnOnce(&mut File) -> io::Result<()>,
{
    let mut writer = Some(writer);
    let mut suffix = 1usize;

    loop {
        let file_name = if suffix == 1 {
            format!("{stem}.{extension}")
        } else {
            format!("{stem}-{suffix}.{extension}")
        };
        let path = directory.join(file_name);

        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let Some(writer) = writer.take() else {
                    return Err(io::Error::other("attachment writer was already used"));
                };
                if let Err(error) = writer(&mut file) {
                    let _ = fs::remove_file(&path);
                    return Err(error);
                }
                return Ok(path);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                suffix = suffix.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_supported_image(path: &Path) -> bool {
    normalized_image_extension(path).is_some()
}

fn normalized_image_extension(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?;
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png.extension()),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg.extension()),
        "webp" => Some(ImageFormat::Webp.extension()),
        "gif" => Some(ImageFormat::Gif.extension()),
        "svg" => Some(ImageFormat::Svg.extension()),
        "bmp" => Some(ImageFormat::Bmp.extension()),
        "tif" | "tiff" => Some(ImageFormat::Tiff.extension()),
        "ico" => Some(ImageFormat::Ico.extension()),
        "pnm" | "pbm" | "pgm" | "ppm" => Some(ImageFormat::Pnm.extension()),
        _ => None,
    }
}

fn sanitize_file_stem(stem: &str) -> String {
    let mut sanitized = String::with_capacity(stem.len());
    let mut previous_was_separator = false;

    for character in stem.chars() {
        if character.is_alphanumeric() || matches!(character, '-' | '_') {
            sanitized.push(character);
            previous_was_separator = false;
        } else if !previous_was_separator {
            sanitized.push('-');
            previous_was_separator = true;
        }
    }

    sanitized.trim_matches('-').to_string()
}

fn markdown_image(image: &ImportedImage) -> String {
    let alt = image
        .alt
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]");
    let path = image.path.to_string_lossy().replace('\\', "/");
    format!("![{alt}](<{path}>)")
}

fn markdown_for_insertion(markdown: &str, text: &str, range: Range<usize>) -> String {
    let before = &text[..range.start];
    let after = &text[range.end..];
    let prefix = if before.is_empty() || before.ends_with("\n\n") {
        ""
    } else if before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let suffix = if after.is_empty() || after.starts_with("\n\n") {
        ""
    } else if after.starts_with('\n') {
        "\n"
    } else {
        "\n\n"
    };

    format!("{prefix}{markdown}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::{
        ImportedImage, markdown_for_insertion, markdown_image, normalized_image_extension,
        resolve_local_image_path, sanitize_file_stem, write_unique_attachment,
    };
    use std::{
        fs,
        io::Write as _,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn recognizes_supported_image_extensions_case_insensitively() {
        assert_eq!(
            normalized_image_extension(Path::new("photo.JPEG")),
            Some("jpg")
        );
        assert_eq!(
            normalized_image_extension(Path::new("diagram.svg")),
            Some("svg")
        );
        assert_eq!(normalized_image_extension(Path::new("notes.txt")), None);
    }

    #[test]
    fn sanitizes_attachment_file_names() {
        assert_eq!(
            sanitize_file_stem("A screenshot (final)"),
            "A-screenshot-final"
        );
        assert_eq!(sanitize_file_stem("  board   view  "), "board-view");
    }

    #[test]
    fn creates_markdown_for_windows_paths_and_escaped_alt_text() {
        let image = ImportedImage {
            alt: "Board [today]".to_string(),
            path: PathBuf::from(r"C:\Castle Data\attachments\board.png"),
        };

        assert_eq!(
            markdown_image(&image),
            r"![Board \[today\]](<C:/Castle Data/attachments/board.png>)"
        );
    }

    #[test]
    fn separates_inserted_images_from_surrounding_markdown() {
        assert_eq!(
            markdown_for_insertion("![Image](<image.png>)", "beforeafter", 6..6),
            "\n\n![Image](<image.png>)\n\n"
        );
        assert_eq!(
            markdown_for_insertion("![Image](<image.png>)", "before\n\nafter", 8..8),
            "![Image](<image.png>)\n\n"
        );
    }

    #[test]
    fn keeps_existing_attachments_when_names_collide() -> Result<(), Box<dyn std::error::Error>> {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "castle-attachment-test-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&directory)?;

        let first =
            write_unique_attachment(&directory, "image", "png", |file| file.write_all(b"first"))?;
        let second =
            write_unique_attachment(&directory, "image", "png", |file| file.write_all(b"second"))?;

        assert_eq!(first.file_name(), Some(std::ffi::OsStr::new("image.png")));
        assert_eq!(
            second.file_name(),
            Some(std::ffi::OsStr::new("image-2.png"))
        );
        assert_eq!(fs::read(first)?, b"first");
        assert_eq!(fs::read(second)?, b"second");

        fs::remove_dir_all(directory)?;
        Ok(())
    }

    #[test]
    fn resolves_castle_and_document_relative_image_paths() {
        let data_dir = std::env::temp_dir().join("castle-data");
        let document_dir = data_dir.join("notes");

        assert_eq!(
            resolve_local_image_path("attachments/20/image.png", &data_dir, Some(&document_dir)),
            Some(data_dir.join("attachments/20/image.png"))
        );
        assert_eq!(
            resolve_local_image_path("images/diagram.png", &data_dir, Some(&document_dir)),
            Some(document_dir.join("images/diagram.png"))
        );
        assert_eq!(
            resolve_local_image_path("../private.png", &data_dir, Some(&document_dir)),
            None
        );
    }
}
