use gpui::{
    AppContext as _, Context, ParentElement as _, SharedString, Styled as _, Window,
    prelude::FluentBuilder as _,
};
use gpui_component::{
    IndexPath, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{
        DialogAction, DialogClose, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
    },
    input::{Input, InputState},
    notification::Notification,
    searchable_list::{SearchableListItem, SearchableVec},
    select::{Select, SelectState},
    v_flex,
};

use super::AppShell;
use crate::AppServices;

#[derive(Clone, Debug, Eq, PartialEq)]
struct DestinationOption {
    value: SharedString,
    label: SharedString,
}

impl SearchableListItem for DestinationOption {
    type Value = SharedString;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }

    fn matches(&self, query: &str) -> bool {
        self.label.to_lowercase().contains(&query.to_lowercase())
    }
}

impl AppShell {
    pub(super) fn open_create_card_from_selection_picker(
        &mut self,
        note_id: u32,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        let app = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            let result =
                runtime
                    .spawn(async move {
                        storage::workspace::links::load_workspace_link_catalog(&db).await
                    })
                    .await;
            window
                .update(|window, cx| match result {
                    Ok(Ok(catalog)) => app.update(cx, |this, cx| {
                        this.show_create_card_from_selection_picker(
                            note_id, title, catalog, window, cx,
                        );
                    }),
                    Ok(Err(error)) => window.push_notification(
                        Notification::error(format!("Could not load board destinations: {error}")),
                        cx,
                    ),
                    Err(error) => window.push_notification(
                        Notification::error(format!(
                            "Could not finish loading destinations: {error}"
                        )),
                        cx,
                    ),
                })
                .ok();
        })
        .detach();
    }

    fn show_create_card_from_selection_picker(
        &mut self,
        note_id: u32,
        title: String,
        catalog: Vec<storage::workspace::links::WorkspaceCatalogEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let note_project_id = catalog
            .iter()
            .find(|entry| {
                entry.item.kind == storage::workspace::links::WorkspaceItemKind::Note
                    && entry.item.id == i64::from(note_id)
            })
            .and_then(|entry| entry.project_id);

        let mut lists = catalog
            .iter()
            .filter(|entry| entry.item.kind == storage::workspace::links::WorkspaceItemKind::List)
            .cloned()
            .collect::<Vec<_>>();

        lists.sort_by_key(|entry| {
            (
                entry.project_id != note_project_id,
                entry.breadcrumb().to_lowercase(),
                entry.item.id,
            )
        });
        if lists.is_empty() {
            window.push_notification(Notification::warning("Create a board list first."), cx);
            return;
        }

        let options = lists
            .iter()
            .map(|entry| DestinationOption {
                value: entry.item.id.to_string().into(),
                label: entry.breadcrumb().into(),
            })
            .collect::<Vec<_>>();

        let selected_index = self
            .last_card_destination
            .and_then(|last| lists.iter().position(|entry| entry.item.id == last))
            .unwrap_or_default();

        let select = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(options),
                Some(IndexPath::default().row(selected_index)),
                window,
                cx,
            )
            .searchable(true)
        });

        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Card title")
                .default_value(title)
        });

        let dialog_select = select.clone();
        let dialog_title = title_input.clone();
        let app = cx.entity();

        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .w(gpui::px(560.))
                .on_ok({
                    let app = app.clone();
                    let select = dialog_select.clone();
                    let title_input = dialog_title.clone();
                    move |_, window, cx| {
                        let Some(list_id) = select
                            .read(cx)
                            .selected_value()
                            .and_then(|value| value.parse::<i64>().ok())
                        else {
                            window.push_notification(
                                Notification::error("Choose a board list."),
                                cx,
                            );
                            return false;
                        };
                        let title = title_input.read(cx).value().trim().to_string();
                        if title.is_empty() {
                            window.push_notification(
                                Notification::error("Enter a card title."),
                                cx,
                            );
                            return false;
                        }
                        let db = cx.global::<AppServices>().store();
                        let runtime = cx.global::<AppServices>().runtime();
                        let app_for_result = app.clone();
                        app.update(cx, |this, cx| {
                            this.last_card_destination = Some(list_id);
                            cx.spawn_in(window, async move |_, window| {
                                let result = runtime
                                    .spawn(async move {
                                        storage::workspace::links::create_card_from_note_selection(
                                            &db,
                                            i64::from(note_id),
                                            list_id,
                                            title,
                                            crate::now_ts(),
                                        )
                                        .await
                                    })
                                    .await;
                                window
                                    .update(|window, cx| match result {
                                        Ok(Ok(created)) => {
                                            let target = u32::try_from(created.board_id)
                                                .ok()
                                                .zip(u32::try_from(created.entry_id).ok())
                                                .map(|(board_id, entry_id)| {
                                                    crate::workspace_navigation::WorkspaceNavigationTarget::card(
                                                        board_id, entry_id,
                                                    )
                                                });
                                            app_for_result.update(cx, |this, cx| {
                                                this.refresh_workspace(cx);
                                            });
                                            let notification = Notification::success("Card created")
                                                .when_some(target, |notification, target| {
                                                    let app = app_for_result.clone();
                                                    notification.action(move |_, _, cx| {
                                                        let app = app.clone();
                                                        Button::new("open-created-card")
                                                            .label("Open card")
                                                            .primary()
                                                            .on_click(cx.listener(move |notification, _, window, cx| {
                                                                app.update(cx, |this, cx| {
                                                                    this.open_workspace_target(target, window, cx);
                                                                });
                                                                notification.dismiss(window, cx);
                                                            }))
                                                    })
                                                });
                                            window.push_notification(notification, cx);
                                        }
                                        Ok(Err(error)) => window.push_notification(
                                            Notification::error(format!("Could not create card: {error}")),
                                            cx,
                                        ),
                                        Err(error) => window.push_notification(
                                            Notification::error(format!("Could not finish creating card: {error}")),
                                            cx,
                                        ),
                                    })
                                    .ok();
                            })
                            .detach();
                        });
                        true
                    }
                })
                .child(
                    DialogHeader::new()
                        .child(DialogTitle::new().child("Create card from note"))
                        .child(DialogDescription::new().child(
                            "The note stays unchanged and is linked to the new card.",
                        )),
                )
                .child(
                    v_flex()
                        .gap_3()
                        .py_3()
                        .child(Input::new(&dialog_title))
                        .child(
                            Select::new(&dialog_select)
                                .search_placeholder("Search boards and lists")
                                .w_full(),
                        ),
                )
                .child(
                    DialogFooter::new()
                        .child(
                            DialogClose::new().child(
                                Button::new("cancel-create-card-from-note")
                                    .label("Cancel")
                                    .outline(),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("confirm-create-card-from-note")
                                    .label("Create card")
                                    .primary(),
                            ),
                        ),
                )
        });
        title_input.update(cx, |input, cx| input.focus(window, cx));
    }

    pub(super) fn open_insert_board_view_picker(
        &mut self,
        note_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        let app = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            let result = runtime
                .spawn(async move {
                    let catalog =
                        storage::workspace::links::load_workspace_link_catalog(&db).await?;
                    let boards = catalog
                        .into_iter()
                        .filter(|entry| {
                            entry.item.kind == storage::workspace::links::WorkspaceItemKind::Board
                        })
                        .collect::<Vec<_>>();
                    let mut choices = Vec::new();
                    for board in boards {
                        choices.push((
                            board.item.id,
                            None,
                            board.title.clone(),
                            "All cards".to_string(),
                        ));
                        let views =
                            storage::board::properties::load_board_views(&db, board.item.id)
                                .await?;
                        choices.extend(views.views.into_iter().map(|view| {
                            (board.item.id, Some(view.id), board.title.clone(), view.name)
                        }));
                    }
                    Ok::<_, anyhow::Error>(choices)
                })
                .await;
            window
                .update(|window, cx| match result {
                    Ok(Ok(choices)) => app.update(cx, |this, cx| {
                        this.show_insert_board_view_picker(note_id, choices, window, cx);
                    }),
                    Ok(Err(error)) => window.push_notification(
                        Notification::error(format!("Could not load board views: {error}")),
                        cx,
                    ),
                    Err(error) => window.push_notification(
                        Notification::error(format!(
                            "Could not finish loading board views: {error}"
                        )),
                        cx,
                    ),
                })
                .ok();
        })
        .detach();
    }

    fn show_insert_board_view_picker(
        &mut self,
        note_id: u32,
        choices: Vec<(i64, Option<i64>, String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if choices.is_empty() {
            window.push_notification(Notification::warning("Create a board first."), cx);
            return;
        }
        let options = choices
            .iter()
            .map(
                |(board_id, view_id, board_title, view_title)| DestinationOption {
                    value: format!(
                        "{board_id}:{}",
                        view_id.map(|id| id.to_string()).unwrap_or_default()
                    )
                    .into(),
                    label: format!("{board_title} · {view_title}").into(),
                },
            )
            .collect::<Vec<_>>();
        let select = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(options),
                Some(IndexPath::default().row(0)),
                window,
                cx,
            )
            .searchable(true)
        });
        let dialog_select = select.clone();
        let app = cx.entity();
        let dialog_choices = choices.clone();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .w(gpui::px(560.))
                .on_ok({
                    let app = app.clone();
                    let select = dialog_select.clone();
                    let choices = dialog_choices.clone();
                    move |_, window, cx| {
                        let Some(value) = select.read(cx).selected_value().cloned() else {
                            return false;
                        };
                        let Some((board, view)) = value.split_once(':') else {
                            return false;
                        };
                        let Some(board_id) = board.parse::<i64>().ok() else {
                            return false;
                        };
                        let view_id = (!view.is_empty()).then(|| view.parse::<i64>().ok()).flatten();
                        let Some((_, _, board_title, view_title)) = choices.iter().find(
                            |(candidate_board, candidate_view, _, _)| {
                                *candidate_board == board_id && *candidate_view == view_id
                            },
                        ) else {
                            return false;
                        };
                        let title = format!("{board_title} · {view_title}").replace('"', "\\\"");
                        let mut block = format!(
                            "```castle-board-view\nboard = {board_id}\n"
                        );
                        if let Some(view_id) = view_id {
                            block.push_str(&format!("view = {view_id}\n"));
                        }
                        block.push_str(&format!("title = \"{title}\"\n```"));
                        app.update(cx, |this, cx| {
                            if let Some(editor) = this.tabs.note_views.get(&note_id) {
                                editor.update(cx, |editor, cx| {
                                    editor.insert_text_at_selection(&block, window, cx);
                                });
                            }
                        });
                        true
                    }
                })
                .child(
                    DialogHeader::new()
                        .child(DialogTitle::new().child("Insert board view"))
                        .child(DialogDescription::new().child(
                            "Insert a read-only live projection. The board remains the editable source.",
                        )),
                )
                .child(
                    v_flex().py_3().child(
                        Select::new(&dialog_select)
                            .search_placeholder("Search boards and saved views")
                            .w_full(),
                    ),
                )
                .child(
                    DialogFooter::new()
                        .child(
                            DialogClose::new().child(
                                Button::new("cancel-insert-board-view")
                                    .label("Cancel")
                                    .outline(),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("confirm-insert-board-view")
                                    .label("Insert")
                                    .primary(),
                            ),
                        ),
                )
        });
    }
}
