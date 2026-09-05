use gpui::{Context, SharedString, Task, Window};
use gpui_component::{WindowExt as _, input::RopeExt as _, notification::Notification};
use std::{
    fs::read_to_string,
    fs::{create_dir_all, remove_file, write},
    path::PathBuf,
    sync::Arc,
};

use runtime::AppRuntime;

use super::file_paths::{
    suggested_save_as_file_name, suggested_save_as_file_name_with_extension, unique_note_path,
};
use super::outline::DocumentOutline;
use super::{AUTO_SAVE_IDLE_DELAY, DocumentEditorEvent, DocumentEditorView};
use super::{
    DocumentKind,
    document_state::{DocumentStats, SaveState},
};

impl DocumentEditorView {
    pub(super) fn load_note_async(
        note_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let app_runtime = cx.global::<AppRuntime>().clone();
        let db = app_runtime.store();

        cx.spawn_in(window, async move |this, window| {
            let query_db = db.clone();
            let (cancel_on_drop, cancelled) = tokio::sync::oneshot::channel::<()>();
            let load = app_runtime.spawn_tokio(window.background_executor(), async move {
                tokio::select! {
                    biased;
                    _ = cancelled => None,
                    result = storage::note::documents::load_document(&query_db, note_id) => Some(result),
                }
            });
            let model = match load.await {
                Ok(Some(Ok(Some(model)))) => model,
                Ok(Some(Ok(None))) => {
                    let message = format!("Note {note_id} was not found.");
                    eprintln!("{message}");
                    this.update_in(window, |this, _, cx| this.fail_load(message, cx))
                        .ok();
                    return;
                }
                Ok(Some(Err(err))) => {
                    let message = format!("Failed to load note {note_id}: {err}");
                    eprintln!("{message}");
                    this.update_in(window, |this, _, cx| this.fail_load(message, cx))
                        .ok();
                    return;
                }
                Ok(None) => return,
                Err(err) => {
                    let message = format!("Failed to load note {note_id}: {err}");
                    eprintln!("{message}");
                    this.update_in(window, |this, _, cx| this.fail_load(message, cx))
                        .ok();
                    return;
                }
            };
            drop(cancel_on_drop);

            let path = model.file_path.as_ref().map(PathBuf::from);
            let cached_content = model.cached_content.clone();

            let cached_epoch = if path.is_none() || !cached_content.is_empty() {
                this.update_in(window, |this, window, cx| {
                    this.load_model(
                        model.clone(),
                        cached_content.clone(),
                        false,
                        false,
                        window,
                        cx,
                    );
                    this.persistence.auto_save_epoch
                })
                .ok()
            } else {
                None
            };

            let Some(path) = path else {
                return;
            };

            match window
                .background_executor()
                .spawn(async move { read_to_string(path) })
                .await
            {
                Ok(content) => {
                    if model.cached_content != content || model.file_missing_since.is_some() {
                        let update_db = db.clone();
                        let update_content = content.clone();
                        let _ = app_runtime.spawn_tokio(window.background_executor(), async move {
                            storage::note::documents::persist_document_content(
                                &update_db,
                                note_id,
                                update_content,
                                true,
                            )
                            .await
                        })
                        .await;
                    }

                    if cached_epoch.is_some()
                        && model.cached_content == content
                        && model.file_missing_since.is_none()
                    {
                        return;
                    }

                    this.update_in(window, |this, window, cx| {
                        if let Some(expected_epoch) = cached_epoch
                            && this.persistence.auto_save_epoch != expected_epoch
                        {
                            return;
                        }

                        this.load_model(model, content, false, false, window, cx);
                    })
                    .ok();
                }
                Err(_) => {
                    if model.file_missing_since.is_none() {
                        let update_db = db.clone();
                        let _ = app_runtime.spawn_tokio(window.background_executor(), async move {
                            storage::note::documents::mark_document_missing(&update_db, note_id)
                                .await
                        })
                        .await;
                    }

                    this.update_in(window, |this, window, cx| {
                        if let Some(expected_epoch) = cached_epoch
                            && this.persistence.auto_save_epoch != expected_epoch
                        {
                            return;
                        }

                        if cached_epoch.is_some() {
                            this.mark_file_missing(cx);
                        } else {
                            this.load_model(model, cached_content, true, false, window, cx);
                        }
                    })
                    .ok();
                }
            }
        })
    }

