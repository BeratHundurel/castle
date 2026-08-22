use super::*;

impl BoardView {
    pub(crate) fn render_fields_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut fields = vec![
            PropertyKey::Labels,
            PropertyKey::DueDate,
            PropertyKey::RelatedNotes,
        ];
        fields.extend(
            self.properties
                .data
                .definitions
                .iter()
                .map(|property| PropertyKey::Custom(property.id)),
        );

        let selected = self
            .properties
            .active_view_config
            .visible_properties
            .clone();

        Popover::new("board-fields-picker")
            .anchor(Anchor::TopRight)
            .open(self.properties.fields_panel_open)
            .on_open_change(cx.listener(|this, open, _, cx| {
                this.set_fields_panel_open(*open, cx);
            }))
            .p_0()
            .w(px(320.))
            .trigger(
                Button::new("toggle-board-fields")
                    .icon(IconName::LayoutDashboard)
                    .label("Fields")
                    .ghost()
                    .small()
                    .selected(self.properties.fields_panel_open)
                    .tooltip("Choose up to three fields shown on cards"),
            )
            .child(
                v_flex()
                    .text_sm()
                    .child(
                        v_flex()
                            .gap_1()
                            .px_4()
                            .py_3()
                            .border_b_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child("Card fields"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Show up to three ordered fields on every card in this view."),
                            ),
                    )
                    .child(
                        v_flex().p_2().children(fields.into_iter().map(|field| {
                            let checked = selected.contains(&field);
                            let field_for_toggle = field.clone();
                            let field_for_up = field.clone();
                            let field_for_down = field.clone();
                            let index = selected.iter().position(|candidate| candidate == &field);
                            h_flex()
                                .min_h_8()
                                .gap_2()
                                .child(
                                    Checkbox::new(SharedString::from(format!("visible-field-{}", property_key_id(&field))))
                                        .checked(checked)
                                        .small()
                                        .label(self.property_key_label(&field))
                                        .flex_1()
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.toggle_visible_property(field_for_toggle.clone(), cx);
                                        })),
                                )
                                .when_some(index, |this, index| {
                                    this.child(
                                        Button::new(SharedString::from(format!("field-up-{}", property_key_id(&field))))
                                            .icon(IconName::ArrowUp)
                                            .ghost()
                                            .xsmall()
                                            .disabled(index == 0)
                                            .tooltip("Move field up")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.move_visible_property(&field_for_up, -1, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new(SharedString::from(format!("field-down-{}", property_key_id(&field))))
                                            .icon(IconName::ArrowDown)
                                            .ghost()
                                            .xsmall()
                                            .disabled(index + 1 >= selected.len())
                                            .tooltip("Move field down")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.move_visible_property(&field_for_down, 1, cx);
                                            })),
                                    )
                                })
                        })),
                    )
                    .child(
                        div()
                            .p_2()
                            .border_t_1()
                            .border_color(cx.theme().border.opacity(0.72))
                            .child(
                                Checkbox::new("compact-board-cards")
                                    .checked(self.properties.active_view_config.compact_cards)
                                    .small()
                                    .label("Compact cards")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.toggle_compact_cards(cx);
                                    })),
                            ),
                    ),
            )
    }
}
