use gpui::{
    App, AppContext as _, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    SharedString, Styled, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme, Disableable as _, IconName, Selectable as _, Sizable as _, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{DialogClose, DialogFooter},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    v_flex,
};
use storage::board::templates::{BoardTemplate, BoardTemplateId};

use super::AppShell;
use crate::AppServices;

const TEMPLATE_DIALOG_WIDTH: f32 = 760.;
const TEMPLATE_DIALOG_HEIGHT: f32 = 620.;
const TEMPLATE_DIALOG_MARGIN: f32 = 32.;

pub(super) struct BoardTemplatePickerState {
    project_id: Option<u32>,
    title_input: Entity<InputState>,
    templates: Vec<BoardTemplate>,
    selected_key: String,
    loading_custom: bool,
    creating: bool,
    confirm_delete_template_id: Option<i64>,
    deleting_template_id: Option<i64>,
    error: Option<SharedString>,
}

impl AppShell {
    pub(super) fn open_board_template_picker(
        &mut self,
        project_id: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.board_template_dialog_open || window.has_active_dialog(cx) {
            return;
        }

        let templates = storage::board::templates::built_in_templates();
        let Some(selected_key) = templates
            .iter()
            .find(|template| template.id == BoardTemplateId::BuiltIn("kanban"))
            .or_else(|| templates.first())
            .map(|template| template.id.key())
        else {
            return;
        };
        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Board name")
                .default_value("Board")
        });

        cx.subscribe_in(
            &title_input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.create_selected_board_template(window, cx);
                }
            },
        )
        .detach();

        self.board_template_dialog_open = true;
        self.board_template_picker = Some(BoardTemplatePickerState {
            project_id,
            title_input: title_input.clone(),
            templates,
            selected_key,
            loading_custom: true,
            creating: false,
            confirm_delete_template_id: None,
            deleting_template_id: None,
            error: None,
        });

        let app = cx.entity();
        let close_owner = app.clone();
        window.open_dialog(cx, move |dialog, window, _| {
            let width = TEMPLATE_DIALOG_WIDTH
                .min((window.viewport_size().width.as_f32() - TEMPLATE_DIALOG_MARGIN).max(320.));
            let height = TEMPLATE_DIALOG_HEIGHT
                .min((window.viewport_size().height.as_f32() - TEMPLATE_DIALOG_MARGIN).max(320.));

            dialog
                .w(px(width))
                .h(px(height))
                .title("Create a board")
                .on_close({
                    let close_owner = close_owner.clone();
                    move |_, _, cx| {
                        close_owner.update(cx, |this, cx| {
                            this.board_template_dialog_open = false;
                            this.board_template_picker = None;
                            cx.notify();
                        });
                    }
                })
                .content({
                    let app = app.clone();
                    move |content, _, cx| {
                        content
                            .p_0()
                            .child(app.read(cx).render_board_template_picker(app.clone(), cx))
                    }
                })
        });

        title_input.update(cx, |input, cx| input.focus(window, cx));
        self.load_custom_board_templates(cx);
        cx.notify();
    }

    fn load_custom_board_templates(&mut self, cx: &mut Context<Self>) {
        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move { storage::board::templates::load_custom_templates(&db).await })
                .await;
            this.update(cx, |this, cx| {
                let Some(picker) = this.board_template_picker.as_mut() else {
                    return;
                };
                picker.loading_custom = false;
                match result {
                    Ok(Ok(custom_templates)) => picker.templates.extend(custom_templates),
                    Ok(Err(error)) => {
                        picker.error =
                            Some(format!("Could not load your templates: {error}").into())
                    }
                    Err(error) => {
                        picker.error =
                            Some(format!("Could not finish loading templates: {error}").into())
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_board_template_picker(&self, app: Entity<Self>, cx: &App) -> impl IntoElement {
        let Some(picker) = self.board_template_picker.as_ref() else {
            return div().into_any_element();
        };
        let project_name = picker.project_id.and_then(|project_id| {
            self.workspace
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .map(|project| project.name.clone())
        });
        let destination = project_name
            .map(|name| format!("Created in {name}"))
            .unwrap_or_else(|| "Created in the workspace".to_string());
        let templates = picker.templates.clone();
        let selected_key = picker.selected_key.clone();
        let confirm_delete_template_id = picker.confirm_delete_template_id;
        let deleting_template_id = picker.deleting_template_id;
        let creating = picker.creating;

        v_flex()
            .size_full()
            .overflow_hidden()
            .child(
                v_flex()
                    .gap_2()
                    .px_5()
                    .pt_4()
                    .pb_3()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.72))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Board name"),
                    )
                    .child(Input::new(&picker.title_input))
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(destination),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_2()
                    .px_5()
                    .py_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                h_flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .child("Start from a template"),
                                    )
                                    .when(picker.loading_custom, |this| {
                                        this.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Loading your templates…"),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("The template is copied into a fully editable board."),
                            ),
                    )
                    .child(
                        v_flex()
                            .id("board-template-list")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .gap_2()
                            .children(templates.into_iter().map(|template| {
                                let key = template.id.key();
                                let selected = key == selected_key;
                                let select_app = app.clone();
                                let delete_app = app.clone();
                                let cancel_delete_app = app.clone();
                                let confirm_delete_app = app.clone();
                                let custom_id = match template.id {
                                    BoardTemplateId::Custom(id) => Some(id),
                                    BoardTemplateId::BuiltIn(_) => None,
                                };
                                let confirming_delete = custom_id
                                    .is_some_and(|id| confirm_delete_template_id == Some(id));
                                let column_preview = template
                                    .definition
                                    .columns
                                    .iter()
                                    .map(|column| column.title.as_str())
                                    .collect::<Vec<_>>()
                                    .join("  →  ");
                                let counts = template.summary();

                                h_flex()
                                    .id(SharedString::from(format!("board-template-row-{key}")))
                                    .w_full()
                                    .min_w_0()
                                    .items_center()
                                    .gap_3()
                                    .px_3()
                                    .py_3()
                                    .rounded(cx.theme().radius)
                                    .border_1()
                                    .border_color(if selected {
                                        cx.theme().primary.opacity(0.75)
                                    } else {
                                        cx.theme().border.opacity(0.72)
                                    })
                                    .bg(if selected {
                                        cx.theme().primary.opacity(0.08)
                                    } else {
                                        cx.theme().background
                                    })
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .gap_1()
                                            .child(
                                                h_flex()
                                                    .gap_2()
                                                    .items_center()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                                            .child(template.name),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .text_color(cx.theme().muted_foreground)
                                                            .child(if custom_id.is_some() {
                                                                "Custom"
                                                            } else {
                                                                "Built-in"
                                                            }),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_sm()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(template.description),
                                            )
                                            .when(!column_preview.is_empty(), |this| {
                                                this.child(
                                                    div()
                                                        .truncate()
                                                        .text_xs()
                                                        .text_color(
                                                            cx.theme()
                                                                .muted_foreground
                                                                .opacity(0.82),
                                                        )
                                                        .child(column_preview),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(counts),
                                            ),
                                    )
                                    .when_some(custom_id.filter(|_| !confirming_delete), |this, template_id| {
                                        this.child(
                                            Button::new(SharedString::from(format!(
                                                "delete-board-template-{template_id}"
                                            )))
                                            .icon(IconName::Delete)
                                            .ghost()
                                            .small()
                                            .disabled(
                                                creating
                                                    || deleting_template_id == Some(template_id),
                                            )
                                            .tooltip("Delete template")
                                            .on_click(move |_, _, cx| {
                                                delete_app.update(cx, |this, cx| {
                                                    if let Some(picker) = this.board_template_picker.as_mut() {
                                                        picker.confirm_delete_template_id = Some(template_id);
                                                        cx.notify();
                                                    }
                                                });
                                            }),
                                        )
                                    })
                                    .when_some(custom_id.filter(|_| confirming_delete), |this, template_id| {
                                        this.child(
                                            h_flex()
                                                .gap_1()
                                                .items_center()
                                                .child(
                                                    Button::new(SharedString::from(format!(
                                                        "cancel-delete-board-template-{template_id}"
                                                    )))
                                                    .label("Cancel")
                                                    .ghost()
                                                    .small()
                                                    .disabled(deleting_template_id == Some(template_id))
                                                    .on_click(move |_, _, cx| {
                                                        cancel_delete_app.update(cx, |this, cx| {
                                                            if let Some(picker) = this.board_template_picker.as_mut() {
                                                                picker.confirm_delete_template_id = None;
                                                                cx.notify();
                                                            }
                                                        });
                                                    }),
                                                )
                                                .child(
                                                    Button::new(SharedString::from(format!(
                                                        "confirm-delete-board-template-{template_id}"
                                                    )))
                                                    .icon(IconName::Delete)
                                                    .label(if deleting_template_id == Some(template_id) {
                                                        "Deleting…"
                                                    } else {
                                                        "Delete"
                                                    })
                                                    .danger()
                                                    .small()
                                                    .disabled(deleting_template_id == Some(template_id))
                                                    .on_click(move |_, _, cx| {
                                                        confirm_delete_app.update(cx, |this, cx| {
                                                            this.delete_board_template(template_id, cx);
                                                        });
                                                    }),
                                                ),
                                        )
                                    })
                                    .when(!confirming_delete, |this| {
                                        this.child(
                                            Button::new(SharedString::from(format!(
                                                "select-board-template-{key}"
                                            )))
                                            .label(if selected { "Selected" } else { "Select" })
                                            .outline()
                                            .small()
                                            .selected(selected)
                                            .disabled(creating)
                                            .on_click(move |_, _, cx| {
                                                select_app.update(cx, |this, cx| {
                                                    if let Some(picker) = this.board_template_picker.as_mut() {
                                                        picker.selected_key = key.clone();
                                                        picker.confirm_delete_template_id = None;
                                                        picker.error = None;
                                                        cx.notify();
                                                    }
                                                });
                                            }),
                                        )
                                    })
                            })),
                    )
                    .when_some(picker.error.clone(), |this, error| {
                        this.child(div().text_sm().text_color(cx.theme().danger).child(error))
                    }),
            )
            .child(
                DialogFooter::new()
                    .px_5()
                    .py_3()
                    .border_t_1()
                    .border_color(cx.theme().border.opacity(0.72))
                    .justify_between()
                    .child(
                        DialogClose::new().child(
                            Button::new("cancel-board-template")
                                .label("Cancel")
                                .outline()
                                .disabled(creating),
                        ),
                    )
                    .child(
                        Button::new("create-board-from-template")
                            .icon(IconName::Plus)
                            .label(if creating {
                                "Creating…"
                            } else {
                                "Create board"
                            })
                            .primary()
                            .disabled(creating)
                            .on_click(move |_, window, cx| {
                                app.update(cx, |this, cx| {
                                    this.create_selected_board_template(window, cx);
                                });
                            }),
                    ),
            )
            .into_any_element()
    }

    fn create_selected_board_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.board_template_picker.as_mut() else {
            return;
        };
        if picker.creating {
            return;
        }
        let title = picker
            .title_input
            .read(cx)
            .text()
            .to_string()
            .trim()
            .to_string();
        if title.is_empty() {
            picker.error = Some("Enter a board name.".into());
            picker
                .title_input
                .update(cx, |input, cx| input.focus(window, cx));
            cx.notify();
            return;
        }
        let Some(template) = picker
            .templates
            .iter()
            .find(|template| template.id.key() == picker.selected_key)
            .cloned()
        else {
            picker.error = Some("Select a template.".into());
            cx.notify();
            return;
        };

        picker.creating = true;
        picker.error = None;
        let project_id = picker.project_id;
        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        let app = cx.entity().downgrade();
        cx.notify();

        cx.spawn_in(window, async move |_, window| {
            let result = runtime
                .spawn(async move {
                    storage::board::templates::create_board_from_template(
                        &db,
                        project_id,
                        title,
                        template.definition,
                    )
                    .await
                })
                .await;

            window
                .update(|window, cx| match result {
                    Ok(Ok(inserted)) => {
                        if let Some(app) = app.upgrade() {
                            app.update(cx, |this, cx| {
                                this.board_template_dialog_open = false;
                                this.board_template_picker = None;
                                this.open_board_tab(
                                    inserted.id,
                                    project_id,
                                    SharedString::from(inserted.title),
                                    window,
                                    cx,
                                );
                                this.refresh_workspace(cx);
                            });
                        }
                        window.close_dialog(cx);
                    }
                    Ok(Err(error)) => {
                        if let Some(app) = app.upgrade() {
                            app.update(cx, |this, cx| {
                                if let Some(picker) = this.board_template_picker.as_mut() {
                                    picker.creating = false;
                                    picker.error =
                                        Some(format!("Could not create the board: {error}").into());
                                    cx.notify();
                                }
                            });
                        }
                    }
                    Err(error) => {
                        if let Some(app) = app.upgrade() {
                            app.update(cx, |this, cx| {
                                if let Some(picker) = this.board_template_picker.as_mut() {
                                    picker.creating = false;
                                    picker.error = Some(
                                        format!("Could not finish creating the board: {error}")
                                            .into(),
                                    );
                                    cx.notify();
                                }
                            });
                        }
                    }
                })
                .ok();
        })
        .detach();
    }

    fn delete_board_template(&mut self, template_id: i64, cx: &mut Context<Self>) {
        let Some(picker) = self.board_template_picker.as_mut() else {
            return;
        };
        if picker.deleting_template_id.is_some() || picker.creating {
            return;
        }
        picker.deleting_template_id = Some(template_id);
        picker.error = None;
        let db = cx.global::<AppServices>().store();
        let runtime = cx.global::<AppServices>().runtime();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::board::templates::delete_custom_template(&db, template_id).await
                })
                .await;
            this.update(cx, |this, cx| {
                let Some(picker) = this.board_template_picker.as_mut() else {
                    return;
                };
                picker.deleting_template_id = None;
                picker.confirm_delete_template_id = None;
                match result {
                    Ok(Ok(())) => {
                        let deleted_key = BoardTemplateId::Custom(template_id).key();
                        picker
                            .templates
                            .retain(|template| template.id.key() != deleted_key);
                        if picker.selected_key == deleted_key
                            && let Some(template) = picker.templates.first()
                        {
                            picker.selected_key = template.id.key();
                        }
                    }
                    Ok(Err(error)) => {
                        picker.error =
                            Some(format!("Could not delete the template: {error}").into())
                    }
                    Err(error) => {
                        picker.error =
                            Some(format!("Could not finish deleting the template: {error}").into())
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