    pub(super) fn load_model(
        &mut self,
        model: storage::note::documents::DocumentRecord,
        content: String,
        missing: bool,
        is_loading: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inspector_links.relation_signature =
            storage::workspace::links::workspace_relation_signature(&content);
        self.title = model.title.into();
        self.inspector_links.project_id = model.project_id;
        self.persistence.current_path = model.file_path.map(PathBuf::from);
        let current_path = self.persistence.current_path.clone();
        self.apply_document_kind(current_path.as_deref(), cx);
        self.inspector_links
            .completion_provider
            .update_reference_catalog(
                self.note_id as i64,
                self.inspector_links.project_id,
                self.kind == DocumentKind::Markdown,
                self.inspector_links.workspace_catalog.clone(),
            );
        self.persistence.file_managed_by_app = model.file_managed_by_app;
        self.persistence.auto_save_epoch = self.persistence.auto_save_epoch.saturating_add(1);
        self.persistence.is_loading = is_loading;
        self.persistence.load_error = None;

        self.persistence.save_state = if missing {
            SaveState::Missing
        } else {
            SaveState::Saved
        };

        self.analysis.stats = DocumentStats::from_text("");
        self.analysis.outline = DocumentOutline::None;
        self.analysis.preview_sections = Arc::new(Vec::new());
        self.analysis.outline_selected = None;
        self.rebuild_outline_rows();

        self.persistence.suppress_editor_events = true;
        let pending_navigation_offset = self.pending_navigation_offset.take();
        self.editor.update(cx, |editor, cx| {
            editor.set_value(content.as_str(), window, cx);
            if let Some(offset) = pending_navigation_offset {
                let offset = offset.min(editor.text().len());
                let position = editor.text().offset_to_position(offset);
                editor.set_cursor_position(position, window, cx);
            }
        });
        self.persistence.suppress_editor_events = false;
        self.reset_vim_command();
        self.focus_source_mode(window, cx);
        self.schedule_document_analysis(false, cx);
        self.refresh_board_embeds(cx);

        cx.notify();
    }

    pub(super) fn fail_load(&mut self, message: String, cx: &mut Context<Self>) {
        let message = SharedString::from(message);
        self.persistence.is_loading = false;
        self.persistence.load_error = Some(message.clone());
        self.persistence.save_state = SaveState::Error(message);
        cx.notify();
    }

    pub(super) fn mark_file_missing(&mut self, cx: &mut Context<Self>) {
        self.persistence.is_loading = false;
        self.persistence.load_error = None;
        self.persistence.save_state = SaveState::Missing;
        cx.notify();
    }

    pub(super) fn update_from_editor(&mut self, cx: &mut Context<Self>) {
        if self.persistence.is_loading {
            return;
        }

        self.analysis.outline_source_highlight = None;
        self.analysis.outline_source_highlight_task = None;
        let old_save_state = self.persistence.save_state.clone();
        if !matches!(self.persistence.save_state, SaveState::Missing) {
            self.persistence.save_state = SaveState::Dirty;
        }

        if self.persistence.save_state != old_save_state {
            cx.notify();
        }

        self.schedule_document_analysis(true, cx);
        self.schedule_board_embed_refresh(cx);
        self.schedule_auto_save(cx);
    }

    pub(super) fn schedule_auto_save(&mut self, cx: &mut Context<Self>) {
        self.persistence.auto_save_epoch = self.persistence.auto_save_epoch.saturating_add(1);
        let epoch = self.persistence.auto_save_epoch;
        let app_runtime = cx.global::<AppRuntime>().clone();

        self.persistence.auto_save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTO_SAVE_IDLE_DELAY).await;

