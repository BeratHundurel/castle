use std::{
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
};

use entity::{entry_attachment, entry_attachment::Entity as EntryAttachment};
use gpui::{Context, ImageFormat, PathPromptOptions, Window};
use gpui_component::{WindowExt as _, notification::Notification};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait, TransactionTrait};

use crate::DB;

use super::{
    BoardView,
    dto::{CardDTO, EntryAttachmentDTO},
};

const ATTACHMENT_THUMBNAIL_WIDTH: u32 = 504;
const ATTACHMENT_THUMBNAIL_HEIGHT: u32 = 300;

#[derive(Clone)]
struct CopiedImage {
    file_name: String,
    path: PathBuf,
}

impl BoardView {
    pub(super) fn add_image_attachments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };
        if entry_id > i32::MAX as u32 {
            return;
        }

        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach images".into()),
        });
        let app_db = cx.global::<DB>();
        let db = app_db.conn.clone();
        let data_dir = app_db.data_dir.clone();
        let destination = attachment_directory(&app_db.data_dir, entry_id);
        let background = cx.background_executor().clone();
        let runtime = tokio::runtime::Handle::current();

        cx.spawn_in(window, async move |this, cx| {
            let Some(paths) = paths.await.ok().and_then(Result::ok).flatten() else {
                return;
            };
            let copied = background
                .spawn(async move { copy_images(paths, destination) })
                .await;
            let mut errors = copied
                .iter()
                .filter_map(|result| result.as_ref().err().cloned())
                .collect::<Vec<_>>();
            let copied = copied
                .into_iter()
                .filter_map(Result::ok)
                .collect::<Vec<_>>();

            if copied.is_empty() {
                if let Some(error) = errors.into_iter().next() {
                    cx.update(|window, cx| {
                        window.push_notification(Notification::error(error), cx);
                    })
                    .ok();
                }
                return;
            }

            let copied_for_cleanup = copied.clone();
            let persistence = runtime
                .spawn(async move {
                    let transaction = db.begin().await?;
                    let mut attachments = Vec::with_capacity(copied.len());
                    for image in copied {
                        let inserted = entry_attachment::ActiveModel {
                            entry_id: Set(entry_id as i64),
                            file_name: Set(image.file_name),
                            ..Default::default()
                        }
                        .insert(&transaction)
                        .await?;
                        attachments.push(EntryAttachmentDTO::from(inserted));
                    }
                    transaction.commit().await?;
                    Ok::<_, sea_orm::DbErr>(attachments)
                })
                .await;

            match persistence {
                Ok(Ok(attachments)) => {
                    let count = attachments.len();
                    this.update_in(cx, |this, window, cx| {
                        if let Some(entry) = this
                            .cards
                            .iter_mut()
                            .flat_map(|list| list.entries.iter_mut())
                            .find(|entry| entry.id == entry_id)
                        {
                            for attachment in &attachments {
                                let original = attachment_path(
                                    &data_dir,
                                    attachment.entry_id,
                                    attachment.file_name.as_ref(),
                                );
                                let thumbnail = attachment_thumbnail_path(
                                    &attachment_directory(&data_dir, attachment.entry_id),
                                    attachment.file_name.as_ref(),
                                );
                                this.attachment_preview_paths.insert(
                                    attachment.id,
                                    if thumbnail.exists() {
                                        thumbnail
                                    } else {
                                        original
                                    },
                                );
                            }
                            entry.attachments.extend(attachments);
                            cx.notify();
                        }
                        for error in errors.drain(..) {
                            window.push_notification(Notification::warning(error), cx);
                        }
                        window.push_notification(
                            Notification::success(format!(
                                "Attached {count} image{}",
                                if count == 1 { "" } else { "s" }
                            )),
                            cx,
                        );
                    })
                    .ok();
                }
                Ok(Err(error)) => {
                    cleanup_copies(&background, copied_for_cleanup).await;
                    cx.update(|window, cx| {
                        window.push_notification(
                            Notification::error(format!("Could not save attachments: {error}")),
                            cx,
                        );
                    })
                    .ok();
                }
                Err(error) => {
                    cleanup_copies(&background, copied_for_cleanup).await;
                    cx.update(|window, cx| {
                        window.push_notification(
                            Notification::error(format!("Attachment task failed: {error}")),
                            cx,
                        );
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    pub(super) fn delete_image_attachment(
        &mut self,
        attachment_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(attachment) = self
            .cards
            .iter()
            .flat_map(|list| list.entries.iter())
            .flat_map(|entry| entry.attachments.iter())
            .find(|attachment| attachment.id == attachment_id)
            .cloned()
        else {
            return;
        };

        let app_db = cx.global::<DB>();
        let db = app_db.conn.clone();
        let path = attachment_path(
            &app_db.data_dir,
            attachment.entry_id,
            attachment.file_name.as_ref(),
        );
        let thumbnail = attachment_thumbnail_path(
            &attachment_directory(&app_db.data_dir, attachment.entry_id),
            attachment.file_name.as_ref(),
        );
        let background = cx.background_executor().clone();
        let runtime = tokio::runtime::Handle::current();

        cx.spawn_in(window, async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    EntryAttachment::delete_by_id(attachment_id as i64)
                        .exec(&*db)
                        .await
                })
                .await;

            match result {
                Ok(Ok(_)) => {
                    let _ = background
                        .spawn(async move {
                            let _ = fs::remove_file(path);
                            let _ = fs::remove_file(thumbnail);
                        })
                        .await;
                    this.update_in(cx, |this, _, cx| {
                        if let Some(entry) = this
                            .cards
                            .iter_mut()
                            .flat_map(|list| list.entries.iter_mut())
                            .find(|entry| entry.id == attachment.entry_id)
                        {
                            entry.attachments.retain(|item| item.id != attachment_id);
                            this.attachment_preview_paths.remove(&attachment_id);
                            cx.notify();
                        }
                    })
                    .ok();
                }
                Ok(Err(error)) => {
                    cx.update(|window, cx| {
                        window.push_notification(
                            Notification::error(format!("Could not remove attachment: {error}")),
                            cx,
                        );
                    })
                    .ok();
                }
                Err(error) => {
                    cx.update(|window, cx| {
                        window.push_notification(
                            Notification::error(format!("Attachment task failed: {error}")),
                            cx,
                        );
                    })
                    .ok();
                }
            }
        })
        .detach();
    }
}

impl BoardView {
    pub(super) fn prepare_attachment_previews(&mut self, entry_id: u32, cx: &mut Context<Self>) {
        let data_dir = cx.global::<DB>().data_dir.clone();
        let generation = self.load_generation;
        let board_id = self.board_id;
        let attachments = attachment_preview_specs(&self.cards, &data_dir, entry_id);

        self.attachment_preview_paths.clear();
        if attachments.is_empty() {
            return;
        }

        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let previews = background
                .spawn(async move {
                    attachments
                        .into_iter()
                        .map(|(attachment_id, original, thumbnail)| {
                            let preview = ensure_attachment_thumbnail(&original, &thumbnail)
                                .unwrap_or(original);
                            (attachment_id, preview)
                        })
                        .collect::<Vec<_>>()
                })
                .await;

            this.update(cx, |this, cx| {
                if this.board_id != board_id || this.load_generation != generation {
                    return;
                }
                this.attachment_preview_paths.extend(previews);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn attachment_preview_specs(
    cards: &[CardDTO],
    data_dir: &Path,
    entry_id: u32,
) -> Vec<(u32, PathBuf, PathBuf)> {
    cards
        .iter()
        .flat_map(|list| list.entries.iter())
        .find(|entry| entry.id == entry_id)
        .into_iter()
        .flat_map(|entry| entry.attachments.iter())
        .map(|attachment| {
            (
                attachment.id,
                attachment_path(data_dir, attachment.entry_id, attachment.file_name.as_ref()),
                attachment_thumbnail_path(
                    &attachment_directory(data_dir, attachment.entry_id),
                    attachment.file_name.as_ref(),
                ),
            )
        })
        .collect()
}

pub(super) fn attachment_directory(data_dir: &Path, entry_id: u32) -> PathBuf {
    data_dir
        .join("attachments")
        .join("entries")
        .join(entry_id.to_string())
}

pub(super) fn attachment_path(data_dir: &Path, entry_id: u32, file_name: &str) -> PathBuf {
    attachment_directory(data_dir, entry_id).join(file_name)
}

async fn cleanup_copies(background: &gpui::BackgroundExecutor, copies: Vec<CopiedImage>) {
    let _ = background
        .spawn(async move {
            for image in copies {
                let _ = fs::remove_file(image.path);
            }
        })
        .await;
}

fn copy_images(paths: Vec<PathBuf>, destination: PathBuf) -> Vec<Result<CopiedImage, String>> {
    if let Err(error) = fs::create_dir_all(&destination) {
        return vec![Err(format!(
            "Could not create attachment folder {}: {error}",
            destination.display()
        ))];
    }

    paths
        .into_iter()
        .map(|source| copy_image(source, &destination))
        .collect()
}

fn copy_image(source: PathBuf, destination: &Path) -> Result<CopiedImage, String> {
    let extension = normalized_image_extension(&source)
        .ok_or_else(|| format!("Unsupported image file: {}", source.display()))?;
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(sanitize_file_stem)
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| "image".to_string());
    let mut source_file = File::open(&source)
        .map_err(|error| format!("Could not open {}: {error}", source.display()))?;
    let path = write_unique_file(destination, &stem, extension, move |file| {
        io::copy(&mut source_file, file).map(|_| ())
    })
    .map_err(|error| format!("Could not copy {}: {error}", source.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "The attachment file name is not valid Unicode".to_string())?
        .to_string();

    let thumbnail = attachment_thumbnail_path(destination, &file_name);
    let _ = ensure_attachment_thumbnail(&path, &thumbnail);

    Ok(CopiedImage { file_name, path })
}

fn attachment_thumbnail_path(directory: &Path, file_name: &str) -> PathBuf {
    directory
        .join(".thumbnails")
        .join(format!("{file_name}.png"))
}

fn ensure_attachment_thumbnail(source: &Path, thumbnail: &Path) -> Result<PathBuf, String> {
    if thumbnail.is_file() {
        return Ok(thumbnail.to_path_buf());
    }

    let image = image::open(source)
        .map_err(|error| format!("Could not decode {}: {error}", source.display()))?;
    let preview = image.thumbnail(ATTACHMENT_THUMBNAIL_WIDTH, ATTACHMENT_THUMBNAIL_HEIGHT);
    let Some(directory) = thumbnail.parent() else {
        return Err("The attachment thumbnail path has no parent folder".to_string());
    };
    fs::create_dir_all(directory).map_err(|error| {
        format!(
            "Could not create attachment thumbnail folder {}: {error}",
            directory.display()
        )
    })?;

    let file_name = thumbnail
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "The attachment thumbnail name is not valid Unicode".to_string())?;
    let temporary = write_unique_file(directory, file_name, "tmp", |file| {
        preview
            .write_to(file, image::ImageFormat::Png)
            .map_err(io::Error::other)
    })
    .map_err(|error| {
        format!(
            "Could not write thumbnail for {}: {error}",
            source.display()
        )
    })?;

    match fs::rename(&temporary, thumbnail) {
        Ok(()) => Ok(thumbnail.to_path_buf()),
        Err(_) if thumbnail.is_file() => {
            let _ = fs::remove_file(temporary);
            Ok(thumbnail.to_path_buf())
        }
        Err(error) => {
            let _ = fs::remove_file(temporary);
            Err(format!(
                "Could not finish thumbnail for {}: {error}",
                source.display()
            ))
        }
    }
}

fn write_unique_file<F>(
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

fn normalized_image_extension(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use image::GenericImageView as _;

    use super::{
        ATTACHMENT_THUMBNAIL_HEIGHT, ATTACHMENT_THUMBNAIL_WIDTH, attachment_preview_specs,
        attachment_thumbnail_path, ensure_attachment_thumbnail, normalized_image_extension,
        sanitize_file_stem,
    };
    use crate::board::dto::{CardDTO, EntryAttachmentDTO, EntryDTO};

    #[test]
    fn accepts_supported_images_case_insensitively() {
        assert_eq!(
            normalized_image_extension(Path::new("photo.JPEG")),
            Some("jpg")
        );
        assert_eq!(normalized_image_extension(Path::new("notes.txt")), None);
    }

    #[test]
    fn sanitizes_attachment_names() {
        assert_eq!(
            sanitize_file_stem("Board capture (final)"),
            "Board-capture-final"
        );
    }

    #[test]
    fn thumbnail_caps_decoded_attachment_memory_without_touching_original() -> anyhow::Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("large-board-image.png");
        image::DynamicImage::new_rgb8(1_600, 1_200).save(&source)?;
        let original_bytes = std::fs::read(&source)?;
        let thumbnail = attachment_thumbnail_path(directory.path(), "large-board-image.png");

        ensure_attachment_thumbnail(&source, &thumbnail).map_err(anyhow::Error::msg)?;

        let preview = image::open(&thumbnail)?;
        assert!(preview.width() <= ATTACHMENT_THUMBNAIL_WIDTH);
        assert!(preview.height() <= ATTACHMENT_THUMBNAIL_HEIGHT);
        assert!(
            u64::from(preview.width()) * u64::from(preview.height()) * 4
                <= u64::from(ATTACHMENT_THUMBNAIL_WIDTH)
                    * u64::from(ATTACHMENT_THUMBNAIL_HEIGHT)
                    * 4
        );
        assert_eq!(std::fs::read(&source)?, original_bytes);
        assert_eq!(image::open(&source)?.dimensions(), (1_600, 1_200));
        Ok(())
    }

    #[test]
    fn thumbnail_work_is_limited_to_the_open_card() {
        let entry = |id, attachment_id, file_name: &'static str| EntryDTO {
            id,
            title: "Entry".into(),
            description: "".into(),
            card_id: 1,
            position: 0,
            due_on: None,
            reminder_enabled: false,
            labels: vec![],
            checklist_items: vec![],
            attachments: vec![EntryAttachmentDTO {
                id: attachment_id,
                entry_id: id,
                file_name: file_name.into(),
            }],
        };
        let cards = vec![CardDTO {
            id: 1,
            title: "List".into(),
            board_id: 1,
            position: 0,
            entries: vec![entry(10, 100, "open.png"), entry(20, 200, "closed.png")],
        }];

        let specs = attachment_preview_specs(&cards, Path::new("data"), 10);

        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].0, 100);
        assert!(specs[0].1.ends_with("open.png"));
    }
}
