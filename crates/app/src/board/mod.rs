mod action;
mod attachments;
mod drag;
mod dto;
mod due_date;
mod entry_dialog;
mod filters;
mod handlers;
mod interactions;
mod persistence;
mod properties;
mod render;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use dto::*;
use gpui::*;
use gpui_component::calendar::Date;
use gpui_component::date_picker::{DatePickerEvent, DatePickerState};
use gpui_component::input::{InputEvent, InputState};

use crate::board::entry_dialog::EntryDialog;

pub(crate) struct BoardView {
    board_id: Option<u32>,
    cards: Vec<CardDTO>,
    board_labels: Vec<BoardLabelDTO>,
    board_properties: storage::board_properties::BoardProperties,
    property_values: HashMap<(i64, i64), storage::board_properties::PropertyValue>,
    saved_views: Vec<storage::board_properties::BoardView>,
    active_view_id: Option<i64>,
    active_view_config: storage::board_properties::BoardViewConfig,
    view_config_dirty: bool,
    view_load_warnings: Vec<SharedString>,
    property_update_error: Option<SharedString>,
    property_field_errors: HashMap<(i64, i64), SharedString>,
    saving_property_values: HashSet<(i64, i64)>,
    property_panel_open: bool,
    property_form_open: bool,
    fields_panel_open: bool,
    view_panel_open: bool,
    new_view_form_open: bool,
    sort_panel_open: bool,
    new_property_kind: storage::board_properties::PropertyKind,
    new_property_input: Entity<InputState>,
    rename_property_input: Entity<InputState>,
    renaming_property_id: Option<i64>,
    new_property_option_input: Entity<InputState>,
    rename_property_option_input: Entity<InputState>,
    renaming_property_option_id: Option<i64>,
    adding_property_option_id: Option<i64>,
    property_value_input: Entity<InputState>,
    property_date_picker: Entity<DatePickerState>,
    editing_property_id: Option<i64>,
    property_select_search_input: Entity<InputState>,
    selecting_property_id: Option<i64>,
    new_view_input: Entity<InputState>,
    rename_view_input: Entity<InputState>,
    renaming_view_id: Option<i64>,
    filter_value_input: Entity<InputState>,
    editing_filter_property_id: Option<i64>,
    next_property_update_revision: u64,
    property_update_revisions: HashMap<(i64, i64), u64>,
    persisted_property_revisions: Arc<tokio::sync::Mutex<HashMap<(i64, i64), u64>>>,
    load_error: Option<SharedString>,
    is_adding_list: bool,
    is_entry_open: bool,
    entry_dialog: EntryDialog,
    new_list_input: Entity<InputState>,
    dialog_title_input: Entity<InputState>,
    dialog_description_input: Entity<InputState>,
    entry_title_input: Entity<InputState>,
    entry_description_input: Entity<InputState>,
    due_date_picker: Entity<DatePickerState>,
    new_label_input: Entity<InputState>,
    rename_label_input: Entity<InputState>,
    new_checklist_item_input: Entity<InputState>,
    rename_checklist_item_input: Entity<InputState>,
    rename_card_input: Entity<InputState>,
    renaming_card_id: Option<u32>,
    pending_card_id: Option<u32>,
    renaming_label_id: Option<u32>,
    renaming_checklist_item_id: Option<u32>,
    selected_label_color: SharedString,
    filters: filters::BoardFilters,
    filter_panel_open: bool,
    next_temporary_card_id: u32,
    next_temporary_entry_id: u32,
    next_checklist_item_position: i32,
    next_due_date_update_revision: u64,
    persisted_due_date_revisions: Arc<tokio::sync::Mutex<HashMap<u32, u64>>>,
    attachment_preview_paths: HashMap<u32, PathBuf>,
    load_generation: u64,
}

pub(crate) enum BoardViewEvent {
    LoadFinished(u32),
}

impl EventEmitter<BoardViewEvent> for BoardView {}

impl BoardView {
    pub(crate) fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let new_list_input = cx.new(|cx| InputState::new(window, cx).placeholder("List name..."));

        let dialog_title_input = cx.new(|cx| InputState::new(window, cx).placeholder("Card title"));

