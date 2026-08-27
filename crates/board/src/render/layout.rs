use super::*;

impl BoardView {
    pub(super) fn render_scrollable_board(
        &self,
        board_id_for_render: Option<u32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let mut cards = Vec::new();

        if let Some(error) = self.mutation.load_error.clone() {
            return div()
                .id("board-load-error")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .text_color(theme.danger)
                .child(error)
                .into_any_element();
        }

        if board_id_for_render.is_some()
            && self.data.lists.is_empty()
            && !self.entry_editing.adding_list
        {
            return self.render_empty_board(cx).into_any_element();
        }

        if let Some(board_id) = board_id_for_render {
            for card in &self.data.lists {
                cards.push(self.render_card(card, board_id, cx).into_any_element());
            }
        }

        cards.push(if self.entry_editing.adding_list {
            Input::new(&self.entry_editing.new_list_input)
                .w_80()
                .min_w_80()
                .h_10()
                .rounded_none()
                .focus_bordered(false)
                .border_0()
                .border_b_1()
                .border_color(theme.foreground)
                .into_any_element()
        } else {
            self.render_add_list_button(cx).into_any_element()
        });

        let scrollable =
            horizontal_board_viewport(&self.board_scroll_handle, cards).into_any_element();

        if board_id_for_render.is_some() {
            v_flex()
                .id("scrollable-container-with-board_id")
                .size_full()
                .overflow_hidden()
                .when_some(self.mutation.mutation_error.clone(), |this, error| {
                    this.child(
                        div()
                            .px_4()
                            .py_2()
                            .border_b_1()
                            .border_color(theme.danger.opacity(0.35))
                            .bg(theme.danger.opacity(0.08))
                            .text_sm()
                            .text_color(theme.danger)
                            .child(error),
                    )
                })
                .child(self.render_filter_toolbar(cx))
                .child(scrollable)
                .into_any_element()
        } else {
            scrollable
        }
    }

