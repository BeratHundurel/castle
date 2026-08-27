use super::*;

impl DocumentEditorView {
    pub(super) fn render_outline(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.inspector_links.tab == DocumentInspectorTab::Links {
            return self.render_links_inspector(cx).into_any_element();
        }
        let selected = self.analysis.outline_selected;
        let rows = self.analysis.outline_rows.clone();
        let kind = self.kind;
        let empty = rows.is_empty();
        let can_expand_all = self.analysis.outline.can_expand_all();
        let can_collapse_all = self.analysis.outline.can_collapse_all();
        let empty_message = if self.kind == DocumentKind::Json {
            "Add JSON properties or array items to navigate this document."
        } else {
            "Add headings to navigate this note."
        };
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);

        v_flex()
            .id("document-outline")
            .key_context("DocumentOutline")
            .track_focus(&self.analysis.outline_focus_handle)
            .relative()
            .w(outline_width)
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar.opacity(0.72))
            .on_action(cx.listener(Self::on_action_outline_previous))
            .on_action(cx.listener(Self::on_action_outline_next))
            .on_action(cx.listener(Self::on_action_outline_left))
            .on_action(cx.listener(Self::on_action_outline_right))
            .on_action(cx.listener(Self::on_action_outline_open))
            .on_action(cx.listener(Self::on_action_outline_close))
            .child(
                h_flex()
                    .h_10()
                    .px_3()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.7))
                    .child(self.render_inspector_tabs(cx))
                    .child(
                        h_flex()
                            .gap_1()
                            .children((kind == DocumentKind::Json).then(|| {
                                h_flex()
                                    .gap_0p5()
                                    .child(
                                        Button::new("expand-all-json-outline")
                                            .icon(IconName::ChevronDown)
                                            .ghost()
                                            .xsmall()
                                            .disabled(!can_expand_all)
                                            .tooltip("Expand all JSON nodes")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_all_outline_nodes_expanded(true, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("collapse-all-json-outline")
                                            .icon(IconName::ChevronRight)
                                            .ghost()
                                            .xsmall()
                                            .disabled(!can_collapse_all)
                                            .tooltip("Collapse all JSON nodes")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.set_all_outline_nodes_expanded(false, cx);
                                            })),
                                    )
                            }))
                            .child(
                                Button::new("close-document-outline")
                                    .icon(IconName::Close)
                                    .ghost()
                                    .xsmall()
                                    .tooltip("Hide outline (Ctrl+Shift+O)")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.toggle_outline(window, cx);
                                    })),
                            ),
                    ),
            )
            .when_else(
                empty,
                |this| {
                    this.child(
                        v_flex()
                            .p_4()
                            .gap_2()
                            .text_color(cx.theme().muted_foreground)
                            .child(Icon::new(IconName::PanelRight).small())
                            .child(div().text_sm().child(empty_message)),
                    )
                },
                |this| {
                    this.child(
                        uniform_list("document-outline-rows", rows.len(), {
                            cx.processor(move |_this, visible_range: Range<usize>, _window, cx| {
                                visible_range
                                    .filter_map(|index| {
                                        let row = rows.get(index)?.clone();
                                        let is_selected = selected == Some(index);
                                        let chevron = row.has_children.then(|| {
                                            let icon = if row.expanded {
                                                IconName::ChevronDown
                                            } else {
                                                IconName::ChevronRight
                                            };
                                            div()
                                                .id(("outline-chevron", index))
                                                .size_4()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.stop_propagation()
                                                })
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.toggle_outline_node(index, cx);
                                                }))
                                                .child(Icon::new(icon).xsmall())
                                        });

                                        Some(
                                            h_flex()
                                                .id(("outline-item", index))
                                                .w_full()
                                                .h_7()
                                                .px_2()
                                                .pl(outline_row_left_padding(row.depth))
                                                .gap_1()
                                                .rounded(cx.theme().radius)
                                                .text_size(px(13.))
                                                .font_weight(
                                                    if kind == DocumentKind::Markdown
                                                        && row.depth == 0
                                                    {
                                                        FontWeight::SEMIBOLD
                                                    } else {
                                                        FontWeight::NORMAL
                                                    },
                                                )
                                                .text_color(if row.disabled {
                                                    cx.theme().muted_foreground.opacity(0.65)
                                                } else if is_selected {
                                                    cx.theme().primary
                                                } else {
                                                    cx.theme().muted_foreground
                                                })
                                                .bg(if is_selected {
                                                    cx.theme().accent.opacity(0.55)
                                                } else {
                                                    cx.theme().sidebar.opacity(0.)
                                                })
                                                .when(!row.disabled, |element| {
                                                    element
                                                        .hover(|this| {
                                                            this.bg(cx.theme().accent.opacity(0.38))
                                                        })
                                                        .on_click(cx.listener(
                                                            move |this, _, window, cx| {
                                                                this.analysis
                                                                    .outline_focus_handle
                                                                    .focus(window, cx);
                                                                this.select_outline_item(
                                                                    index, window, cx,
                                                                );
                                                            },
                                                        ))
                                                })
                                                .children(chevron)
                                                .when(
                                                    reserves_disclosure_space(
                                                        kind,
                                                        row.has_children,
                                                    ),
                                                    |element| element.child(div().w_4()),
                                                )
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .overflow_hidden()
                                                        .text_ellipsis()
                                                        .child(row.title),
                                                ),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            })
                        })
                        .flex_1()
                        .min_h_0()
                        .p_2()
                        .track_scroll(&self.analysis.outline_scroll_handle)
                        .with_sizing_behavior(ListSizingBehavior::Auto),
                    )
                },
            )
            .child(self.render_outline_resize_handle(cx))
            .vertical_scrollbar(&self.analysis.outline_scroll_handle)
            .into_any_element()
    }

    fn render_outline_resize_handle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let editor_id = cx.entity_id();
        let drag = OutlineResizeDrag { editor_id };

        div()
            .id("document-outline-resize-handle")
            .group("document-outline-resize-handle")
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(-4.))
            .w(px(8.))
            .flex()
            .justify_center()
            .cursor_col_resize()
            .child(
                div()
                    .h_full()
                    .w(px(1.))
                    .bg(cx.theme().border)
                    .group_hover("document-outline-resize-handle", |this| {
                        this.bg(cx.theme().drag_border)
                    }),
            )
            .on_drag_move(cx.listener(
                move |this, event: &DragMoveEvent<OutlineResizeDrag>, _, cx| {
                    if event.drag(cx).editor_id != cx.entity_id() {
                        return;
                    }
                    this.resize_outline_from_pointer(event.event.position.x, cx);
                },
            ))
            .on_drag(drag, |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
    }

    fn resize_outline_from_pointer(&mut self, pointer_x: Pixels, cx: &mut Context<Self>) {
        let Some(view_bounds) = self.view_bounds else {
            return;
        };
        let requested_width = view_bounds.right() - pointer_x;
        let width = outline_width_for_view(requested_width, view_bounds.size.width);
        if self.outline_width != width {
            self.outline_width = width;
            cx.notify();
        }
    }

    pub(super) fn render_outline_transition(&self, cx: &mut Context<Self>) -> AnyElement {
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);
        let wrapper = div()
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .child(self.render_outline(cx));

        if self.analysis.outline_transition_epoch == 0 {
            return wrapper.w(outline_width).into_any_element();
        }

        let (from_width, to_width) = if self.analysis.outline_visible {
            (px(0.), outline_width)
        } else {
            (outline_width, px(0.))
        };

        wrapper
            .with_animation(
                (
                    "document-outline-transition",
                    self.analysis.outline_transition_epoch,
                ),
                Animation::new(crate::OUTLINE_TRANSITION_DURATION).with_easing(ease_in_out_cubic),
                move |this, delta| this.w(from_width + (to_width - from_width) * delta),
            )
            .into_any_element()
    }
}