            let save_request = this
                .update(cx, |this, cx| {
                    if this.persistence.auto_save_epoch != epoch {
                        return None;
                    }

                    let note_id = this.note_id;
                    let path = this.persistence.current_path.clone();
                    let is_missing = matches!(this.persistence.save_state, SaveState::Missing);
                    let content = this.editor.read(cx).value();

                    if path.is_some() && !is_missing {
                        this.persistence.save_state = SaveState::Saving;
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
                .read_with(cx, |_, cx| cx.global::<AppRuntime>().store())
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
                    Ok(()) => {
                        let persisted_content = content.clone();
                        let save_db = db.clone();
                        match app_runtime
                            .spawn_tokio(cx.background_executor(), async move {
                                storage::note::documents::persist_document_content(
                                    &save_db,
                                    note_id,
                                    persisted_content.to_string(),
                                    true,
                                )
                                .await
                            })
                            .await
                        {
                            Ok(Ok(_)) => Ok(()),
                            Ok(Err(err)) => Err(err.to_string()),
                            Err(err) => Err(format!("Failed to join autosave task: {err}")),
                        }
                    }
                    Err(err) => Err(err),
                }
            } else {
                let persisted_content = content.clone();
                match app_runtime
                    .spawn_tokio(cx.background_executor(), async move {
                        storage::note::documents::persist_document_content(
                            &db,
                            note_id,
                            persisted_content.to_string(),
                            false,
                        )
                        .await
                    })
                    .await
                {
                    Ok(Ok(_)) => Ok(()),
                    Ok(Err(err)) => Err(err.to_string()),
                    Err(err) => Err(format!("Failed to join autosave task: {err}")),
                }
            };

            match result {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        let workspace_relation_signature =
                            storage::workspace::links::workspace_relation_signature(&content);
                        let workspace_links_changed =
                            this.inspector_links.relation_signature != workspace_relation_signature;
                        this.inspector_links.relation_signature = workspace_relation_signature;
                        this.persistence.save_state = this.resolve_save_state(&content, cx);
                        if this.persistence.save_state == SaveState::Saved {
                            this.refresh_note_links_with_runtime(app_runtime.clone(), cx);
                            cx.emit(DocumentEditorEvent::Saved(this.note_id));
                            if workspace_links_changed {
                                cx.emit(DocumentEditorEvent::WorkspaceLinksChanged);
                            }
                        }
                    })
                    .ok();
                }
                Err(err) => {
                    this.update(cx, |this, _cx| {
                        this.persistence.save_state = SaveState::Error(err.into());
                    })
                    .ok();
                }
            }
        }));
    }

    pub fn save(&mut self, cx: &mut Context<Self>) {
        let (path, file_managed_by_app) = self
            .persistence
            .current_path
            .clone()
            .map(|path| (path, self.persistence.file_managed_by_app))
            .unwrap_or_else(|| {
                (
                    unique_note_path(
                        cx.global::<AppRuntime>().data_dir().join("notes"),
                        self.title.as_ref(),
                    ),
                    true,
                )
            });
        self.save_to_path(path, file_managed_by_app, cx);
    }

    pub fn save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let file_name = suggested_save_as_file_name(
            self.persistence.current_path.as_deref(),
            self.title.as_ref(),
        );
        self.prompt_save_as(file_name, window, cx);
    }

    pub fn change_document_kind(
        &mut self,
        kind: DocumentKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind == kind || self.persistence.is_loading {
            return;
        }

        if let Some(current_path) = self.persistence.current_path.clone()
            && self.persistence.file_managed_by_app
        {
            let target_path = current_path.with_extension(kind.extension());
            if target_path.exists() {
                window.push_notification(
                    Notification::error(format!(
                        "Cannot convert this document because {} already exists.",
                        target_path.display()
                    )),
                    cx,
                );
                return;
            }

            self.save_to_path_replacing(target_path, true, Some(current_path), cx);
            return;
        }

        let file_name = suggested_save_as_file_name_with_extension(
            self.persistence.current_path.as_deref(),
            self.title.as_ref(),
            kind.extension(),
        );
        self.prompt_save_as(file_name, window, cx);
    }

    fn prompt_save_as(&mut self, file_name: String, window: &mut Window, cx: &mut Context<Self>) {
        let start_dir = self
            .persistence
            .current_path
            .as_ref()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(|| cx.global::<AppRuntime>().data_dir().join("notes"));

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
        self.save_to_path_replacing(path, file_managed_by_app, None, cx);
    }

    fn save_to_path_replacing(
        &mut self,
        path: PathBuf,
        file_managed_by_app: bool,
        replaced_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.persistence.auto_save_epoch = self.persistence.auto_save_epoch.saturating_add(1);
        self.persistence.save_state = SaveState::Saving;

        let content = self.editor.read(cx).value();
        let note_id = self.note_id;
        let app_runtime = cx.global::<AppRuntime>().clone();
        let db = app_runtime.store();
        let saved_path = path.clone();
        let path_string = path.display().to_string();

        cx.notify();

        cx.spawn(async move |this, cx| {
            let content_to_write = content.to_string();
            let write_path = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    if let Some(parent) = write_path.parent() {
                        create_dir_all(parent).map_err(|err| err.to_string())?;
                    }
                    write(write_path, content_to_write).map_err(|err| err.to_string())
                })
                .await;

            let result = match result {
                Ok(()) => {
                    let persisted_content = content.clone();
                    match app_runtime
                        .spawn_tokio(cx.background_executor(), async move {
                            storage::note::documents::persist_document_to_path(
                                &db,
                                note_id,
                                path_string,
                                file_managed_by_app,
                                persisted_content.to_string(),
                            )
                            .await
                        })
                        .await
                    {
                        Ok(Ok(_)) => Ok(()),
                        Ok(Err(err)) => Err(err.to_string()),
                        Err(err) => Err(format!("Failed to join note save task: {err}")),
                    }
                }
                Err(err) => Err(err),
            };

            match result {
                Ok(_) => {
                    if let Some(replaced_path) = replaced_path {
                        let replaced_path_display = replaced_path.display().to_string();
                        if let Err(err) = cx
                            .background_executor()
                            .spawn(async move { remove_file(replaced_path) })
                            .await
                        {
                            eprintln!(
                                "Failed to remove replaced document file \
                                 {replaced_path_display}: {err}"
                            );
                        }
                    }

                    this.update(cx, |this, cx| {
                        let path_changed =
                            this.persistence.current_path.as_ref() != Some(&saved_path);
                        this.persistence.current_path = Some(saved_path);
                        this.persistence.file_managed_by_app = file_managed_by_app;
                        this.persistence.is_loading = false;
                        let workspace_relation_signature =
                            storage::workspace::links::workspace_relation_signature(&content);
                        let workspace_links_changed =
                            this.inspector_links.relation_signature != workspace_relation_signature;
                        this.inspector_links.relation_signature = workspace_relation_signature;
                        this.persistence.save_state = this.resolve_save_state(&content, cx);
                        if this.persistence.save_state == SaveState::Saved {
                            this.refresh_note_links_with_runtime(app_runtime.clone(), cx);
                            cx.emit(DocumentEditorEvent::Saved(this.note_id));
                            if workspace_links_changed {
                                cx.emit(DocumentEditorEvent::WorkspaceLinksChanged);
                            }
                        }
                        let path = this.persistence.current_path.clone();
                        this.apply_document_kind(path.as_deref(), cx);
                        this.schedule_document_analysis(false, cx);
                        if path_changed {
                            cx.emit(DocumentEditorEvent::PathChanged);
                        }
                    })
                    .ok();
                }
                Err(err) => {
                    this.update(cx, |this, _cx| {
                        this.persistence.save_state = SaveState::Error(err.into());
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