    pub(super) fn render_filter_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("board-filter-toolbar")
            .min_h_10()
            .px_3()
            .gap_1()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.72))
            .bg(cx.theme().background)
            .can_drop(|value, _, _| {
                value
                    .downcast_ref::<WorkspaceDragInfo>()
                    .and_then(WorkspaceDragInfo::note_id)
                    .is_some()
            })
            .drag_over::<WorkspaceDragInfo>(|this, _, _, cx| {
                this.border_1()
                    .border_color(cx.theme().primary)
                    .bg(cx.theme().drop_target)
            })
            .on_drop(cx.listener(|this, info: &WorkspaceDragInfo, _, cx| {
                if let (Some(board_id), Some(note_id)) = (this.data.board_id, info.note_id()) {
                    this.link_note_to_item(
                        storage::workspace::links::WorkspaceItemRef {
                            kind: storage::workspace::links::WorkspaceItemKind::Board,
                            id: i64::from(board_id),
                        },
                        note_id,
                        cx,
                    );
                }
            }))
            .child(self.render_view_picker(cx))
            .child(
                div()
                    .mx_1()
                    .h_5()
                    .w(px(1.))
                    .bg(cx.theme().border.opacity(0.72)),
            )
            .when(!self.filters.due_dates.is_empty(), |this| {
                this.child(
                    Button::new("active-due-date-filter")
                        .icon(IconName::Close)
                        .label("Due date")
                        .ghost()
                        .xsmall()
                        .tooltip("Remove due date filter")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_due_date_filters(cx);
                        })),
                )
            })
            .when(!self.filters.label_ids.is_empty(), |this| {
                this.child(
                    Button::new("active-label-filter")
                        .icon(IconName::Close)
                        .label("Labels")
                        .ghost()
                        .xsmall()
                        .tooltip("Remove label filter")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_label_filters(cx);
                        })),
                )
            })
            .when_some(self.filters.related_notes, |this, _| {
                this.child(
                    Button::new("active-related-notes-filter")
                        .icon(IconName::Close)
                        .label("Related notes")
                        .ghost()
                        .xsmall()
                        .tooltip("Remove related notes filter")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.set_related_notes_filter(None, cx);
                        })),
                )
            })
            .children(self.filters.custom.iter().filter_map(|filter| {
                let storage::board::properties::PropertyKey::Custom(property_id) = &filter.property
                else {
                    return None;
                };
                let property_id = *property_id;
                Some(
                    Button::new(SharedString::from(format!(
                        "active-custom-filter-{property_id}"
                    )))
                    .icon(IconName::Close)
                    .label(self.property_key_label(&filter.property))
                    .ghost()
                    .xsmall()
                    .tooltip("Remove property filter")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.remove_custom_filter(property_id, cx);
                    })),
                )
            }))
            .when_some(self.data.board_id, |this, board_id| {
                this.child(self.render_related_notes_popover(
                    storage::workspace::links::WorkspaceItemRef {
                        kind: storage::workspace::links::WorkspaceItemKind::Board,
                        id: i64::from(board_id),
                    },
                    "board".into(),
                    cx,
                ))
            })
            .child(div().flex_1())
            .when(
                self.filters.is_active() || self.properties.active_view_config.sort.is_some(),
                |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Dragging is paused in this view"),
                    )
                },
            )
            .child(
                Popover::new("board-filter-popover")
                    .anchor(Anchor::TopRight)
                    .open(self.filter_panel_open)
                    .on_open_change(cx.listener(|this, open, _, cx| {
                        this.set_filter_panel_open(*open, cx);
                    }))
                    .p_0()
                    .w_80()
                    .trigger(
                        Button::new("toggle-board-filters")
                            .icon(IconName::Settings2)
                            .label("Filter")
                            .ghost()
                            .small()
                            .selected(self.filters.is_active() || self.filter_panel_open)
                            .tooltip("Filter cards"),
                    )
                    .child(self.render_filter_panel(cx)),
            )
            .when(self.filters.is_active(), |this| {
                this.child(
                    Button::new("clear-board-filters")
                        .label("Clear")
                        .ghost()
                        .small()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_filters(cx);
                        })),
                )
            })
            .child(self.render_sort_picker(cx))
            .child(self.render_fields_picker(cx))
            .child(
                Button::new("save-board-template")
                    .icon(IconName::Copy)
                    .label("Template")
                    .ghost()
                    .small()
                    .tooltip("Save this board as a reusable template")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.show_save_template_dialog(window, cx);
                    })),
            )
            .child(
                div()
                    .mx_1()
                    .h_5()
                    .w(px(1.))
                    .bg(cx.theme().border.opacity(0.72)),
            )
            .child(self.render_property_manager(cx))
    }

    pub(super) fn render_filter_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let labels = self.data.labels.clone();
        let board_view = cx.entity();

        v_flex()
            .id("board-filter-panel")
            .w_full()
            .max_h(px(560.))
            .overflow_hidden()
            .text_sm()
            .child(
                h_flex()
                    .flex_shrink_0()
                    .min_h_12()
                    .px_4()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.72))
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Filter cards"),
                    )
                    .when(self.filters.is_active(), |this| {
                        this.child(
                            Button::new("clear-board-filters-popover")
                                .label("Clear all")
                                .ghost()
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.clear_filters(cx);
                                })),
                        )
                    }),
            )
            .child(
                v_flex().flex_1().overflow_hidden().child(
                    div()
                        .relative()
                        .flex_1()
                        .overflow_hidden()
                        .child(
                            v_flex()
                                .id("board-filter-options-scroll")
                                .size_full()
                                .track_scroll(&self.filter_scroll_handle)
                                .overflow_y_scroll()
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .px_4()
                                        .py_3()
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Due date"),
                                        )
                                        .children(
                                            [
                                                (DueDateFilter::Overdue, "Overdue"),
                                                (DueDateFilter::Today, "Today"),
                                                (DueDateFilter::NextSevenDays, "Next 7 days"),
                                                (DueDateFilter::NoDueDate, "No due date"),
                                            ]
                                            .into_iter()
                                            .enumerate()
                                            .map(
                                                |(index, (filter, label))| {
                                                    let selected =
                                                        self.filters.due_dates.contains(&filter);
                                                    let board_view = board_view.clone();

                                                    Checkbox::new(("filter-due-date", index))
                                                        .checked(selected)
                                                        .small()
                                                        .w_full()
                                                        .py_1()
                                                        .label(label)
                                                        .on_click(move |selected, _, cx| {
                                                            board_view.update(cx, |this, cx| {
                                                                this.set_due_date_filter(
                                                                    filter, *selected, cx,
                                                                );
                                                            });
                                                        })
                                                },
                                            ),
                                        ),
                                )
                                .child(self.render_custom_filter_controls(cx))
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .px_4()
                                        .py_3()
                                        .border_t_1()
                                        .border_color(cx.theme().border.opacity(0.72))
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Related notes"),
                                        )
                                        .child(
                                            Checkbox::new("filter-related-notes-present")
                                                .checked(self.filters.related_notes == Some(true))
                                                .small()
                                                .w_full()
                                                .label("Is not empty")
                                                .on_click({
                                                    let board_view = board_view.clone();
                                                    move |selected, _, cx| {
                                                        board_view.update(cx, |this, cx| {
                                                            this.set_related_notes_filter(
                                                                selected.then_some(true),
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        )
                                        .child(
                                            Checkbox::new("filter-related-notes-empty")
                                                .checked(self.filters.related_notes == Some(false))
                                                .small()
                                                .w_full()
                                                .label("Is empty")
                                                .on_click({
                                                    let board_view = board_view.clone();
                                                    move |selected, _, cx| {
                                                        board_view.update(cx, |this, cx| {
                                                            this.set_related_notes_filter(
                                                                selected.then_some(false),
                                                                cx,
                                                            );
                                                        });
                                                    }
                                                }),
                                        ),
                                )
                                .child(
                                    v_flex()
                                        .gap_2()
                                        .px_4()
                                        .py_3()
                                        .border_t_1()
                                        .border_color(cx.theme().border.opacity(0.72))
                                        .child(
                                            div()
                                                .text_xs()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(cx.theme().muted_foreground)
                                                .child("Labels"),
                                        )
                                        .child(
                                            v_flex()
                                                .gap_2()
                                                .when(labels.is_empty(), |this| {
                                                    this.child(
                                                        div()
                                                            .py_1()
                                                            .text_sm()
                                                            .text_color(cx.theme().muted_foreground)
                                                            .child("No labels on this board"),
                                                    )
                                                })
                                                .children(labels.iter().map(|label| {
                                                    let label_id = label.id;
                                                    let selected =
                                                        self.filters.label_ids.contains(&label_id);
                                                    let board_view = board_view.clone();

                                                    Checkbox::new((
                                                        "filter-label",
                                                        label_id as usize,
                                                    ))
                                                    .checked(selected)
                                                    .small()
                                                    .w_full()
                                                    .py_1()
                                                    .label(label.name.clone())
                                                    .on_click(move |selected, _, cx| {
                                                        board_view.update(cx, |this, cx| {
                                                            this.set_label_filter(
                                                                label_id, *selected, cx,
                                                            );
                                                        });
                                                    })
                                                })),
                                        ),
                                ),
                        )
                        .vertical_scrollbar(&self.filter_scroll_handle),
                ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px_4()
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border.opacity(0.72))
                    .bg(cx.theme().secondary.opacity(0.35))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Cards match at least one option in each selected section"),
            )
    }
}

