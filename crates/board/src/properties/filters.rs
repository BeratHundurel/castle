use super::*;

impl BoardView {
    pub(crate) fn start_custom_filter(
        &mut self,
        property_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.properties.editing_filter_property_id = Some(property_id);
        let value = self
            .filters
            .custom
            .iter()
            .find(|filter| filter.property == PropertyKey::Custom(property_id))
            .and_then(|filter| filter.operand.as_ref())
            .map(filter_operand_text)
            .unwrap_or_default();
        self.properties.filter_value_input.update(cx, |input, cx| {
            input.set_value(value, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn commit_custom_filter(&mut self, value: String, cx: &mut Context<Self>) {
        let Some(property_id) = self.properties.editing_filter_property_id else {
            return;
        };
        let Some(kind) = self
            .properties
            .data
            .definitions
            .iter()
            .find(|property| property.id == property_id)
            .map(|property| property.kind)
        else {
            return;
        };
        let value = value.trim();
        if value.is_empty() {
            self.remove_custom_filter(property_id, cx);
            return;
        }
        let operand = match kind {
            PropertyKind::Text | PropertyKind::Url => FilterOperand::Text(value.to_string()),
            PropertyKind::Number => match value.parse::<f64>() {
                Ok(value) if value.is_finite() => FilterOperand::Number(value),
                _ => {
                    self.properties.update_error = Some("Enter a finite filter number".into());
                    cx.notify();
                    return;
                }
            },
            PropertyKind::Date => {
                if NaiveDate::parse_from_str(value, "%Y-%m-%d").is_err() {
                    self.properties.update_error = Some("Use YYYY-MM-DD for the filter".into());
                    cx.notify();
                    return;
                }
                FilterOperand::Date(value.to_string())
            }
            PropertyKind::Checkbox | PropertyKind::Select => return,
        };
        let default_operator = match kind {
            PropertyKind::Text | PropertyKind::Url => FilterOperator::Contains,
            PropertyKind::Number => FilterOperator::Equals,
            PropertyKind::Date => FilterOperator::On,
            PropertyKind::Checkbox | PropertyKind::Select => FilterOperator::Equals,
        };
        if let Some(filter) = self
            .filters
            .custom
            .iter_mut()
            .find(|filter| filter.property == PropertyKey::Custom(property_id))
        {
            filter.operand = Some(operand);
        } else {
            self.filters.custom.push(ViewFilter {
                property: PropertyKey::Custom(property_id),
                operator: default_operator,
                operand: Some(operand),
            });
        }
        self.properties.editing_filter_property_id = None;
        self.mark_filters_dirty(cx);
    }

    pub(crate) fn cycle_custom_filter_operator(
        &mut self,
        property_id: i64,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = self
            .properties
            .data
            .definitions
            .iter()
            .find(|property| property.id == property_id)
            .map(|property| property.kind)
        else {
            return;
        };
        let operators: &[FilterOperator] = match kind {
            PropertyKind::Text | PropertyKind::Url => &[
                FilterOperator::Contains,
                FilterOperator::DoesNotContain,
                FilterOperator::Equals,
                FilterOperator::IsEmpty,
                FilterOperator::IsNotEmpty,
            ],
            PropertyKind::Number => &[
                FilterOperator::Equals,
                FilterOperator::GreaterThan,
                FilterOperator::LessThan,
                FilterOperator::IsEmpty,
                FilterOperator::IsNotEmpty,
            ],
            PropertyKind::Date => &[
                FilterOperator::Before,
                FilterOperator::On,
                FilterOperator::After,
                FilterOperator::IsEmpty,
                FilterOperator::IsNotEmpty,
            ],
            PropertyKind::Checkbox => &[
                FilterOperator::IsChecked,
                FilterOperator::IsUnchecked,
                FilterOperator::IsEmpty,
            ],
            PropertyKind::Select => &[
                FilterOperator::IsAnyOf,
                FilterOperator::IsNoneOf,
                FilterOperator::IsEmpty,
                FilterOperator::IsNotEmpty,
            ],
        };
        let filter = self
            .filters
            .custom
            .iter_mut()
            .find(|filter| filter.property == PropertyKey::Custom(property_id));
        if let Some(filter) = filter {
            let index = operators
                .iter()
                .position(|operator| *operator == filter.operator)
                .unwrap_or(0);
            filter.operator = operators[(index + 1) % operators.len()];
            if matches!(
                filter.operator,
                FilterOperator::IsEmpty
                    | FilterOperator::IsNotEmpty
                    | FilterOperator::IsChecked
                    | FilterOperator::IsUnchecked
            ) {
                filter.operand = None;
            } else if kind == PropertyKind::Select
                && matches!(
                    filter.operator,
                    FilterOperator::IsAnyOf | FilterOperator::IsNoneOf
                )
                && filter.operand.is_none()
            {
                filter.operand = Some(FilterOperand::OptionIds(Vec::new()));
            }
        } else {
            self.filters.custom.push(ViewFilter {
                property: PropertyKey::Custom(property_id),
                operator: operators[0],
                operand: None,
            });
        }
        self.mark_filters_dirty(cx);
    }

    pub(crate) fn toggle_select_filter_option(
        &mut self,
        property_id: i64,
        option_id: i64,
        cx: &mut Context<Self>,
    ) {
        let index = self
            .filters
            .custom
            .iter()
            .position(|filter| filter.property == PropertyKey::Custom(property_id));
        if let Some(index) = index {
            let filter = &mut self.filters.custom[index];
            let ids = match &mut filter.operand {
                Some(FilterOperand::OptionIds(ids)) => ids,
                _ => {
                    filter.operand = Some(FilterOperand::OptionIds(Vec::new()));
                    let Some(FilterOperand::OptionIds(ids)) = filter.operand.as_mut() else {
                        return;
                    };
                    ids
                }
            };
            if let Some(index) = ids.iter().position(|id| *id == option_id) {
                ids.remove(index);
            } else {
                ids.push(option_id);
            }
            if ids.is_empty() {
                self.filters.custom.remove(index);
            }
        } else {
            self.filters.custom.push(ViewFilter {
                property: PropertyKey::Custom(property_id),
                operator: FilterOperator::IsAnyOf,
                operand: Some(FilterOperand::OptionIds(vec![option_id])),
            });
        }
        self.mark_filters_dirty(cx);
    }

    pub(crate) fn set_checkbox_filter(
        &mut self,
        property_id: i64,
        checked: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        self.filters
            .custom
            .retain(|filter| filter.property != PropertyKey::Custom(property_id));
        if let Some(checked) = checked {
            self.filters.custom.push(ViewFilter {
                property: PropertyKey::Custom(property_id),
                operator: if checked {
                    FilterOperator::IsChecked
                } else {
                    FilterOperator::IsUnchecked
                },
                operand: None,
            });
        }
        self.mark_filters_dirty(cx);
    }

    pub(crate) fn set_empty_checkbox_filter(&mut self, property_id: i64, cx: &mut Context<Self>) {
        self.filters
            .custom
            .retain(|filter| filter.property != PropertyKey::Custom(property_id));
        self.filters.custom.push(ViewFilter {
            property: PropertyKey::Custom(property_id),
            operator: FilterOperator::IsEmpty,
            operand: None,
        });
        self.mark_filters_dirty(cx);
    }

    pub(crate) fn remove_custom_filter(&mut self, property_id: i64, cx: &mut Context<Self>) {
        self.filters
            .custom
            .retain(|filter| filter.property != PropertyKey::Custom(property_id));
        self.properties.editing_filter_property_id = None;
        self.mark_filters_dirty(cx);
    }

    fn mark_filters_dirty(&mut self, cx: &mut Context<Self>) {
        self.filters
            .sync_config(&mut self.properties.active_view_config);
        self.properties.view_config_dirty = true;
        self.properties.update_error = None;
        cx.notify();
    }
}
