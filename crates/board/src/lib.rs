mod action;
mod attachments;
mod color_contrast;
mod drag;
mod due_date;
mod entry_dialog;
mod filters;
mod interactions;
mod model;
mod notifications;
mod persistence;
mod properties;
mod related_notes;
mod render;
mod state;
mod template_picker;
mod templates;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use gpui::*;
use gpui_component::calendar::Date;
use gpui_component::date_picker::{DatePickerEvent, DatePickerState};
use gpui_component::input::{EditorState, InputEvent, InputState, TextareaState};
use model::*;
use state::*;

pub use notifications::{NotificationAvailability, NotificationGateway};
pub use template_picker::{BoardTemplatePicker, BoardTemplatePickerEvent};

use entry_dialog::EntryDialog;
use runtime::AppRuntime;

#[derive(Clone)]
struct BoardServices {
    layout_persistence: storage::board::positions::BoardLayoutPersistence,
    notifications: notifications::BoardNotifications,
}

impl BoardServices {
    fn new(runtime: tokio::runtime::Handle) -> Self {
        Self::with_notifications(runtime, notifications::BoardNotifications::unavailable())
    }

    fn with_notifications(
        runtime: tokio::runtime::Handle,
        notifications: notifications::BoardNotifications,
    ) -> Self {
        Self {
            layout_persistence: storage::board::positions::BoardLayoutPersistence::new(runtime),
            notifications,
        }
    }

    fn layout_persistence(&self) -> storage::board::positions::BoardLayoutPersistence {
        self.layout_persistence.clone()
    }

    fn notifications(&self) -> notifications::BoardNotifications {
        self.notifications.clone()
    }
}

impl Global for BoardServices {}

pub fn init(cx: &mut App) {
    if !cx.has_global::<BoardServices>() {
        let runtime = cx.global::<AppRuntime>().tokio_handle();
        cx.set_global(BoardServices::new(runtime));
    }
}

pub fn init_with_notification_gateway(cx: &mut App, gateway: Arc<dyn NotificationGateway>) {
    if !cx.has_global::<BoardServices>() {
        let runtime = cx.global::<AppRuntime>().tokio_handle();
        cx.set_global(BoardServices::with_notifications(
            runtime,
            notifications::BoardNotifications::new(gateway),
        ));
    }
}

pub struct BoardView {
    focus_handle: FocusHandle,
    data: BoardDataState,
    properties: BoardPropertiesState,
    related_notes: RelatedNotesState,
    mutation: BoardMutationState,
    entry_editing: EntryEditingState,
    filters: filters::BoardFilters,
    filter_panel_open: bool,
    board_scroll_handle: ScrollHandle,
    filter_scroll_handle: ScrollHandle,
    pending_reveal_target: Option<workspace::WorkspaceNavigationTarget>,
    revealed_list_id: Option<u32>,
}

#[derive(Clone, Debug)]
pub enum BoardViewEvent {
    LoadFinished(u32),
    OpenNote(u32),
    OpenWorkspaceTarget(workspace::WorkspaceNavigationTarget),
    NavigationUnavailable(String),
    DataCommitted {
        board_id: u32,
        links_changed: bool,
    },
    CreateLinkedNote {
        item: storage::workspace::links::WorkspaceItemRef,
        project_id: Option<u32>,
        title: String,
    },
}

impl EventEmitter<BoardViewEvent> for BoardView {}

fn description_editor(window: &mut Window, cx: &mut Context<EditorState>) -> EditorState {
    EditorState::new(window, cx)
        .language("text")
        .line_number(false)
        .folding(false)
        .indent_guides(false)
        .placeholder("Card description")
        .soft_wrap(true)
        .searchable(true)
}

fn dialog_description_input(window: &mut Window, cx: &mut Context<TextareaState>) -> TextareaState {
    TextareaState::new(window, cx)
        .placeholder("Card description")
        .soft_wrap(true)
        .searchable(true)
}

