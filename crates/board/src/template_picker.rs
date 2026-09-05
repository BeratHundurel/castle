use gpui::{
    App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, SharedString, Styled, Window, div, prelude::FluentBuilder as _, px,
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

use runtime::AppRuntime;

const TEMPLATE_DIALOG_WIDTH: f32 = 760.;
const TEMPLATE_DIALOG_HEIGHT: f32 = 620.;
const TEMPLATE_DIALOG_MARGIN: f32 = 32.;

struct BoardTemplatePickerState {
    project_id: Option<u32>,
    project_name: Option<SharedString>,
    title_input: Entity<InputState>,
    templates: Vec<BoardTemplate>,
    selected_key: String,
    loading_custom: bool,
    creating: bool,
    confirm_delete_template_id: Option<i64>,
    deleting_template_id: Option<i64>,
    error: Option<SharedString>,
}

pub struct BoardTemplatePicker {
    dialog_open: bool,
    state: Option<BoardTemplatePickerState>,
}

#[derive(Clone, Debug)]
pub enum BoardTemplatePickerEvent {
    BoardCreated {
        board_id: u32,
        project_id: Option<u32>,
        title: SharedString,
    },
}

impl EventEmitter<BoardTemplatePickerEvent> for BoardTemplatePicker {}

impl BoardTemplatePicker {
    pub fn view(cx: &mut App) -> Entity<Self> {
        cx.new(|_| Self {
            dialog_open: false,
            state: None,
        })
    }

    pub fn open(
        &mut self,
        project_id: Option<u32>,
        project_name: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_open || window.has_active_dialog(cx) {
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

        self.dialog_open = true;
        self.state = Some(BoardTemplatePickerState {
            project_id,
            project_name,
            title_input: title_input.clone(),
            templates,
            selected_key,
            loading_custom: true,
            creating: false,
            confirm_delete_template_id: None,
            deleting_template_id: None,
            error: None,
        });

        self.open_template_picker_dialog(window, cx);

        title_input.update(cx, |input, cx| input.focus(window, cx));
        self.load_custom_board_templates(cx);
        cx.notify();
    }

    fn open_template_picker_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let picker_view = cx.entity();
        let close_owner = picker_view.clone();
        window.open_dialog(cx, move |dialog, window, _| {
            let width = TEMPLATE_DIALOG_WIDTH
                .min((window.viewport_size().width.as_f32() - TEMPLATE_DIALOG_MARGIN).max(320.));
            let height = TEMPLATE_DIALOG_HEIGHT
                .min((window.viewport_size().height.as_f32() - TEMPLATE_DIALOG_MARGIN).max(320.));

            dialog
                .w(px(width))
                .h(px(height))
                .pb_0()
                .title("Create a board")
                .on_close({
                    let close_owner = close_owner.clone();
                    move |_, _, cx| {
                        close_owner.update(cx, |this, cx| {
                            this.dialog_open = false;
                            this.state = None;
                            cx.notify();
                        });
                    }
                })
                .content({
                    let picker_view = picker_view.clone();
                    move |content, _, cx| {
                        content.p_0().child(
                            picker_view
                                .read(cx)
                                .render_board_template_picker(picker_view.clone(), cx),
                        )
                    }
                })
        });
    }

    fn load_custom_board_templates(&mut self, cx: &mut Context<Self>) {
        let task = cx
            .global::<AppRuntime>()
            .spawn_store(cx.background_executor(), move |store| async move {
                storage::board::templates::load_custom_templates(&store).await
            });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                let Some(picker) = this.state.as_mut() else {
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

    fn render_board_template_picker(
        &self,
        picker_view: Entity<Self>,
        cx: &App,
    ) -> impl IntoElement {
        let Some(picker) = self.state.as_ref() else {
            return div().into_any_element();
        };
        let destination = picker
            .project_name
            .clone()
            .map(|name| format!("Created in {name}"))
            .unwrap_or_else(|| "Created in the workspace".to_string());
        let templates = picker.templates.clone();
        let selected_key = picker.selected_key.clone();
        let confirm_delete_template_id = picker.confirm_delete_template_id;
        let deleting_template_id = picker.deleting_template_id;
        let creating = picker.creating;

        v_flex()
            .debug_selector(|| "board-template-picker".into())
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
                    .py_4()
                    .child(
                        v_flex()
                            .gap_1()
                            .px_5()
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
                            .debug_selector(|| "board-template-list".into())
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .px_5()
                            .gap_2()
                            .children(templates.into_iter().map(|template| {
                                let key = template.id.key();
                                let selected = key == selected_key;
                                let select_picker = picker_view.clone();
                                let delete_picker = picker_view.clone();
                                let cancel_delete_picker = picker_view.clone();
                                let confirm_delete_picker = picker_view.clone();
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
                                                delete_picker.update(cx, |this, cx| {
                                                    if let Some(picker) = this.state.as_mut() {
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
                                                        cancel_delete_picker.update(cx, |this, cx| {
                                                            if let Some(picker) = this.state.as_mut() {
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
                                                        confirm_delete_picker.update(cx, |this, cx| {
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
                                                select_picker.update(cx, |this, cx| {
                                                    if let Some(picker) = this.state.as_mut() {
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
                div()
                    .debug_selector(|| "board-template-picker-footer".into())
                    .child(
                    DialogFooter::new()
                        .px_5()
                        .py_3()
                        .border_t_1()
                        .border_color(cx.theme().border.opacity(0.72))
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .debug_selector(|| "cancel-board-template".into())
                                .flex_none()
                                .child(DialogClose::new().child(
                                    Button::new("cancel-board-template")
                                        .label("Cancel")
                                        .outline()
                                        .disabled(creating),
                                )),
                        )
                        .child(
                            div()
                                .debug_selector(|| "create-board-from-template".into())
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
                                            picker_view.update(cx, |this, cx| {
                                                this.create_selected_board_template(window, cx);
                                            });
                                        }),
                                ),
                        ),
                    ),
            )
            .into_any_element()
    }

    fn create_selected_board_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = self.state.as_mut() else {
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
        let task = cx.global::<AppRuntime>().spawn_store(
            cx.background_executor(),
            move |store| async move {
                storage::board::templates::create_board_from_template(
                    &store,
                    project_id,
                    title,
                    template.definition,
                )
                .await
            },
        );
        let picker_view = cx.entity().downgrade();
        cx.notify();

        cx.spawn_in(window, async move |_, window| {
            let result = task.await;

            window
                .update(|window, cx| match result {
                    Ok(Ok(inserted)) => {
                        if let Some(picker_view) = picker_view.upgrade() {
                            picker_view.update(cx, |this, cx| {
                                this.dialog_open = false;
                                this.state = None;
                                cx.emit(BoardTemplatePickerEvent::BoardCreated {
                                    board_id: inserted.id,
                                    project_id,
                                    title: SharedString::from(inserted.title),
                                });
                            });
                        }
                        window.close_dialog(cx);
                    }
                    Ok(Err(error)) => {
                        if let Some(picker_view) = picker_view.upgrade() {
                            picker_view.update(cx, |this, cx| {
                                if let Some(picker) = this.state.as_mut() {
                                    picker.creating = false;
                                    picker.error =
                                        Some(format!("Could not create the board: {error}").into());
                                    cx.notify();
                                }
                            });
                        }
                    }
                    Err(error) => {
                        if let Some(picker_view) = picker_view.upgrade() {
                            picker_view.update(cx, |this, cx| {
                                if let Some(picker) = this.state.as_mut() {
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
        let Some(picker) = self.state.as_mut() else {
            return;
        };
        if picker.deleting_template_id.is_some() || picker.creating {
            return;
        }
        picker.deleting_template_id = Some(template_id);
        picker.error = None;
        let task = cx.global::<AppRuntime>().spawn_store(
            cx.background_executor(),
            move |store| async move {
                storage::board::templates::delete_custom_template(&store, template_id).await
            },
        );
        cx.notify();

        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                let Some(picker) = this.state.as_mut() else {
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

#[cfg(test)]
mod tests {
    use super::*;

    use gpui::{Render, TestAppContext, VisualTestContext, size};
    use gpui_component::Root;

    struct EmptyView;

    impl Render for EmptyView {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .bg(cx.theme().background)
                .children(Root::render_dialog_layer(window, cx))
        }
    }

    #[gpui::test]
    fn template_picker_footer_reaches_the_dialog_bottom(cx: &mut TestAppContext) {
        let mut picker = None;
        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.open_window(Default::default(), |window, cx| {
                let templates = storage::board::templates::built_in_templates();
                let selected_key = templates[0].id.key();
                let title_input = cx.new(|cx| InputState::new(window, cx).default_value("Board"));
                picker = Some(cx.new(|_| BoardTemplatePicker {
                    dialog_open: true,
                    state: Some(BoardTemplatePickerState {
                        project_id: None,
                        project_name: None,
                        title_input,
                        templates,
                        selected_key,
                        loading_custom: false,
                        creating: false,
                        confirm_delete_template_id: None,
                        deleting_template_id: None,
                        error: None,
                    }),
                }));
                let view = cx.new(|_| EmptyView);
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("template picker test window should open")
        });
        let picker = picker.expect("template picker should exist");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(1_000.), px(800.)));
        cx.update(|window, cx| {
            picker.update(cx, |picker, cx| {
                picker.open_template_picker_dialog(window, cx)
            });
        });
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        assert!(picker.read_with(&cx, |picker, _| picker.dialog_open));
        assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
        assert!(cx.debug_bounds("board-template-picker").is_some());

        let picker_bounds = cx
            .debug_bounds("board-template-picker")
            .expect("template picker should render");
        let template_list = cx
            .debug_bounds("board-template-list")
            .expect("template list should render");
        let footer = cx
            .debug_bounds("board-template-picker-footer")
            .expect("template picker footer should render");
        let cancel_button = cx
            .debug_bounds("cancel-board-template")
            .expect("cancel button should render");
        let create_button = cx
            .debug_bounds("create-board-from-template")
            .expect("create button should render");
        assert!(
            footer.bottom().as_f32() >= TEMPLATE_DIALOG_HEIGHT,
            "footer must use the full configured dialog height: {footer:?}"
        );
        assert_eq!(
            template_list.right(),
            picker_bounds.right(),
            "the scroll owner must reach the dialog content edge"
        );
        assert_eq!(
            cancel_button.size.height, create_button.size.height,
            "footer actions must have matching heights"
        );
        assert!(
            cancel_button.size.width < px(200.),
            "cancel action must keep its intrinsic button width: {cancel_button:?}"
        );
    }
}
