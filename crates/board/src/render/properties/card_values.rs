use super::*;

impl BoardView {
    pub(crate) fn render_card_property_values(
        &self,
        entry: &BoardCardDTO,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = Vec::new();
        for key in &self.properties.active_view_config.visible_properties {
            if key == &storage::board::properties::PropertyKey::RelatedNotes {
                if !entry.related_notes.is_empty() {
                    rows.push(
                        h_flex()
                            .gap_1()
                            .text_xs()
                            .text_color(cx.theme().primary_foreground.opacity(0.76))
                            .child(Icon::new(IconName::File).xsmall())
                            .child(entry.related_notes.len().to_string())
                            .into_any_element(),
                    );
                }
                continue;
            }
            let storage::board::properties::PropertyKey::Custom(property_id) = key else {
                continue;
            };
            let Some(property) = self
                .properties
                .data
                .definitions
                .iter()
                .find(|property| property.id == *property_id)
            else {
                continue;
            };
            let Some(value) = self
                .properties
                .values
                .get(&(i64::from(entry.id), *property_id))
            else {
                continue;
            };
            rows.push(self.render_card_property_value(property, value, cx));
        }
        v_flex()
            .w_full()
            .min_w_0()
            .gap_1()
            .children(rows)
            .into_any_element()
    }

    fn render_card_property_value(
        &self,
        property: &PropertyDefinition,
        value: &PropertyValue,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let muted = cx.theme().primary_foreground.opacity(0.68);
        match value {
            PropertyValue::Select(option_id) => {
                let option = property
                    .options
                    .iter()
                    .find(|option| option.id == *option_id);
                h_flex()
                    .min_w_0()
                    .when_some(option, |this, option| {
                        this.gap_1p5()
                            .child(
                                div()
                                    .size(px(6.))
                                    .rounded_full()
                                    .bg(self.label_marker_color(&option.color, cx)),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(option.name.clone()),
                            )
                    })
                    .into_any_element()
            }
            PropertyValue::Checkbox(checked) => h_flex()
                .min_w_0()
                .gap_1p5()
                .text_xs()
                .text_color(muted)
                .child(
                    Icon::new(if *checked {
                        IconName::CircleCheck
                    } else {
                        IconName::CircleX
                    })
                    .xsmall(),
                )
                .child(div().truncate().child(property.name.clone()))
                .into_any_element(),
            PropertyValue::Date(value) => {
                let label = NaiveDate::parse_from_str(value, "%Y-%m-%d")
                    .map(|date| date.format("%b %-d, %Y").to_string())
                    .unwrap_or_else(|_| value.clone());
                self.render_card_date_pill(label, cx.theme().secondary, cx)
                    .into_any_element()
            }
            PropertyValue::Url(value) => {
                let url = value.clone();
                h_flex()
                    .id(SharedString::from(format!(
                        "card-property-url-{}",
                        property.id
                    )))
                    .min_w_0()
                    .gap_1p5()
                    .cursor_pointer()
                    .text_xs()
                    .text_color(cx.theme().info)
                    .child(Icon::new(IconName::ExternalLink).xsmall())
                    .child(div().truncate().child(display_url(value)))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        cx.open_url(&url);
                    })
                    .into_any_element()
            }
            PropertyValue::Text(value) => property_card_text_row(&property.name, value, muted),
            PropertyValue::Number(value) => {
                property_card_text_row(&property.name, &value.to_string(), muted)
            }
        }
    }
}
