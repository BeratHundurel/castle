use std::sync::Arc;

use app_services::AppServices;
use gpui::{Context, Task};

pub use workspace_ui::{
    WikiLinkCompletionProvider, WikiLinkPreviewPlugin, workspace_navigation_target,
};

use super::{DocumentEditorView, DocumentInspectorTab};

impl DocumentEditorView {
    pub(super) fn load_note_links_async(
        note_id: u32,
        generation: u64,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let runtime = cx.global::<AppServices>().runtime();
        Self::load_note_links_with_runtime(note_id, generation, runtime, cx)
    }

    fn load_note_links_with_runtime(
        note_id: u32,
        generation: u64,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let db = cx.global::<AppServices>().store();
        cx.spawn(async move |this, cx| {
            let (cancel_on_drop, cancelled) = tokio::sync::oneshot::channel::<()>();
            let load = runtime.spawn(async move {
                tokio::select! {
                    biased;
                    _ = cancelled => None,
                    result = async move {
                        let links = storage::note_links::load_note_links(&db, note_id as i64).await?;
                        let note_catalog = storage::note_links::load_note_link_catalog(&db);
                        let workspace_links = storage::workspace_links::load_note_workspace_links(
                            &db,
                            note_id as i64,
                        );
                        let workspace_catalog =
                            storage::workspace_links::load_workspace_link_catalog(&db);
                        let (note_catalog, workspace_links, workspace_catalog) =
                            tokio::try_join!(note_catalog, workspace_links, workspace_catalog)?;
                        Ok::<_, anyhow::Error>((
                            links,
                            note_catalog,
                            workspace_links,
                            workspace_catalog,
                        ))
                    } => Some(result),
                }
            });
            let result = load.await;
            drop(cancel_on_drop);

            this.update(cx, |this, cx| {
                if this.note_id != note_id
                    || this.inspector_links.request.generation() != generation
                {
                    return;
                }
                this.inspector_links.loading = false;
                match result {
                    Ok(Some(Ok((links, note_catalog, workspace_links, workspace_catalog)))) => {
                        this.inspector_links.note_links = Arc::new(links);
                        this.inspector_links.note_catalog = Arc::new(note_catalog);
                        this.inspector_links.workspace_links = Arc::new(workspace_links);
                        this.inspector_links.workspace_catalog = Arc::new(workspace_catalog);
                        this.inspector_links.completion_provider.update(
                            this.note_id as i64,
                            this.inspector_links.project_id,
                            this.kind == super::DocumentKind::Markdown,
                            this.inspector_links.note_catalog.clone(),
                            this.inspector_links.workspace_catalog.clone(),
                        );
                        this.inspector_links.error = None;
                    }
                    Ok(Some(Err(error))) => {
                        this.inspector_links.error = Some(error.to_string().into())
                    }
                    Ok(None) => return,
                    Err(error) => {
                        this.inspector_links.error =
                            Some(format!("Link task failed: {error}").into())
                    }
                }
                cx.notify();
            })
            .ok();
        })
    }

    pub fn refresh_note_links(&mut self, cx: &mut Context<Self>) {
        let runtime = cx.global::<AppServices>().runtime();
        self.refresh_note_links_with_runtime(runtime, cx);
    }

    pub(super) fn refresh_note_links_with_runtime(
        &mut self,
        runtime: tokio::runtime::Handle,
        cx: &mut Context<Self>,
    ) {
        let generation = self.inspector_links.request.begin();
        self.inspector_links.loading = true;
        let task = Self::load_note_links_with_runtime(self.note_id, generation, runtime, cx);
        self.inspector_links.request.set_task(task);
        cx.notify();
    }

    pub(super) fn show_outline_inspector(&mut self, cx: &mut Context<Self>) {
        self.inspector_links.tab = DocumentInspectorTab::Outline;
        cx.notify();
    }

    pub(super) fn show_links_inspector(&mut self, cx: &mut Context<Self>) {
        self.inspector_links.tab = DocumentInspectorTab::Links;
        self.refresh_note_links(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_settings::AppSettings;
    use entity::note;
    use gpui::{AppContext as _, EntityInputHandler as _};
    use gpui_component::input::{Enter, Position};
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};
    use std::{path::PathBuf, sync::Arc};

    #[gpui::test]
    fn editor_change_populates_wikilink_completion(cx: &mut gpui::TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("Tokio test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.executor().allow_parking();
        let (db, source_id, _target_id) = runtime
            .block_on(async {
                let db = Database::connect("sqlite::memory:").await?;
                Migrator::up(&db, None).await?;
                let source = note::ActiveModel {
                    title: Set("Source".into()),
                    cached_content: Set(String::new()),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                let target = note::ActiveModel {
                    title: Set("Target note".into()),
                    cached_content: Set(String::new()),
                    created_at: Set(1),
                    updated_at: Set(1),
                    ..Default::default()
                }
                .insert(&db)
                .await?;
                Ok::<_, anyhow::Error>((db, source.id as u32, target.id))
            })
            .expect("completion test database should initialize");
        let settings_dir =
            std::env::temp_dir().join(format!("castle-wikilink-completion-{}", std::process::id()));
        let mut editor_view = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(AppSettings::load(settings_dir));
            cx.set_global(AppServices::new(Arc::new(db), PathBuf::new()));
            cx.open_window(Default::default(), |window, cx| {
                let view = DocumentEditorView::view(source_id, window, cx);
                editor_view = Some(view.clone());
                cx.new(|cx| gpui_component::Root::new(view, window, cx))
            })
            .expect("completion test window should open")
        });
        let view = editor_view.expect("document editor should exist");
        let mut cx = gpui::VisualTestContext::from_window(window.into(), cx);
        for _ in 0..100 {
            cx.run_until_parked();
            if view.read_with(&cx, |editor, _| {
                !editor.persistence.is_loading && editor.inspector_links.note_catalog.len() == 2
            }) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.editor.update(cx, |input, cx| {
                    input.set_value("[[Ta]]", window, cx);
                    input.set_cursor_position(Position::new(0, 4), window, cx);
                    input.replace_text_in_range(None, "r", window, cx);
                });
            });
        });
        cx.run_until_parked();

        cx.update(|window, cx| {
            view.update(cx, |editor, cx| {
                editor.editor.update(cx, |input, cx| {
                    assert!(input.handle_action_for_context_menu(
                        Box::new(Enter {
                            secondary: false,
                            shift: false,
                        }),
                        window,
                        cx,
                    ));
                });
            });
        });
        cx.run_until_parked();
        view.read_with(&cx, |editor, cx| {
            assert_eq!(editor.editor.read(cx).value(), "[[Target note]]");
        });
    }
}