        let dialog_description_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Card description")
                .multi_line(true)
                .auto_grow(3, 24)
                .soft_wrap(true)
                .searchable(true)
        });

        let entry_title_input = cx.new(|cx| InputState::new(window, cx).placeholder("Card title"));

        let entry_description_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Card description")
                .multi_line(true)
                .auto_grow(4, 24)
                .soft_wrap(true)
                .searchable(true)
        });
        let due_date_picker = cx.new(|cx| DatePickerState::new(window, cx));

        let new_label_input = cx.new(|cx| InputState::new(window, cx).placeholder("Label name"));
        let rename_label_input = cx.new(|cx| InputState::new(window, cx).placeholder("Label name"));
        let new_checklist_item_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Add a checklist item"));
        let rename_checklist_item_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Checklist item"));

        let card_edit_input = cx.new(|cx| InputState::new(window, cx).placeholder("Edit title..."));
        let new_property_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Property name"));
        let rename_property_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Property name"));
        let new_property_option_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Option name"));
        let rename_property_option_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Option name"));
        let property_value_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Property value"));
        let property_date_picker = cx.new(|cx| DatePickerState::new(window, cx));
        let property_select_search_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search options"));
        let new_view_input = cx.new(|cx| InputState::new(window, cx).placeholder("View name"));
        let rename_view_input = cx.new(|cx| InputState::new(window, cx).placeholder("View name"));
        let filter_value_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter value"));

        let entry_dialog = EntryDialog::new();

        cx.subscribe(
            &new_list_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let name = text.trim();
                    if let Some(board_id) = this.board_id
                        && !name.is_empty()
                    {
                        let card_id = this.next_card_id();
                        this.is_adding_list = false;
                        this.add_card(
                            cx,
                            CardDTO {
                                id: card_id,
                                title: SharedString::from(name),
                                board_id,
                                position: this.cards.len() as i32,
                                entries: vec![],
                            },
                            card_id,
                        );
                    } else {
                        this.is_adding_list = false;
                        cx.notify();
                    }
                }
                InputEvent::Blur => {
                    this.is_adding_list = false;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe_in(
            &new_label_input,
            window,
            |this: &mut Self, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let name = input.read(cx).value().trim().to_string();
                    if !name.is_empty() {
                        this.create_board_label(name, cx);
                        input.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                        });
                    }
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &due_date_picker,
            window,
            |this: &mut Self, _, event: &DatePickerEvent, _, cx| {
                if let DatePickerEvent::Change(Date::Single(date)) = event {
                    this.update_selected_entry_due_on(
                        date.map(|date| date.format("%Y-%m-%d").to_string()),
                        cx,
                    );
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &rename_label_input,
            window,
            |this: &mut Self, input, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let name = input.read(cx).value().trim().to_string();
                    if name.is_empty() {
                        this.renaming_label_id = None;
                        cx.notify();
                    } else {
                        this.rename_board_label(name, cx);
                        input.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                        });
                    }
                }
                InputEvent::Blur => {
                    this.renaming_label_id = None;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe_in(
            &new_checklist_item_input,
            window,
            |this: &mut Self, input, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let title = input.read(cx).value().trim().to_string();
                    if !title.is_empty() {
                        this.create_checklist_item(title, cx);
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
                }
            },
        )
        .detach();

        cx.subscribe(
            &rename_checklist_item_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let title = input.read(cx).value().trim().to_string();
                    if !title.is_empty() {
                        this.rename_checklist_item(title, cx);
                    }
                }
                InputEvent::Blur => {
                    this.renaming_checklist_item_id = None;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe(
            &card_edit_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let name = text.trim();
                    if !name.is_empty() {
                        this.rename_card(cx, name);
                    } else {
                        this.renaming_card_id = None;
                        cx.notify();
                    }
                }
                InputEvent::Blur => {
                    this.renaming_card_id = None;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe_in(
            &new_property_input,
            window,
            |this: &mut Self, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let name = input.read(cx).value().trim().to_string();
                    if !name.is_empty() {
                        this.create_board_property(name, cx);
                    }
                }
            },
        )
        .detach();

        cx.subscribe_in(
            &new_property_option_input,
            window,
            |this: &mut Self, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    let name = input.read(cx).value().trim().to_string();
                    if !name.is_empty() {
                        this.create_board_property_option(name, cx);
                    }
                }
            },
        )
        .detach();

        cx.subscribe(
            &property_value_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let value = input.read(cx).value().to_string();
                    this.commit_property_value(value, cx);
                }
                InputEvent::Blur if this.editing_property_id.is_some() => {
                    let value = input.read(cx).value().to_string();
                    this.commit_property_value(value, cx);
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe_in(
            &property_date_picker,
            window,
            |this: &mut Self, _, event: &DatePickerEvent, _, cx| {
                if let DatePickerEvent::Change(Date::Single(date)) = event
                    && let (Some(entry_id), Some(property_id)) =
                        (this.entry_dialog.entry_id, this.editing_property_id)
                {
                    this.set_entry_property_value(
                        i64::from(entry_id),
                        property_id,
                        date.map(|date| {
                            storage::board_properties::PropertyValue::Date(
                                date.format("%Y-%m-%d").to_string(),
                            )
                        }),
                        cx,
                    );
                }
            },
        )
        .detach();

        cx.subscribe(
            &property_select_search_input,
            |_: &mut Self, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            },
        )
        .detach();

        cx.subscribe(
            &rename_property_input,
            |this: &mut Self, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.commit_property_rename(input.read(cx).value().to_string(), cx);
                }
            },
        )
        .detach();

        cx.subscribe(
            &rename_property_option_input,
            |this: &mut Self, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.commit_property_option_rename(input.read(cx).value().to_string(), cx);
                }
            },
        )
        .detach();

        cx.subscribe(
            &new_view_input,
            |this: &mut Self, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.create_saved_view(input.read(cx).value().to_string(), cx);
                }
            },
        )
        .detach();

        cx.subscribe(
            &rename_view_input,
            |this: &mut Self, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.commit_view_rename(input.read(cx).value().to_string(), cx);
                }
            },
        )
        .detach();

        cx.subscribe(
            &filter_value_input,
            |this: &mut Self, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.commit_custom_filter(input.read(cx).value().to_string(), cx);
                }
            },
        )
        .detach();

        Self {
            board_id: None,
            cards: vec![],
            board_labels: vec![],
            board_properties: storage::board_properties::BoardProperties::default(),
            property_values: HashMap::new(),
            saved_views: vec![],
            active_view_id: None,
            active_view_config: storage::board_properties::BoardViewConfig {
                visible_properties: vec![
                    storage::board_properties::PropertyKey::Labels,
                    storage::board_properties::PropertyKey::DueDate,
                ],
                ..Default::default()
            },
            view_config_dirty: false,
            view_load_warnings: vec![],
            property_update_error: None,
            property_field_errors: HashMap::new(),
            saving_property_values: HashSet::new(),
            property_panel_open: false,
            property_form_open: false,
            fields_panel_open: false,
            view_panel_open: false,
            new_view_form_open: false,
            sort_panel_open: false,
            new_property_kind: storage::board_properties::PropertyKind::Text,
            new_property_input,
            rename_property_input,
            renaming_property_id: None,
            new_property_option_input,
            rename_property_option_input,
            renaming_property_option_id: None,
            adding_property_option_id: None,
            property_value_input,
            property_date_picker,
            editing_property_id: None,
            property_select_search_input,
            selecting_property_id: None,
            new_view_input,
            rename_view_input,
            renaming_view_id: None,
            filter_value_input,
            editing_filter_property_id: None,
            next_property_update_revision: 0,
            property_update_revisions: HashMap::new(),
            persisted_property_revisions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            load_error: None,
            is_adding_list: false,
            is_entry_open: false,
            entry_dialog,
            new_list_input,
            dialog_title_input,
            dialog_description_input,
            entry_title_input,
            entry_description_input,
            due_date_picker,
            new_label_input,
            rename_label_input,
            new_checklist_item_input,
            rename_checklist_item_input,
            rename_card_input: card_edit_input,
            renaming_card_id: None,
            pending_card_id: None,
            renaming_label_id: None,
            renaming_checklist_item_id: None,
            selected_label_color: SharedString::from("blue"),
            filters: filters::BoardFilters::default(),
            filter_panel_open: false,
            next_temporary_card_id: 0,
            next_temporary_entry_id: 0,
            next_checklist_item_position: 0,
            next_due_date_update_revision: 0,
            persisted_due_date_revisions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            attachment_preview_paths: HashMap::new(),
            load_generation: 0,
        }
    }
}