impl Focusable for BoardView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl BoardView {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        init(cx);
        let entry_wikilink_completion_provider =
            workspace::WorkspaceReferenceCompletionProvider::new(-1);
        let entry_completion_provider =
            std::rc::Rc::new(entry_wikilink_completion_provider.clone());
        let new_list_input = cx.new(|cx| InputState::new(window, cx).placeholder("List name..."));

        let dialog_title_input = cx.new(|cx| InputState::new(window, cx).placeholder("Card title"));

        let dialog_description_input = cx.new(|cx| dialog_description_input(window, cx));

        let entry_title_input = cx.new(|cx| InputState::new(window, cx).placeholder("Card title"));

        let entry_description_input = cx.new(|cx| {
            let mut input = description_editor(window, cx);
            input.lsp_mut().completion_provider = Some(entry_completion_provider);
            input
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
        let related_note_picker = related_notes::RelatedNotePickerState::new(window, cx);
        let related_note_search_input = related_note_picker.search_input.clone();

        cx.subscribe(
            &related_note_search_input,
            |this: &mut Self, _, event: &InputEvent, cx| match event {
                InputEvent::Change => {
                    this.related_notes.picker.active_row = 0;
                    this.related_notes.picker.keyboard_selection_visible = false;
                    this.related_notes
                        .picker
                        .scroll_handle
                        .set_offset(point(px(0.), px(0.)));
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.activate_related_note_candidate(cx),
                _ => {}
            },
        )
        .detach();

        let entry_dialog = EntryDialog::new();

        cx.subscribe(
            &new_list_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let name = text.trim();
                    if let Some(board_id) = this.data.board_id
                        && !name.is_empty()
                    {
                        let card_id = this.next_card_id();
                        this.entry_editing.adding_list = false;
                        this.add_card(
                            cx,
                            BoardListState {
                                id: card_id,
                                title: SharedString::from(name),
                                board_id,
                                position: this.data.lists.len() as i32,
                                entries: vec![],
                            },
                            card_id,
                        );
                    } else {
                        this.entry_editing.adding_list = false;
                        cx.notify();
                    }
                }
                InputEvent::Blur => {
                    this.entry_editing.adding_list = false;
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
                        this.entry_editing.renaming_label_id = None;
                        cx.notify();
                    } else {
                        this.rename_board_label(name, cx);
                        input.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                        });
                    }
                }
                InputEvent::Blur => {
                    this.entry_editing.renaming_label_id = None;
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
                    this.entry_editing.renaming_checklist_item_id = None;
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
                        this.entry_editing.renaming_list_id = None;
                        cx.notify();
                    }
                }
                InputEvent::Blur => {
                    this.entry_editing.renaming_list_id = None;
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
                InputEvent::Blur if this.properties.editing_property_id.is_some() => {
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
                    && let (Some(entry_id), Some(property_id)) = (
                        this.entry_editing.dialog.entry_id,
                        this.properties.editing_property_id,
                    )
                {
                    this.set_entry_property_value(
                        i64::from(entry_id),
                        property_id,
                        date.map(|date| {
                            storage::board::properties::PropertyValue::Date(
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
            focus_handle: cx.focus_handle(),
            data: BoardDataState {
                board_id: None,
                lists: vec![],
                labels: vec![],
            },
            properties: BoardPropertiesState {
                data: storage::board::properties::BoardProperties::default(),
                values: HashMap::new(),
                saved_views: vec![],
                active_view_id: None,
                active_view_config: storage::board::properties::BoardViewConfig {
                    visible_properties: vec![
                        storage::board::properties::PropertyKey::Labels,
                        storage::board::properties::PropertyKey::DueDate,
                    ],
                    ..Default::default()
                },
                view_config_dirty: false,
                view_load_warnings: vec![],
                update_error: None,
                field_errors: HashMap::new(),
                saving_values: HashSet::new(),
                property_panel_open: false,
                property_form_open: false,
                fields_panel_open: false,
                view_panel_open: false,
                new_view_form_open: false,
                sort_panel_open: false,
                new_property_kind: storage::board::properties::PropertyKind::Text,
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
                next_update_revision: 0,
                update_revisions: HashMap::new(),
                persisted_revisions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            },
            related_notes: RelatedNotesState {
                picker: related_note_picker,
                by_item: HashMap::new(),
                reference_catalog: Arc::new(Default::default()),
                catalog: Arc::new(Vec::new()),
                completion_provider: entry_wikilink_completion_provider,
                error: None,
            },
            mutation: BoardMutationState {
                load_error: None,
                mutation_error: None,
                load_request: workspace::RequestTracker::default(),
                layout_commit_task: None,
                loaded_generation: None,
                local_generation: 0,
            },
            entry_editing: EntryEditingState {
                adding_list: false,
                open: false,
                dialog: entry_dialog,
                new_list_input,
                dialog_title_input,
                dialog_description_input,
                title_input: entry_title_input,
                description_input: entry_description_input,
                due_date_picker,
                new_label_input,
                rename_label_input,
                new_checklist_item_input,
                rename_checklist_item_input,
                rename_list_input: card_edit_input,
                renaming_list_id: None,
                pending_list_id: None,
                renaming_label_id: None,
                renaming_checklist_item_id: None,
                selected_label_color: SharedString::from("blue"),
                next_temporary_list_id: 0,
                next_temporary_card_id: 0,
                next_checklist_item_position: 0,
                next_due_date_update_revision: 0,
                persisted_due_date_revisions: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
                attachment_preview_paths: HashMap::new(),
            },
            filters: filters::BoardFilters::default(),
            filter_panel_open: false,
            board_scroll_handle: ScrollHandle::new(),
            filter_scroll_handle: ScrollHandle::new(),
            pending_reveal_target: None,
            revealed_list_id: None,
        }
    }

    pub fn queue_reveal_target(
        &mut self,
        target: workspace::WorkspaceNavigationTarget,
        cx: &mut Context<Self>,
    ) {
        self.pending_reveal_target = Some(target);
        cx.notify();
    }

    pub fn apply_pending_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(target) = self.pending_reveal_target else {
            return true;
        };
        let workspace::WorkspaceNavigationTarget::Board {
            board_id,
            list_id,
            card_id,
        } = target
        else {
            return false;
        };
        if self.data.board_id != Some(board_id) {
            return false;
        }
        if list_id.is_none() && card_id.is_none() {
            self.pending_reveal_target = None;
            return true;
        }
        if self.mutation.loaded_generation != Some(self.mutation.load_request.generation()) {
            return false;
        }
        if self.data.lists.is_empty() {
            self.pending_reveal_target = None;
            cx.emit(BoardViewEvent::NavigationUnavailable(
                "The linked list or card is no longer available.".to_string(),
            ));
            return false;
        }

        let resolved_list_id = card_id
            .and_then(|card_id| {
                self.data
                    .lists
                    .iter()
                    .find(|list| list.entries.iter().any(|card| card.id == card_id))
                    .map(|list| list.id)
            })
            .or(list_id);
        let Some(resolved_list_id) =
            resolved_list_id.or_else(|| self.data.lists.first().map(|list| list.id))
        else {
            return false;
        };
        let Some(list_index) = self
            .data
            .lists
            .iter()
            .position(|list| list.id == resolved_list_id)
        else {
            self.pending_reveal_target = None;
            cx.emit(BoardViewEvent::NavigationUnavailable(
                "The linked list is no longer available.".to_string(),
            ));
            return false;
        };
        if let Some(card_id) = card_id {
            if !self.data.lists[list_index]
                .entries
                .iter()
                .any(|card| card.id == card_id)
            {
                self.pending_reveal_target = None;
                cx.emit(BoardViewEvent::NavigationUnavailable(
                    "The linked card is no longer available.".to_string(),
                ));
                return false;
            }
            self.open_entry_dialog(card_id, window, cx);
        }
        self.board_scroll_handle.scroll_to_item(list_index);
        self.revealed_list_id = Some(resolved_list_id);
        self.pending_reveal_target = None;
        let generation = self.mutation.load_request.generation();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1_200))
                .await;
            this.update(cx, |this, cx| {
                if this.mutation.load_request.generation() == generation {
                    this.revealed_list_id = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
        true
    }
}