fn horizontal_board_viewport(scroll_handle: &ScrollHandle, cards: Vec<AnyElement>) -> AnyElement {
    div()
        .id("horizontal-board-viewport")
        .relative()
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
        .child(
            h_flex()
                .id("horizontal-board-scroll-area")
                .size_full()
                .track_scroll(scroll_handle)
                .overflow_x_scroll()
                .gap_4()
                .p_4()
                .items_start()
                .children(cards),
        )
        .horizontal_scrollbar(scroll_handle)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::horizontal_board_viewport;
    use crate::{
        BoardView,
        model::{BoardLabel, BoardListState},
    };
    use gpui::{
        Context, Entity, InteractiveElement, IntoElement, ParentElement, Render, ScrollDelta,
        ScrollHandle, ScrollWheelEvent, Styled, TestAppContext, VisualTestContext, Window, div,
        point, px, size,
    };
    use gpui_component::v_flex;
    use runtime::AppRuntime;
    use sea_orm::Database;
    use std::{path::PathBuf, sync::Arc};

    struct HorizontalBoardViewportTest {
        scroll_handle: ScrollHandle,
    }

    impl Render for HorizontalBoardViewportTest {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            v_flex()
                .size_full()
                .child(div().h_10().flex_shrink_0())
                .child(horizontal_board_viewport(
                    &self.scroll_handle,
                    (0usize..4)
                        .map(|index| {
                            div()
                                .id(("board-column-test", index))
                                .w_80()
                                .min_w_80()
                                .h_10()
                                .into_any_element()
                        })
                        .collect(),
                ))
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }

    fn open_board_viewport(cx: &mut TestAppContext) -> (ScrollHandle, &mut VisualTestContext) {
        cx.update(gpui_component::init);
        let scroll_handle = ScrollHandle::new();
        let test_scroll_handle = scroll_handle.clone();
        let (_, cx) = cx.add_window_view(move |_, _| HorizontalBoardViewportTest { scroll_handle });
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(800.), px(400.)));
        draw(cx);
        (test_scroll_handle, cx)
    }

    fn open_filter_panel(
        cx: &mut TestAppContext,
    ) -> (
        tokio::runtime::Runtime,
        Entity<BoardView>,
        VisualTestContext,
    ) {
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(runtime) => runtime,
            Err(error) => panic!("Tokio test runtime should start: {error}"),
        };
        let runtime_guard = runtime.enter();
        let database = match runtime.block_on(Database::connect("sqlite::memory:")) {
            Ok(database) => Arc::new(database),
            Err(error) => panic!("filter panel test database should connect: {error}"),
        };

        let window = cx.update(|cx| {
            cx.set_global(gpui_component::Theme::default());
            gpui_component::init(cx);
            cx.set_global(AppRuntime::new(database, PathBuf::new()));
            match cx.open_window(Default::default(), |window, cx| {
                let view = BoardView::view(window, cx);
                view.update(cx, |board, cx| {
                    board.data.board_id = Some(1);
                    board.data.lists = vec![BoardListState {
                        id: 1,
                        title: "List".into(),
                        board_id: 1,
                        position: 0,
                        entries: Vec::new(),
                    }];
                    board.data.labels = (1..=40)
                        .map(|id| BoardLabel {
                            id,
                            board_id: 1,
                            name: format!("Label {id}").into(),
                            color: "blue".into(),
                        })
                        .collect();
                    board.filter_panel_open = true;
                    cx.notify();
                });
                view
            }) {
                Ok(window) => window,
                Err(error) => panic!("filter panel test window should open: {error}"),
            }
        });
        drop(runtime_guard);

        let view = match window.root(cx) {
            Ok(view) => view,
            Err(error) => panic!("filter panel view should exist: {error}"),
        };
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(1200.), px(700.)));
        draw(&mut cx);
        (runtime, view, cx)
    }

    #[gpui::test]
    fn fixed_columns_overflow_and_add_list_can_be_revealed(cx: &mut TestAppContext) {
        let (scroll_handle, cx) = open_board_viewport(cx);

        assert!(
            scroll_handle.max_offset().x > px(0.),
            "four fixed-width board columns should overflow an 800px viewport"
        );

        let scroll_area = scroll_handle.bounds();
        let Some(first_column) = scroll_handle.bounds_for_item(0) else {
            panic!("first board column should be measured");
        };
        let Some(add_list) = scroll_handle.bounds_for_item(3) else {
            panic!("add-list column should be measured");
        };
        assert_eq!(first_column.size.width, px(320.));
        assert!(add_list.right() > scroll_area.right());

        scroll_handle.scroll_to_item(3);
        draw(cx);

        let scroll_area = scroll_handle.bounds();
        let Some(add_list) = scroll_handle.bounds_for_item(3) else {
            panic!("add-list column should remain measured");
        };
        let painted_left = add_list.left() + scroll_handle.offset().x;
        let painted_right = add_list.right() + scroll_handle.offset().x;
        assert!(painted_left >= scroll_area.left());
        assert!(painted_right <= scroll_area.right());
    }

    #[gpui::test]
    fn vertical_mouse_wheel_pans_horizontal_board(cx: &mut TestAppContext) {
        let (scroll_handle, cx) = open_board_viewport(cx);
        let viewport = scroll_handle.bounds();

        cx.simulate_event(ScrollWheelEvent {
            position: viewport.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-100.))),
            ..Default::default()
        });
        draw(cx);

        assert!(
            scroll_handle.offset().x < px(0.),
            "vertical mouse-wheel input should pan the horizontal board"
        );
    }

    #[gpui::test]
    fn overscrolling_at_end_keeps_scroll_geometry_stable(cx: &mut TestAppContext) {
        let (scroll_handle, cx) = open_board_viewport(cx);
        let scroll_range = scroll_handle.max_offset().x;
        let viewport = scroll_handle.bounds();

        scroll_handle.scroll_to_item(3);
        draw(cx);

        cx.simulate_event(ScrollWheelEvent {
            position: viewport.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-1000.))),
            ..Default::default()
        });
        draw(cx);
        let end_offset = scroll_handle.offset().x;

        cx.simulate_event(ScrollWheelEvent {
            position: viewport.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-1000.))),
            ..Default::default()
        });
        draw(cx);

        assert_eq!(scroll_handle.offset().x, end_offset);
        assert_eq!(scroll_handle.max_offset().x, scroll_range);
        assert_eq!(scroll_handle.bounds(), viewport);
    }

    #[gpui::test]
    fn filter_options_have_bounded_scroll_range_and_respond_to_wheel(cx: &mut TestAppContext) {
        let (_runtime, view, mut cx) = open_filter_panel(cx);
        let scroll_handle = view.read_with(&cx, |board, _| board.filter_scroll_handle.clone());
        let viewport = scroll_handle.bounds();
        assert!(
            viewport.size.height > px(0.),
            "filter options should have a measurable viewport"
        );
        assert!(
            scroll_handle.max_offset().y > px(0.),
            "filter options should overflow when all seeded labels do not fit"
        );

        cx.simulate_event(ScrollWheelEvent {
            position: viewport.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-240.))),
            ..Default::default()
        });
        draw(&mut cx);

        assert!(
            scroll_handle.offset().y < px(0.),
            "wheel input should scroll the filter options"
        );
        assert_eq!(scroll_handle.bounds(), viewport);
    }
}
