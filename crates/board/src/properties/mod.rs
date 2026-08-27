use chrono::NaiveDate;
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, WindowExt as _,
    button::{Button, ButtonVariant, ButtonVariants},
    calendar::Date,
    checkbox::Checkbox,
    date_picker::DatePicker,
    dialog::DialogButtonProps,
    h_flex,
    input::Input,
    menu::DropdownMenu as _,
    popover::Popover,
    scroll::ScrollableElement as _,
    v_flex,
};
use storage::board::properties::{
    BoardViewConfig, FilterOperand, FilterOperator, PropertyKey, PropertyKind, PropertyValue,
    SortDirection, ViewFilter, ViewSort,
};

use runtime::AppRuntime;

use super::{BoardView, action::*, model::BoardCardState};

mod definitions;
mod editing;
mod filters;
mod options;
mod values;
mod view;
mod views;

const OPTION_COLORS: [&str; 6] = ["blue", "green", "amber", "red", "purple", "slate"];

impl BoardView {
    fn set_property_task_error(&mut self, error: tokio::task::JoinError, _cx: &mut Context<Self>) {
        self.properties.update_error = Some(format!("Property task failed: {error}").into());
    }
}

fn property_value_text(value: &PropertyValue) -> String {
    match value {
        PropertyValue::Text(value) | PropertyValue::Date(value) | PropertyValue::Url(value) => {
            value.clone()
        }
        PropertyValue::Number(value) => value.to_string(),
        PropertyValue::Checkbox(_) | PropertyValue::Select(_) => String::new(),
    }
}

fn filter_operand_text(value: &FilterOperand) -> String {
    match value {
        FilterOperand::Text(value) | FilterOperand::Date(value) => value.clone(),
        FilterOperand::Number(value) => value.to_string(),
        FilterOperand::OptionIds(_)
        | FilterOperand::LabelIds(_)
        | FilterOperand::DueDatePresets(_) => String::new(),
    }
}

fn is_supported_url(value: &str) -> bool {
    value.starts_with("https://") || value.starts_with("http://")
}

fn config_references_property(config: &BoardViewConfig, property_id: i64) -> bool {
    let key = PropertyKey::Custom(property_id);
    config.visible_properties.contains(&key)
        || config.filters.iter().any(|filter| filter.property == key)
        || config
            .sort
            .as_ref()
            .is_some_and(|sort| sort.property == key)
}

fn remove_property_from_config(config: &mut BoardViewConfig, property_id: i64) {
    let key = PropertyKey::Custom(property_id);
    config
        .visible_properties
        .retain(|property| property != &key);
    config.filters.retain(|filter| filter.property != key);
    if config
        .sort
        .as_ref()
        .is_some_and(|sort| sort.property == key)
    {
        config.sort = None;
    }
}

fn remove_option_from_config(config: &mut BoardViewConfig, option_id: i64) {
    config.filters.retain_mut(|filter| {
        let Some(FilterOperand::OptionIds(ids)) = filter.operand.as_mut() else {
            return true;
        };
        ids.retain(|id| *id != option_id);
        !ids.is_empty()
    });
}
