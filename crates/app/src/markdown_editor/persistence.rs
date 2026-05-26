use entity::{note, note::Entity as Note};
use gpui::{Context, SharedString, Window};
use gpui_component::highlighter::Language;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use std::{
    fs::read_to_string,
    fs::{create_dir_all, write},
    path::PathBuf,
};

use crate::DB;

use super::types::{DocumentStats, SaveState};
use super::util::{now_ts, suggested_file_name, unique_note_path};
use super::{AUTO_SAVE_IDLE_DELAY, MarkdownEditorView};

impl MarkdownEditorView {
    pub(super) fn load_note_async(note_id: u32, window: &mut Window, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let model = Note::find_by_id(note_id as i64).one(&*db).await.ok()??;
            let path = model.file_path.as_ref().map(PathBuf::from);
            let (content, missing) = match path.as_ref() {
                Some(path) => match read_to_string(path) {
                    Ok(content) => (content, false),
                    Err(_) => (model.cached_content.clone(), true),
                },
                None => (model.cached_content.clone(), false),
            };

            if missing && model.file_missing_since.is_none() {
                let _ = note::ActiveModel {
                    id: Set(note_id as i64),
                    file_missing_since: Set(Some(now_ts())),
                    ..Default::default()
                }
                .update(&*db)
                .await;
            }

            window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.load_model(model, content, missing, window, cx);
                    })
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    pub(super) fn load_model(
        &mut self,
        model: note::Model,
        content: String,
        missing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.title = model.title.into();
        self.current_path = model.file_path.map(PathBuf::from);
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);

        self.save_state = if missing {
            SaveState::Missing
        } else {
            SaveState::Saved
        };

        self.stats = DocumentStats::from_text(&content);

        self.editor.update(cx, |editor, cx| {
            editor.set_highlighter(Language::Markdown, cx);
            editor.set_value(content.as_str(), window, cx);
            editor.focus(window, cx);
        });

        self.preview.update(cx, |preview, cx| {
            preview.set_text(content.as_ref(), cx);
        });

        cx.notify();
    }

    pub(super) fn update_from_editor(&mut self, cx: &mut Context<Self>) {
        let value = self.editor.read(cx).value();

        self.preview.update(cx, |preview, cx| {
            preview.set_text(value.as_ref(), cx);
        });

        self.stats = DocumentStats::from_text(value.as_ref());

        let old_save_state = self.save_state.clone();
        if !matches!(self.save_state, SaveState::Missing) {
            self.save_state = SaveState::Dirty;
        }

        if self.save_state != old_save_state {
            cx.notify();
        }

        self.schedule_auto_save(cx);
    }

    pub(super) fn schedule_auto_save(&mut self, cx: &mut Context<Self>) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        let epoch = self.auto_save_epoch;

        self._auto_save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTO_SAVE_IDLE_DELAY).await;

            let save_request = this
                .update(cx, |this, cx| {
                    if this.auto_save_epoch != epoch {
                        return None;
                    }

                    let note_id = this.note_id;
                    let path = this.current_path.clone();
                    let is_missing = matches!(this.save_state, SaveState::Missing);
                    let content = this.editor.read(cx).value();

                    if path.is_some() && !is_missing {
                        this.save_state = SaveState::Saving;
                        cx.notify();
                    }

                    Some((note_id, path, is_missing, content))
                })
                .ok()
                .flatten();

            let Some((note_id, path, is_missing, content)) = save_request else {
                return;
            };

            let db = this
                .read_with(cx, |_, cx| cx.global::<DB>().conn.clone())
                .ok();

            let Some(db) = db else {
                return;
            };

            let mut write_result = Ok(());
            if let Some(path) = path.as_ref()
                && !is_missing
            {
                if let Some(parent) = path.parent() {
                    write_result = create_dir_all(parent).map_err(|err| err.to_string());
                }
                if write_result.is_ok() {
                    write_result = write(path, content.to_string()).map_err(|err| err.to_string());
                }
            }

            let cache_result = note::ActiveModel {
                id: Set(note_id as i64),
                cached_content: Set(content.to_string()),
                updated_at: Set(now_ts()),
                ..Default::default()
            }
            .update(&*db)
            .await
            .map(|_| ())
            .map_err(|err| err.to_string());

            let result = write_result.and(cache_result);

            match result {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        this.save_state = this.resolve_save_state(&content, cx);
                    })
                    .ok();
                }
                Err(err) => {
                    this.update(cx, |this, _cx| {
                        this.save_state = SaveState::Error(err.into());
                    })
                    .ok();
                }
            }
        }));
    }

    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        let path = self.current_path.clone().unwrap_or_else(|| {
            unique_note_path(
                cx.global::<DB>().data_dir.join("notes"),
                self.title.as_ref(),
            )
        });
        self.save_to_path(path, cx);
    }

    pub(super) fn save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let start_dir = self
            .current_path
            .as_ref()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(|| cx.global::<DB>().data_dir.join("notes"));

        let file_name = suggested_file_name(self.title.as_ref());
        let receiver = cx.prompt_for_new_path(&start_dir, Some(&file_name));
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let path = receiver.await.ok().into_iter().flatten().flatten().next()?;
            window
                .update(|_, cx| {
                    view.update(cx, |this, cx| {
                        this.save_to_path(path, cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    pub(super) fn save_to_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        self.save_state = SaveState::Saving;

        let content = self.editor.read(cx).value();
        let note_id = self.note_id;
        let db = cx.global::<DB>().conn.clone();

        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = (|| {
                if let Some(parent) = path.parent() {
                    create_dir_all(parent).map_err(|err| err.to_string())?;
                }
                write(&path, content.to_string()).map_err(|err| err.to_string())?;
                Ok(())
            })();

            let result = match result {
                Ok(()) => note::ActiveModel {
                    id: Set(note_id as i64),
                    file_path: Set(Some(path.display().to_string())),
                    cached_content: Set(content.to_string()),
                    file_missing_since: Set(None),
                    updated_at: Set(now_ts()),
                    ..Default::default()
                }
                .update(&*db)
                .await
                .map(|_| ())
                .map_err(|err| err.to_string()),

                Err(err) => Err(err),
            };

            match result {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        this.save_state = this.resolve_save_state(&content, cx);
                    })
                    .ok();
                }
                Err(err) => {
                    this.update(cx, |this, _cx| {
                        this.save_state = SaveState::Error(err.into());
                    })
                    .ok();
                }
            }

            this.update(cx, |this, cx| {
                this.save_state = this.resolve_save_state(&content, cx);
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn resolve_save_state(
        &self,
        saved_content: &SharedString,
        cx: &mut Context<Self>,
    ) -> SaveState {
        let current = self.editor.read(cx).value();
        if current == *saved_content {
            SaveState::Saved
        } else {
            SaveState::Dirty
        }
    }
}
