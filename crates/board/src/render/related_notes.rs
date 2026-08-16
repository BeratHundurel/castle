use super::*;

fn render_related_note_candidate(
    id: SharedString,
    title: String,
    project_name: String,
    selected: bool,
    disabled: bool,
    theme: &gpui_component::Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let theme = theme.clone();

    h_flex()
        .id(id)
        .w_full()
        .h(px(48.))
        .px_2()
        .gap_2()
        .rounded(theme.radius)
        .when(selected, |this| this.bg(theme.primary.opacity(0.14)))
        .when(!selected && !disabled, |this| {
            this.hover(|this| this.bg(theme.accent.opacity(0.5)))
        })
        .when(disabled, |this| this.opacity(0.55))
        .when(!disabled, |this| this.cursor_pointer().on_click(on_click))
        .child(
            Icon::new(IconName::File)
                .xsmall()
                .text_color(theme.muted_foreground),
        )
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .gap_0p5()
                .child(
                    div()
                        .truncate()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(title),
                )
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(project_name),
                ),
        )
        .child(
            Icon::new(IconName::Plus)
                .xsmall()
                .text_color(theme.muted_foreground),
        )
}

impl BoardView {
    pub(super) fn render_related_notes_popover(
        &self,
        item: storage::workspace::links::WorkspaceItemRef,
        id: SharedString,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let related = self.related_notes_for_item(item);
        let linked_ids = related
            .iter()
            .map(|note| note.note_id)
            .collect::<std::collections::HashSet<_>>();

        let picker_open =
            self.related_notes.picker.open && self.related_notes.picker.target == Some(item);

        let candidates = if picker_open {
            self.related_note_candidates(item, cx)
        } else {
            Vec::new()
        };

        let candidate_list_height = px(((candidates.len().max(1) as f32 * 48.) + 8.).min(248.));
        let popover_id = SharedString::from(format!("related-notes-{id}"));
        let trigger_id = SharedString::from(format!("related-notes-trigger-{id}"));
        let create_id = id.clone();
        let related_id = id.clone();
        let candidate_id = id.clone();

        Popover::new(popover_id)
            .anchor(Anchor::TopCenter)
            .open(picker_open)
            .on_open_change(cx.listener(move |this, open, window, cx| {
                this.related_notes
                    .picker
                    .set_open(*open, Some(item), window, cx);
                cx.notify();
            }))
            .w(px(384.))
            .p_0()
            .trigger(
                Button::new(trigger_id)
                    .icon(IconName::File)
                    .label(related.len().to_string())
                    .ghost()
                    .compact()
                    .tooltip("Related notes"),
            )
            .child(
                v_flex()
                    .w_full()
                    .max_h(px(520.))
                    .child(
                        v_flex()
                            .p_3()
                            .gap_2()
                            .border_b_1()
                            .border_color(theme.border.opacity(0.72))
                            .child(
                                h_flex()
                                    .min_h_7()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.foreground)
                                            .child("Related notes"),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!(
                                            "create-note-{create_id}"
                                        )))
                                        .icon(IconName::Plus)
                                        .ghost()
                                        .small()
                                        .tooltip("Create linked note")
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.create_note_for_item(item, cx);
                                            }),
                                        ),
                                    ),
                            )
                            .child(Input::new(&self.related_notes.picker.search_input)),
                    )
                    .when(!related.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .p_2()
                                .gap_1()
                                .border_b_1()
                                .border_color(theme.border.opacity(0.72))
                                .child(
                                    div()
                                        .px_1()
                                        .pb_1()
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.muted_foreground)
                                        .child(format!("Related · {}", related.len())),
                                )
                                .children(related.into_iter().map(|note| {
                                    let note_id = note.note_id;
                                    let manual = note.manually_linked();
                                    let pending = self
                                        .related_notes
                                        .picker
                                        .pending
                                        .contains(&(item, note_id));
                                    let related_id = related_id.clone();
                                    h_flex()
                                        .id(SharedString::from(format!(
                                            "item-related-note-{related_id}-{note_id}"
                                        )))
                                        .min_h_9()
                                        .px_1()
                                        .gap_2()
                                        .child(
                                            Icon::new(IconName::File)
                                                .xsmall()
                                                .text_color(theme.muted_foreground),
                                        )
                                        .child(
                                            Button::new(SharedString::from(format!(
                                                "open-item-note-{related_id}-{note_id}"
                                            )))
                                            .label(note.title)
                                            .ghost()
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.open_related_note(note_id, cx);
                                            })),
                                        )
                                        .child(div().flex_1())
                                        .when(manual, |this| {
                                            this.child(
                                                Button::new(SharedString::from(format!(
                                                    "unlink-item-note-{related_id}-{note_id}"
                                                )))
                                                .icon(IconName::Close)
                                                .ghost()
                                                .xsmall()
                                                .disabled(pending)
                                                .tooltip("Remove manual link")
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.unlink_note_from_item(item, note_id, cx);
                                                })),
                                            )
                                        })
                                })),
                        )
                    })
                    .child(
                        v_flex()
                            .child(
                                div()
                                    .px_3()
                                    .pt_3()
                                    .pb_1()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.muted_foreground)
                                    .child("Add a note"),
                            )
                            .child(
                                v_flex()
                                    .id(SharedString::from(format!(
                                        "related-note-candidates-{candidate_id}"
                                    )))
                                    .h(candidate_list_height)
                                    .max_h(px(248.))
                                    .px_1()
                                    .py_1()
                                    .overflow_y_scroll()
                                    .when(picker_open, |this| {
                                        this.track_scroll(&self.related_notes.picker.scroll_handle)
                                    })
                                    .when(candidates.is_empty(), |this| {
                                        this.child(
                                            h_flex()
                                                .h_12()
                                                .px_2()
                                                .text_sm()
                                                .text_color(theme.muted_foreground)
                                                .child("No notes match your search"),
                                        )
                                    })
                                    .children(candidates.into_iter().enumerate().map(
                                        |(index, candidate)| {
                                            let note_id = candidate.item.id;
                                            let already_linked = linked_ids.contains(&note_id);
                                            let pending = self
                                                .related_notes
                                                .picker
                                                .pending
                                                .contains(&(item, note_id));
                                            let selected = !already_linked
                                                && !pending
                                                && self
                                                    .related_notes
                                                    .picker
                                                    .keyboard_selection_visible
                                                && self.related_notes.picker.active_row == index;
                                            let project_name = candidate
                                                .project_name
                                                .unwrap_or_else(|| "Standalone".into());

                                            render_related_note_candidate(
                                                SharedString::from(format!(
                                                    "item-note-candidate-{candidate_id}-{note_id}"
                                                )),
                                                candidate.title,
                                                project_name,
                                                selected,
                                                already_linked || pending,
                                                &theme,
                                                cx.listener(move |this, _, _, cx| {
                                                    this.link_note_to_item(item, note_id, cx);
                                                }),
                                            )
                                        },
                                    )),
                            ),
                    ),
            )
    }

    pub(super) fn render_entry_related_notes(
        &self,
        selected_entry: Option<(&str, &BoardCardDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let item = selected_entry.map(|(_, card)| storage::workspace::links::WorkspaceItemRef {
            kind: storage::workspace::links::WorkspaceItemKind::Card,
            id: i64::from(card.id),
        });
        let related = item
            .map(|item| self.related_notes_for_item(item))
            .unwrap_or_default();
        let linked_ids = related
            .iter()
            .map(|note| note.note_id)
            .collect::<std::collections::HashSet<_>>();
        let picker_open = item.is_some()
            && self.related_notes.picker.open
            && self.related_notes.picker.target == item;
        let candidates = if let Some(item) = item.filter(|_| picker_open) {
            self.related_note_candidates(item, cx)
        } else {
            Vec::new()
        };
        let candidate_list_height = px(((candidates.len().max(1) as f32 * 48.) + 8.).min(248.));

        v_flex()
            .gap_3()
            .p_3()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border.opacity(0.48))
            .bg(theme.secondary.opacity(0.12))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::File).xsmall())
                            .child(if related.is_empty() {
                                "Related notes".to_string()
                            } else {
                                format!("Related notes · {}", related.len())
                            }),
                    )
                    .child(
                        Popover::new("related-note-picker")
                            .anchor(Anchor::TopRight)
                            .open(picker_open)
                            .on_open_change(cx.listener(|this, open, window, cx| {
                                let target = this.selected_workspace_item();
                                this.related_notes
                                    .picker
                                    .set_open(*open, target, window, cx);
                                cx.notify();
                            }))
                            .w(px(384.))
                            .p_0()
                            .trigger(
                                Button::new("add-related-note")
                                    .icon(IconName::Plus)
                                    .label("Link note")
                                    .ghost()
                                    .small(),
                            )
                            .child(
                                v_flex()
                                    .w_full()
                                    .max_h(px(480.))
                                    .child(
                                        v_flex()
                                            .p_3()
                                            .gap_2()
                                            .border_b_1()
                                            .border_color(theme.border.opacity(0.72))
                                            .child(
                                                h_flex()
                                                    .min_h_7()
                                                    .justify_between()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .text_sm()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.foreground)
                                                            .child("Related notes"),
                                                    )
                                                    .child(
                                                        Button::new("create-related-note")
                                                            .icon(IconName::Plus)
                                                            .ghost()
                                                            .small()
                                                            .tooltip("Create linked note")
                                                            .on_click(cx.listener(
                                                                |this, _, _, cx| {
                                                                    if let Some(item) =
                                                                        this.selected_workspace_item()
                                                                    {
                                                                        this.create_note_for_item(
                                                                            item, cx,
                                                                        );
                                                                    }
                                                                },
                                                            )),
                                                    ),
                                            )
                                            .child(Input::new(
                                                &self.related_notes.picker.search_input,
                                            )),
                                    )
                                    .child(
                                        v_flex()
                                            .child(
                                                div()
                                                    .px_3()
                                                    .pt_3()
                                                    .pb_1()
                                                    .text_xs()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(theme.muted_foreground)
                                                    .child("Add a note"),
                                            )
                                            .child(
                                                v_flex()
                                                    .id("entry-related-note-candidates")
                                                    .h(candidate_list_height)
                                                    .max_h(px(248.))
                                                    .px_1()
                                                    .py_1()
                                                    .overflow_y_scroll()
                                                    .when(picker_open, |this| {
                                                        this.track_scroll(
                                                            &self.related_notes.picker.scroll_handle,
                                                        )
                                                    })
                                                    .when(candidates.is_empty(), |this| {
                                                        this.child(
                                                            h_flex()
                                                                .h_12()
                                                                .px_2()
                                                                .text_sm()
                                                                .text_color(theme.muted_foreground)
                                                                .child(
                                                                    "No notes match your search",
                                                                ),
                                                        )
                                                    })
                                                    .children(candidates.into_iter().enumerate().map(
                                                        |(index, candidate)| {
                                                            let note_id = candidate.item.id;
                                                            let already_linked =
                                                                linked_ids.contains(&note_id);
                                                            let pending = item.is_some_and(|item| {
                                                                self.related_notes
                                                                    .picker
                                                                    .pending
                                                                    .contains(&(item, note_id))
                                                            });
                                                            let selected = !already_linked
                                                                && !pending
                                                                && self
                                                                    .related_notes
                                                                    .picker
                                                                    .keyboard_selection_visible
                                                                && self.related_notes.picker.active_row
                                                                    == index;
                                                            let project_name = candidate
                                                                .project_name
                                                                .unwrap_or_else(|| {
                                                                    "Standalone".into()
                                                                });

                                                            render_related_note_candidate(
                                                                SharedString::from(format!(
                                                                    "related-note-candidate-{note_id}"
                                                                )),
                                                                candidate.title,
                                                                project_name,
                                                                selected,
                                                                already_linked || pending,
                                                                &theme,
                                                                cx.listener(
                                                                    move |this, _, _, cx| {
                                                                        if let Some(item) = this
                                                                            .selected_workspace_item()
                                                                        {
                                                                            this.link_note_to_item(
                                                                                item, note_id, cx,
                                                                            );
                                                                        }
                                                                    },
                                                                ),
                                                            )
                                                        },
                                                    )),
                                            ),
                                    ),
                            ),
                    ),
            )
            .when(related.is_empty(), |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Connect long-form context without copying it into this card."),
                )
            })
            .children(related.into_iter().map(|note| {
                let note_id = note.note_id;
                let manual = note.manually_linked();
                let pending = item.is_some_and(|item| {
                    self.related_notes.picker.pending.contains(&(item, note_id))
                });
                h_flex()
                    .id(SharedString::from(format!("related-note-{note_id}")))
                    .min_h_10()
                    .gap_2()
                    .items_center()
                    .child(Icon::new(IconName::File).xsmall())
                    .child(
                        Button::new(SharedString::from(format!("open-related-note-{note_id}")))
                            .label(note.title)
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_related_note(note_id, cx);
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(if manual { "Linked" } else { "In description" }),
                    )
                    .when(manual, |this| {
                        this.child(
                            Button::new(SharedString::from(format!(
                                "unlink-related-note-{note_id}"
                            )))
                            .icon(IconName::Close)
                            .ghost()
                            .xsmall()
                            .disabled(pending)
                            .tooltip("Remove manual link")
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    if let Some(item) = this.selected_workspace_item() {
                                        this.unlink_note_from_item(item, note_id, cx);
                                    }
                                },
                            )),
                        )
                    })
            }))
            .when_some(self.related_notes.error.clone(), |this, error| {
                this.child(div().text_xs().text_color(theme.danger).child(error))
            })
    }
}
