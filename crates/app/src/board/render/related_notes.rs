use super::*;

impl BoardView {
    pub(super) fn render_related_notes_popover(
        &self,
        item: storage::workspace_links::WorkspaceItemRef,
        id: SharedString,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
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
        let popover_id = SharedString::from(format!("related-notes-{id}"));
        let trigger_id = SharedString::from(format!("related-notes-trigger-{id}"));
        let create_id = id.clone();
        let related_id = id.clone();
        let candidate_id = id.clone();

        Popover::new(popover_id)
            .anchor(Anchor::TopRight)
            .open(picker_open)
            .on_open_change(cx.listener(move |this, open, window, cx| {
                this.related_notes
                    .picker
                    .set_open(*open, Some(item), window, cx);
                cx.notify();
            }))
            .w_80()
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
                    .max_h(px(480.))
                    .child(
                        h_flex()
                            .p_3()
                            .gap_2()
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .child(
                                div()
                                    .flex_1()
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, window, cx| {
                                            match event.keystroke.key.as_str() {
                                                "up" => this.move_related_note_candidate(-1, cx),
                                                "down" => this.move_related_note_candidate(1, cx),
                                                "escape" => this
                                                    .related_notes
                                                    .picker
                                                    .set_open(false, None, window, cx),
                                                _ => return,
                                            }
                                            window.prevent_default();
                                            cx.stop_propagation();
                                        },
                                    ))
                                    .child(Input::new(&self.related_notes.picker.search_input)),
                            )
                            .child(
                                Button::new(SharedString::from(format!("create-note-{create_id}")))
                                    .icon(IconName::Plus)
                                    .ghost()
                                    .small()
                                    .tooltip("Create linked note")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.create_note_for_item(item, cx);
                                    })),
                            ),
                    )
                    .when(!related.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .py_1()
                                .border_b_1()
                                .border_color(cx.theme().border.opacity(0.72))
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
                                        .px_2()
                                        .gap_1()
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
                            .max_h(px(280.))
                            .overflow_y_scrollbar()
                            .when(candidates.is_empty(), |this| {
                                this.child(
                                    div()
                                        .p_4()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("No notes match"),
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
                                    Button::new(SharedString::from(format!(
                                        "item-note-candidate-{candidate_id}-{note_id}"
                                    )))
                                    .label(candidate.title)
                                    .tooltip(
                                        candidate
                                            .project_name
                                            .unwrap_or_else(|| "Standalone note".into()),
                                    )
                                    .ghost()
                                    .disabled(already_linked || pending)
                                    .selected(
                                        !already_linked
                                            && !pending
                                            && self.related_notes.picker.active_row == index,
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.link_note_to_item(item, note_id, cx);
                                    }))
                                },
                            )),
                    ),
            )
    }

    pub(super) fn render_entry_related_notes(
        &self,
        selected_entry: Option<(&str, &BoardCardDTO)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let item = selected_entry.map(|(_, card)| storage::workspace_links::WorkspaceItemRef {
            kind: storage::workspace_links::WorkspaceItemKind::Card,
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
                            .w_80()
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
                                    .max_h(px(420.))
                                    .child(
                                        div()
                                            .p_3()
                                            .border_b_1()
                                            .border_color(theme.border.opacity(0.72))
                                            .on_key_down(cx.listener(
                                                |this, event: &KeyDownEvent, window, cx| {
                                                    match event.keystroke.key.as_str() {
                                                        "up" => {
                                                            this.move_related_note_candidate(-1, cx)
                                                        }
                                                        "down" => {
                                                            this.move_related_note_candidate(1, cx)
                                                        }
                                                        "escape" => this
                                                            .related_notes
                                                            .picker
                                                            .set_open(false, None, window, cx),
                                                        _ => return,
                                                    }
                                                    window.prevent_default();
                                                    cx.stop_propagation();
                                                },
                                            ))
                                            .child(Input::new(
                                                &self.related_notes.picker.search_input,
                                            )),
                                    )
                                    .child(
                                        v_flex()
                                            .max_h(px(300.))
                                            .overflow_y_scrollbar()
                                            .when(candidates.is_empty(), |this| {
                                                this.child(
                                                    div()
                                                        .p_4()
                                                        .text_sm()
                                                        .text_color(theme.muted_foreground)
                                                        .child("No notes match"),
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
                                                    Button::new(SharedString::from(format!(
                                                        "related-note-candidate-{note_id}"
                                                    )))
                                                    .label(candidate.title)
                                                    .tooltip(candidate.project_name.unwrap_or_else(
                                                        || "Standalone note".into(),
                                                    ))
                                                    .ghost()
                                                    .disabled(already_linked || pending)
                                                    .selected(
                                                        !already_linked
                                                            && !pending
                                                            && self.related_notes.picker.active_row
                                                                == index,
                                                    )
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        if let Some(item) =
                                                            this.selected_workspace_item()
                                                        {
                                                            this.link_note_to_item(
                                                                item, note_id, cx,
                                                            );
                                                        }
                                                    }))
                                                },
                                            )),
                                    )
                                    .child(
                                        Button::new("create-related-note")
                                            .icon(IconName::Plus)
                                            .label("Create linked note")
                                            .ghost()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                if let Some(item) = this.selected_workspace_item() {
                                                    this.create_note_for_item(item, cx);
                                                }
                                            })),
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
