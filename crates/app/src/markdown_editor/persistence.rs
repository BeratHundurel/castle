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
        let background_executor = cx.background_executor().clone();

        cx.spawn_in(window, async move |_, window| {
            let model = Note::find_by_id(note_id as i64).one(&*db).await.ok()??;
            let path = model.file_path.as_ref().map(PathBuf::from);
            let cached_content = model.cached_content.clone();

            let cached_epoch = if path.is_none() || !cached_content.is_empty() {
                window
                    .update(|window, cx| {
                        view.update(cx, |this, cx| {
                            this.load_model(
                                model.clone(),
                                cached_content.clone(),
                                false,
                                false,
                                window,
                                cx,
                            );
                            this.auto_save_epoch
                        })
                    })
                    .ok()
            } else {
                None
            };

            let Some(path) = path else {
                return Some(());
            };

            match background_executor
                .spawn(async move { read_to_string(path) })
                .await
            {
                Ok(content) => {
                    if model.cached_content != content || model.file_missing_since.is_some() {
                        let _ = note::ActiveModel {
                            id: Set(note_id as i64),
                            cached_content: Set(content.clone()),
                            file_missing_since: Set(None),
                            updated_at: Set(now_ts()),
                            ..Default::default()
                        }
                        .update(&*db)
                        .await;
                    }

                    if cached_epoch.is_some()
                        && model.cached_content == content
                        && model.file_missing_since.is_none()
                    {
                        return Some(());
                    }

                    window
                        .update(|window, cx| {
                            view.update(cx, |this, cx| {
                                if let Some(expected_epoch) = cached_epoch
                                    && this.auto_save_epoch != expected_epoch
                                {
                                    return;
                                }

                                this.load_model(model, content, false, false, window, cx);
                            })
                        })
                        .ok()?;
                }
                Err(_) => {
                    if model.file_missing_since.is_none() {
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
                                if let Some(expected_epoch) = cached_epoch
                                    && this.auto_save_epoch != expected_epoch
                                {
                                    return;
                                }

                                if cached_epoch.is_some() {
                                    this.mark_file_missing(cx);
                                } else {
                                    this.load_model(model, cached_content, true, false, window, cx);
                                }
                            })
                        })
                        .ok()?;
                }
            }

            Some(())
        })
        .detach();
    }

    pub(super) fn load_model(
        &mut self,
        model: note::Model,
        content: String,
        missing: bool,
        is_loading: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.title = model.title.into();
        self.current_path = model.file_path.map(PathBuf::from);
        self.file_managed_by_app = model.file_managed_by_app;
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        self.is_loading = is_loading;

        self.save_state = if missing {
            SaveState::Missing
        } else {
            SaveState::Saved
        };

        self.stats = DocumentStats::from_text(&content);

        self.suppress_editor_events = true;
        self.editor.update(cx, |editor, cx| {
            editor.set_highlighter(Language::Markdown, cx);
            editor.set_value(content.as_str(), window, cx);
            editor.focus(window, cx);
        });
        self.suppress_editor_events = false;

        self.preview.update(cx, |preview, cx| {
            preview.set_text(content.as_ref(), cx);
        });

        cx.notify();
    }

    pub(super) fn mark_file_missing(&mut self, cx: &mut Context<Self>) {
        self.is_loading = false;
        self.save_state = SaveState::Missing;
        cx.notify();
    }

    pub(super) fn update_from_editor(&mut self, cx: &mut Context<Self>) {
        if self.is_loading {
            return;
        }

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

            let result = if let Some(path) = path
                && !is_missing
            {
                let content_to_write = content.to_string();
                let write_result = cx
                    .background_executor()
                    .spawn(async move {
                        if let Some(parent) = path.parent() {
                            create_dir_all(parent).map_err(|err| err.to_string())?;
                        }
                        write(path, content_to_write).map_err(|err| err.to_string())
                    })
                    .await;

                match write_result {
                    Ok(()) => note::ActiveModel {
                        id: Set(note_id as i64),
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
                }
            } else {
                note::ActiveModel {
                    id: Set(note_id as i64),
                    cached_content: Set(content.to_string()),
                    updated_at: Set(now_ts()),
                    ..Default::default()
                }
                .update(&*db)
                .await
                .map(|_| ())
                .map_err(|err| err.to_string())
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
        }));
    }

    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        let (path, file_managed_by_app) = self
            .current_path
            .clone()
            .map(|path| (path, self.file_managed_by_app))
            .unwrap_or_else(|| {
                (
                    unique_note_path(
                        cx.global::<DB>().data_dir.join("notes"),
                        self.title.as_ref(),
                    ),
                    true,
                )
            });
        self.save_to_path(path, file_managed_by_app, cx);
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
                        this.save_to_path(path, true, cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    pub(super) fn save_to_path(
        &mut self,
        path: PathBuf,
        file_managed_by_app: bool,
        cx: &mut Context<Self>,
    ) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        self.save_state = SaveState::Saving;

        let content = self.editor.read(cx).value();
        let note_id = self.note_id;
        let db = cx.global::<DB>().conn.clone();
        let background_executor = cx.background_executor().clone();
        let saved_path = path.clone();
        let path_string = path.display().to_string();

        cx.notify();

        cx.spawn(async move |this, cx| {
            let content_to_write = content.to_string();
            let write_path = path.clone();
            let result = background_executor
                .spawn(async move {
                    if let Some(parent) = write_path.parent() {
                        create_dir_all(parent).map_err(|err| err.to_string())?;
                    }
                    write(write_path, content_to_write).map_err(|err| err.to_string())
                })
                .await;

            let result = match result {
                Ok(()) => note::ActiveModel {
                    id: Set(note_id as i64),
                    file_path: Set(Some(path_string)),
                    file_managed_by_app: Set(file_managed_by_app),
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
                        this.current_path = Some(saved_path);
                        this.file_managed_by_app = file_managed_by_app;
                        this.is_loading = false;
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
