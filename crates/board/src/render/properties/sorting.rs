use super::*;

impl BoardView {
    pub(crate) fn render_sort_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut fields = vec![
            PropertyKey::DueDate,
            PropertyKey::Labels,
            PropertyKey::RelatedNotes,
        ];
        fields.extend(
            self.properties
                .data
                .definitions
                .iter()
                .map(|property| PropertyKey::Custom(property.id)),
        );
        let active_sort = self.properties.active_view_config.sort.clone();
        Popover::new("board-sort-picker")
            .anchor(Anchor::TopRight)
            .open(self.properties.sort_panel_open)
            .on_open_change(cx.listener(|this, open, _, cx| {
                this.set_sort_panel_open(*open, cx);
            }))
            .p_0()
            .w(px(280.))
            .trigger(
                Button::new("toggle-board-sort")
                    .icon(IconName::SortAscending)
                    .label(if active_sort.is_some() {
                        "Sort · 1"
                    } else {
                        "Sort"
                    })
                    .ghost()
                    .small()
                    .selected(active_sort.is_some() || self.properties.sort_panel_open)
                    .tooltip("Sort temporarily within each list"),
            )
            .child(
                v_flex()
                    .text_sm()
                    .child(
                        h_flex()
                            .px_4()
                            .py_3()
                            .justify_between()
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Sort within lists"),
                            )
                            .when(active_sort.is_some(), |this| {
                                this.child(
                                    Button::new("clear-board-sort")
                                        .label("Clear")
                                        .ghost()
                                        .xsmall()
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.clear_sort(cx)),
                                        ),
                                )
                            }),
                    )
                    .child(v_flex().justify_center().items_start().p_1().children(
                        fields.into_iter().map(|field| {
                            let selected_sort =
                                active_sort.as_ref().filter(|sort| sort.property == field);
                            let label = match selected_sort.map(|sort| sort.direction) {
                                Some(SortDirection::Ascending) => {
                                    format!("{} · Ascending", self.property_key_label(&field))
                                }
                                Some(SortDirection::Descending) => {
                                    format!("{} · Descending", self.property_key_label(&field))
                                }
                                None => self.property_key_label(&field),
                            };
                            Button::new(SharedString::from(format!(
                                "sort-field-{}",
                                property_key_id(&field)
                            )))
                            .label(label)
                            .ghost()
                            .small()
                            .selected(selected_sort.is_some())
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.set_sort(field.clone(), cx);
                                },
                            ))
                        }),
                    ))
                    .child(
                        div()
                            .px_4()
                            .py_2()
                            .border_t_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Manual order is preserved. Empty values stay last."),
                    ),
            )
    }
}
