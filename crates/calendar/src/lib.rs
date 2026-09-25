mod date;
mod event;
mod recurrence;

use chrono::{Datelike, Duration, Local, NaiveDate};
pub use date::{CalendarDate, CalendarDateError};
pub use event::{CalendarEvent, CalendarEventError};
use gpui_kit::assets::IconName as AssetIconName;
use gpui_kit::base::Selectable as _;
use gpui_kit::component::{
    ActiveTheme, Disableable as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    calendar::Date,
    date_picker::{DatePickerEvent, DatePickerState},
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement as _,
    searchable_list::{SearchableListItem, SearchableVec},
    select::{Select, SelectState},
    v_flex,
};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FontWeight, InteractiveElement as _,
    IntoElement as _, ParentElement as _, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, prelude::FluentBuilder as _, px,
};
use gpui_kit::{EventEmitter, FocusHandle, Focusable};
pub use recurrence::{
    GenerationMode, RecurrenceError, RecurrenceFrequency, RecurrenceOccurrence, RecurrenceRule,
    RecurrenceRuleError, RecurrenceSeries, Weekday, expand_series, project_occurrences,
};
use std::{collections::HashMap, sync::Arc};
mod pages;
mod render;
pub use pages::CalendarPage;

pub type CalendarTask<T> = gpui_kit::Task<Result<anyhow::Result<T>, tokio::task::JoinError>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryLifecycleState {
    Open,
    Completed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarListRole {
    Neutral,
    Done,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarListEntry {
    pub id: u32,
    pub title: String,
    pub due_on: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarListRecord {
    pub id: u32,
    pub title: String,
    pub entries: Vec<CalendarListEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarEntryRecord {
    pub entry_id: i64,
    pub title: String,
    pub description: String,
    pub start_on: Option<String>,
    pub due_on: String,
    pub list_id: i64,
    pub list_title: String,
    pub board_id: i64,
    pub board_title: String,
    pub workflow_role: CalendarListRole,
    pub completed_at: Option<i64>,
    pub cancelled_at: Option<i64>,
    pub recurrence_series_id: Option<i64>,
    pub occurrence_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RecurringTaskRecord {
    pub id: i64,
    pub entry_id: i64,
    pub start_on: CalendarDate,
    pub rule: RecurrenceRule,
    pub next_on: CalendarDate,
    pub until_on: Option<CalendarDate>,
    pub occurrence_limit: Option<u32>,
    pub generation_mode: GenerationMode,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CalendarSnapshot {
    pub entries: Vec<CalendarEntryRecord>,
    pub reminders: Vec<CalendarReminderRecord>,
    pub recurring: Vec<RecurringTaskRecord>,
    pub lists: Vec<CalendarListRecord>,
    pub board_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarReminderRecord {
    pub id: i64,
    pub title: String,
    pub date: String,
}

#[derive(Clone, Debug)]
pub struct SaveCalendarReminder {
    pub id: Option<i64>,
    pub title: String,
    pub date: String,
}

#[derive(Clone, Debug)]
pub struct SaveCalendarEntry {
    pub entry_id: i64,
    pub title: String,
    pub due_on: Option<String>,
    pub status: EntryLifecycleState,
}

#[derive(Clone, Debug)]
pub struct CreateCalendarRecurrence {
    pub entry_id: i64,
    pub start_on: String,
    pub rule: String,
    pub until_on: Option<String>,
    pub generation_mode: GenerationMode,
}

pub trait CalendarService: Send + Sync {
    fn load(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        board_id: Option<u32>,
        month: NaiveDate,
    ) -> CalendarTask<CalendarSnapshot>;

    fn save_entry(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        request: SaveCalendarEntry,
    ) -> CalendarTask<()>;

    fn create_recurring_task(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        request: CreateCalendarRecurrence,
    ) -> CalendarTask<()>;

    fn delete_recurring_task(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        entry_id: i64,
    ) -> CalendarTask<()>;

    fn reschedule_entry(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        entry_id: i64,
        due_on: String,
    ) -> CalendarTask<()>;

    fn save_reminder(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        request: SaveCalendarReminder,
    ) -> CalendarTask<CalendarReminderRecord>;

    fn delete_reminder(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        reminder_id: i64,
    ) -> CalendarTask<()>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CalendarEntryOption {
    entry_id: i64,
    entry_title: SharedString,
    list_title: SharedString,
}

impl SearchableListItem for CalendarEntryOption {
    type Value = i64;

    fn title(&self) -> SharedString {
        if self.list_title.is_empty() {
            self.entry_title.clone()
        } else {
            format!("{} · {}", self.entry_title, self.list_title).into()
        }
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> impl gpui_kit::IntoElement {
        h_flex()
            .debug_selector(|| format!("calendar-entry-picker-option-{}", self.entry_id))
            .w_full()
            .min_w_0()
            .gap_2()
            .child(div().flex_1().min_w_0().child(self.entry_title.clone()))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.list_title.clone()),
            )
    }

    fn value(&self) -> &Self::Value {
        &self.entry_id
    }
}

type CalendarEntrySelectState = SelectState<SearchableVec<CalendarEntryOption>>;

pub(crate) struct CalendarPanelState {
    pub(crate) loading: bool,
    pub(crate) creating: bool,
    pub(crate) error: Option<SharedString>,
    pub(crate) recurrence_error: Option<SharedString>,
    pub(crate) month: NaiveDate,
    pub(crate) entries: Vec<CalendarEntryRecord>,
    pub(crate) reminders: Vec<CalendarReminderRecord>,
    pub(crate) recurring: Vec<RecurringTaskRecord>,
    recurrence_entry_select: Entity<CalendarEntrySelectState>,
    pub(crate) recurrence_start_picker: Entity<DatePickerState>,
    pub(crate) recurrence_rule_input: Entity<InputState>,
    pub(crate) recurrence_until_picker: Entity<DatePickerState>,
    pub(crate) recurrence_on_schedule: bool,
    pub(crate) recurrence_form_open: bool,
    pub(crate) selected_date: Option<NaiveDate>,
    pub(crate) selected_entry_id: Option<i64>,
    pub(crate) selected_reminder_id: Option<i64>,
    pub(crate) new_reminder_date: Option<String>,
    pub(crate) saving_reminder: bool,
    pub(crate) reminder_error: Option<SharedString>,
    pub(crate) detail_title_input: Entity<InputState>,
    pub(crate) detail_due_picker: Entity<DatePickerState>,
}

#[derive(Clone)]
struct CalendarEntryDrag {
    entry_id: i64,
    title: SharedString,
}

impl gpui_kit::Render for CalendarEntryDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl gpui_kit::IntoElement {
        let theme = cx.theme();
        div()
            .px_2()
            .py_1()
            .rounded_sm()
            .border_1()
            .border_color(theme.drag_border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .text_xs()
            .shadow_sm()
            .child(self.title.clone())
    }
}

impl CalendarPanelState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<CalendarWorkspace>) -> Self {
        let today = Local::now().date_naive();
        let month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
        Self {
            loading: false,
            creating: false,
            error: None,
            recurrence_error: None,
            month,
            entries: Vec::new(),
            reminders: Vec::new(),
            recurring: Vec::new(),
            recurrence_entry_select: cx.new(|cx| {
                SelectState::new(SearchableVec::new(Vec::new()), None, window, cx).searchable(true)
            }),
            recurrence_start_picker: cx
                .new(|cx| DatePickerState::new(window, cx).date_format("%Y-%m-%d")),
            recurrence_rule_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("daily / every 2 weeks on mon, wed")
            }),
            recurrence_until_picker: cx
                .new(|cx| DatePickerState::new(window, cx).date_format("%Y-%m-%d")),
            recurrence_on_schedule: false,
            recurrence_form_open: false,
            selected_date: None,
            selected_entry_id: None,
            selected_reminder_id: None,
            new_reminder_date: None,
            saving_reminder: false,
            reminder_error: None,
            detail_title_input: cx.new(|cx| InputState::new(window, cx).placeholder("Title")),
            detail_due_picker: cx
                .new(|cx| DatePickerState::new(window, cx).date_format("%Y-%m-%d")),
        }
    }
}

impl CalendarWorkspace {
    fn calendar_entry_options(&self) -> Vec<CalendarEntryOption> {
        self.lists
            .iter()
            .flat_map(|list| {
                list.entries.iter().map(|entry| CalendarEntryOption {
                    entry_id: i64::from(entry.id),
                    entry_title: entry.title.clone().into(),
                    list_title: list.title.clone().into(),
                })
            })
            .collect()
    }

    fn calendar_entry_label(&self, entry_id: i64) -> String {
        self.lists
            .iter()
            .flat_map(|list| {
                list.entries
                    .iter()
                    .map(move |entry| (entry, list.title.as_str()))
            })
            .find(|(entry, _)| i64::from(entry.id) == entry_id)
            .map(|(entry, list_title)| format!("{} · {}", entry.title, list_title))
            .unwrap_or_else(|| format!("Entry {entry_id}"))
    }

    fn sync_recurrence_entry_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let options = self.calendar_entry_options();
        let selected_entry_id = self
            .state
            .recurrence_entry_select
            .read(cx)
            .selected_value()
            .copied()
            .filter(|entry_id| options.iter().any(|option| option.entry_id == *entry_id));
        let picker = self.state.recurrence_entry_select.clone();
        picker.update(cx, |picker, cx| {
            picker.set_items(SearchableVec::new(options), window, cx);
            if let Some(entry_id) = selected_entry_id {
                picker.set_selected_value(&entry_id, window, cx);
            } else {
                picker.set_selected_index(None, window, cx);
            }
        });
    }

    fn clear_recurrence_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.recurrence_error = None;
        self.state
            .recurrence_entry_select
            .update(cx, |picker, cx| picker.set_selected_index(None, window, cx));
        self.state.recurrence_start_picker.update(cx, |picker, cx| {
            picker.set_date(Date::Single(None), window, cx)
        });
        self.state
            .recurrence_rule_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.state.recurrence_until_picker.update(cx, |picker, cx| {
            picker.set_date(Date::Single(None), window, cx)
        });
        self.state.recurrence_form_open = false;
        cx.notify();
    }

    fn open_recurrence_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.recurrence_error = None;
        self.state.recurrence_form_open = true;
        self.sync_recurrence_entry_picker(window, cx);
        cx.notify();
    }

    pub fn open_calendar_entry(
        &mut self,
        entry_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .state
            .entries
            .iter()
            .find(|entry| entry.entry_id == entry_id)
            .cloned()
        else {
            return;
        };
        if let std::collections::hash_map::Entry::Vacant(draft_entry) = self.drafts.entry(entry_id)
        {
            let title = cx.new(|cx| InputState::new(window, cx).default_value(entry.title.clone()));
            let due_picker = cx.new(|cx| DatePickerState::new(window, cx).date_format("%Y-%m-%d"));
            due_picker.update(cx, |picker, cx| {
                picker.set_date(
                    Date::Single(parse_calendar_date(Some(&entry.due_on))),
                    window,
                    cx,
                );
            });
            cx.observe(&title, |_, _, cx| cx.notify()).detach();
            cx.subscribe_in(
                &due_picker,
                window,
                move |this, _, event: &DatePickerEvent, _, cx| {
                    if matches!(event, DatePickerEvent::Change(Date::Single(_))) {
                        if let Some(draft) = this.drafts.get_mut(&entry_id) {
                            draft.error = None;
                        }
                        cx.notify();
                    }
                },
            )
            .detach();
            let status = calendar_entry_lifecycle(&entry);
            draft_entry.insert(EntryDraft {
                title,
                due_picker,
                baseline: (entry.title.clone(), entry.due_on.clone()),
                status,
                baseline_status: status,
                entry,
                saving: false,
                error: None,
            });
        }
        let Some(draft) = self.drafts.get(&entry_id) else {
            return;
        };
        self.state.selected_reminder_id = None;
        self.state.new_reminder_date = None;
        self.state.selected_date = NaiveDate::parse_from_str(&draft.entry.due_on, "%Y-%m-%d").ok();
        self.state.selected_entry_id = Some(entry_id);
        self.state.detail_title_input = draft.title.clone();
        self.state.detail_due_picker = draft.due_picker.clone();
        if self.route_narrow(CalendarRoute::Month) {
            cx.emit(CalendarWorkspaceEvent::Navigate(CalendarRoute::Item));
        }
        cx.notify();
    }

    pub(crate) fn close_calendar_entry(&mut self, cx: &mut Context<Self>) {
        self.state.selected_entry_id = None;
        self.state.selected_reminder_id = None;
        self.state.new_reminder_date = None;
        cx.notify();
    }

    pub(crate) fn select_calendar_day(
        &mut self,
        date: NaiveDate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let month = date.with_day(1).unwrap_or(date);
        let month_changed = self.state.month != month;
        self.state.selected_date = Some(date);
        self.state.selected_entry_id = None;
        self.state.selected_reminder_id = None;
        self.state.new_reminder_date = None;
        self.state.reminder_error = None;
        if month_changed {
            self.state.month = month;
            self.load_calendar(self.board_id, window, cx);
        }
        if self.route_narrow(CalendarRoute::Month) {
            cx.emit(CalendarWorkspaceEvent::Navigate(CalendarRoute::Item));
        }
        cx.notify();
    }

    fn open_recurrence_for_entry(
        &mut self,
        entry_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .lists
            .iter()
            .flat_map(|list| list.entries.iter())
            .find(|entry| i64::from(entry.id) == entry_id)
            .cloned()
        else {
            return;
        };
        self.state.recurrence_error = None;
        self.state.recurrence_form_open = true;
        self.sync_recurrence_entry_picker(window, cx);
        let picker = self.state.recurrence_entry_select.clone();
        picker.update(cx, |picker, cx| {
            picker.set_selected_value(&entry_id, window, cx);
        });
        let start_on = entry.due_on.unwrap_or_default();
        self.state.recurrence_start_picker.update(cx, |picker, cx| {
            picker.set_date(
                Date::Single(parse_calendar_date(Some(&start_on))),
                window,
                cx,
            );
        });
        cx.emit(CalendarWorkspaceEvent::Navigate(CalendarRoute::Recurring));
        cx.notify();
    }

    fn selected_calendar_entry(&self) -> Option<&CalendarEntryRecord> {
        self.state
            .selected_entry_id
            .and_then(|id| self.drafts.get(&id).map(|draft| &draft.entry))
    }

    pub(crate) fn save_calendar_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry_id) = self.state.selected_entry_id else {
            return;
        };
        let Some(draft) = self.drafts.get_mut(&entry_id) else {
            return;
        };
        if draft.saving {
            return;
        }
        let title = draft.title.read(cx).value().trim().to_string();
        let due = date_picker_value(draft.due_picker.read(cx));
        if let Err(error) = validate_calendar_entry_draft(&title, &due) {
            draft.error = Some(error.into());
            cx.notify();
            return;
        }
        draft.saving = true;
        draft.error = None;
        let saved = (title.clone(), due.clone());
        let status = draft.status;
        let owning_board_id = u32::try_from(draft.entry.board_id).ok();
        let request = SaveCalendarEntry {
            entry_id,
            title,
            due_on: (!due.is_empty()).then_some(due.clone()),
            status,
        };
        let task = self
            .service
            .save_entry(cx.background_executor().clone(), request);
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if let Some(draft) = this.drafts.get_mut(&entry_id) {
                    let saved_due_on = saved.1.clone();
                    draft.saving = false;
                    match result {
                        Ok(Ok(())) => {
                            draft.baseline = saved.clone();
                            draft.baseline_status = status;
                            draft.entry.title = saved.0;
                            draft.entry.due_on = saved.1;
                        }
                        Ok(Err(error)) => draft.error = Some(error.to_string().into()),
                        Err(error) => draft.error = Some(error.to_string().into()),
                    }
                    if draft.error.is_none() {
                        if let Ok(date) = NaiveDate::parse_from_str(&saved_due_on, "%Y-%m-%d") {
                            this.state.selected_date = Some(date);
                            this.state.month = date.with_day(1).unwrap_or(date);
                        }
                        if let Some(board_id) = owning_board_id {
                            cx.emit(CalendarWorkspaceEvent::Committed(board_id));
                        }
                        this.load_calendar(this.board_id, window, cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn set_calendar_entry_lifecycle(
        &mut self,
        state: EntryLifecycleState,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self
            .state
            .selected_entry_id
            .and_then(|id| self.drafts.get_mut(&id))
        {
            draft.status = state;
            draft.error = None;
            cx.notify();
        }
    }

    fn load_calendar(
        &mut self,
        board_id: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.loading = true;
        self.load_revision += 1;
        let revision = self.load_revision;
        let month = self.state.month;
        let task = self
            .service
            .load(cx.background_executor().clone(), board_id, month);
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.board_id != board_id || this.load_revision != revision {
                    return;
                }
                this.state.loading = false;
                match result {
                    Ok(Ok(snapshot)) => {
                        let CalendarSnapshot {
                            entries,
                            reminders,
                            recurring,
                            lists,
                            board_changed,
                        } = snapshot;
                        let lists_changed = this.lists != lists;
                        let presentation_changed = this.state.entries != entries
                            || this.state.reminders != reminders
                            || this.state.recurring != recurring
                            || lists_changed
                            || this.state.error.is_some();
                        this.lists = lists;
                        for entry in &entries {
                            if let Some(draft) = this.drafts.get_mut(&entry.entry_id) {
                                let clean = !draft.saving
                                    && draft.title.read(cx).value().as_ref() == draft.baseline.0
                                    && date_picker_value(draft.due_picker.read(cx))
                                        == draft.baseline.1
                                    && draft.status == draft.baseline_status;
                                if clean {
                                    draft.title.update(cx, |input, cx| {
                                        input.set_value(entry.title.clone(), window, cx)
                                    });
                                    draft.due_picker.update(cx, |picker, cx| {
                                        picker.set_date(
                                            Date::Single(parse_calendar_date(Some(&entry.due_on))),
                                            window,
                                            cx,
                                        )
                                    });
                                    draft.baseline = (entry.title.clone(), entry.due_on.clone());
                                    draft.status = calendar_entry_lifecycle(entry);
                                    draft.baseline_status = draft.status;
                                    draft.entry = entry.clone();
                                }
                            }
                        }
                        this.state.entries = entries;
                        this.state.reminders = reminders;
                        this.state.recurring = recurring;
                        this.state.error = None;
                        if lists_changed {
                            this.sync_recurrence_entry_picker(window, cx);
                        }
                        if board_changed && let Some(board_id) = board_id {
                            cx.emit(CalendarWorkspaceEvent::Committed(board_id));
                        }
                        if presentation_changed {
                            cx.notify();
                        }
                    }
                    Ok(Err(error)) => {
                        this.state.error = Some(error.to_string().into());
                        cx.notify();
                    }
                    Err(error) => {
                        this.state.error = Some(error.to_string().into());
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn shift_calendar_month(
        &mut self,
        delta: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.month = shift_month(self.state.month, delta);
        self.state.selected_date = None;
        self.state.selected_entry_id = None;
        self.state.selected_reminder_id = None;
        self.state.new_reminder_date = None;
        self.load_calendar(self.board_id, window, cx);
        cx.notify();
    }

    pub(crate) fn create_recurring_task_from_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.creating {
            return;
        }
        let Some(board_id) = self.board_id else {
            return;
        };
        let Some(entry_id) = self
            .state
            .recurrence_entry_select
            .read(cx)
            .selected_value()
            .copied()
        else {
            self.state.recurrence_error = Some(if self.calendar_entry_options().is_empty() {
                "There are no entries on this board to repeat".into()
            } else {
                "Choose an entry to repeat".into()
            });
            cx.notify();
            return;
        };
        let rule = self
            .state
            .recurrence_rule_input
            .read(cx)
            .value()
            .trim()
            .to_string();

        let start_date = date_picker_option(self.state.recurrence_start_picker.read(cx));
        let until_date = date_picker_option(self.state.recurrence_until_picker.read(cx));

        let Some(entry) = self
            .lists
            .iter()
            .flat_map(|list| list.entries.iter())
            .find(|entry| i64::from(entry.id) == entry_id)
        else {
            self.state.recurrence_error = Some("That entry is not on this board".into());
            cx.notify();
            return;
        };
        let Some(start_on) = start_date.or_else(|| entry.due_on.clone()) else {
            self.state.recurrence_error =
                Some("Set a first occurrence date for entries without a due date".into());
            cx.notify();
            return;
        };
        let input = CreateCalendarRecurrence {
            entry_id,
            start_on,
            rule,
            until_on: until_date,
            generation_mode: if self.state.recurrence_on_schedule {
                GenerationMode::OnSchedule
            } else {
                GenerationMode::OnCompletion
            },
        };
        self.state.creating = true;
        self.state.recurrence_error = None;
        let task = self
            .service
            .create_recurring_task(cx.background_executor().clone(), input);

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.board_id != Some(board_id) {
                    return;
                }
                this.state.creating = false;
                match result {
                    Ok(Ok(())) => {
                        this.clear_recurrence_form(window, cx);
                        cx.emit(CalendarWorkspaceEvent::Committed(board_id));
                        this.load_calendar(Some(board_id), window, cx);
                    }
                    Ok(Err(error)) => {
                        this.state.recurrence_error = Some(error.to_string().into());
                    }
                    Err(error) => this.state.recurrence_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn delete_recurring_task(
        &mut self,
        entry_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.board_id else {
            return;
        };
        let task = self
            .service
            .delete_recurring_task(cx.background_executor().clone(), entry_id);

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.board_id != Some(board_id) {
                    return;
                }
                match result {
                    Ok(Ok(())) => {
                        cx.emit(CalendarWorkspaceEvent::Committed(board_id));
                        this.load_calendar(Some(board_id), window, cx);
                    }
                    Ok(Err(error)) => this.state.error = Some(error.to_string().into()),
                    Err(error) => this.state.error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn set_recurrence_generation_mode(
        &mut self,
        on_schedule: bool,
        cx: &mut Context<Self>,
    ) {
        if self.state.recurrence_on_schedule != on_schedule {
            self.state.recurrence_on_schedule = on_schedule;
            cx.notify();
        }
    }

    pub(crate) fn reschedule_calendar_entry(
        &mut self,
        entry_id: i64,
        due_on: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owning_board_id = self
            .state
            .entries
            .iter()
            .find(|entry| entry.entry_id == entry_id)
            .and_then(|entry| u32::try_from(entry.board_id).ok())
            .or(self.board_id);

        let calendar_board_id = self.board_id;
        self.state.loading = true;
        self.state.error = None;

        let task =
            self.service
                .reschedule_entry(cx.background_executor().clone(), entry_id, due_on);

        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.board_id != calendar_board_id {
                    return;
                }
                match result {
                    Ok(Ok(())) => {
                        if let Some(board_id) = owning_board_id {
                            cx.emit(CalendarWorkspaceEvent::Committed(board_id));
                        }
                        this.load_calendar(calendar_board_id, window, cx);
                    }
                    Ok(Err(error)) => {
                        this.state.loading = false;
                        this.state.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.state.loading = false;
                        this.state.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn calendar_entry_lifecycle(entry: &CalendarEntryRecord) -> EntryLifecycleState {
    if entry.cancelled_at.is_some() {
        EntryLifecycleState::Cancelled
    } else if entry.completed_at.is_some() {
        EntryLifecycleState::Completed
    } else {
        EntryLifecycleState::Open
    }
}

fn calendar_entry_lifecycle_label(state: EntryLifecycleState) -> &'static str {
    match state {
        EntryLifecycleState::Open => "Open",
        EntryLifecycleState::Completed => "Done",
        EntryLifecycleState::Cancelled => "Cancelled",
    }
}

fn parse_calendar_date(value: Option<&str>) -> Option<NaiveDate> {
    value
        .filter(|value| !value.is_empty())
        .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
}

fn date_picker_option(picker: &DatePickerState) -> Option<String> {
    match picker.date() {
        Date::Single(Some(date)) => Some(date.format("%Y-%m-%d").to_string()),
        Date::Single(None) | Date::Range(_, _) => None,
    }
}

fn date_picker_value(picker: &DatePickerState) -> String {
    date_picker_option(picker).unwrap_or_default()
}

fn validate_calendar_entry_draft(title: &str, due_on: &str) -> Result<(), &'static str> {
    if title.trim().is_empty() {
        return Err("Title cannot be empty");
    }
    if !due_on.trim().is_empty() && NaiveDate::parse_from_str(due_on.trim(), "%Y-%m-%d").is_err() {
        return Err("Due date must use YYYY-MM-DD");
    }
    Ok(())
}

fn shift_month(month: NaiveDate, delta: i32) -> NaiveDate {
    let index = month.year() * 12 + month.month0() as i32 + delta;
    let year = index.div_euclid(12);
    let month_number = index.rem_euclid(12) as u32 + 1;
    NaiveDate::from_ymd_opt(year, month_number, 1).unwrap_or(month)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CalendarRoute {
    Month,
    Item,
    Recurring,
}

#[derive(Clone, Debug)]
pub enum CalendarWorkspaceEvent {
    Navigate(CalendarRoute),
    Back,
    Board,
    Committed(u32),
    ReminderCommitted,
}

struct EntryDraft {
    title: Entity<InputState>,
    due_picker: Entity<DatePickerState>,
    baseline: (String, String),
    status: EntryLifecycleState,
    baseline_status: EntryLifecycleState,
    entry: CalendarEntryRecord,
    saving: bool,
    error: Option<SharedString>,
}

pub struct CalendarWorkspace {
    board_id: Option<u32>,
    service: Arc<dyn CalendarService>,
    lists: Vec<CalendarListRecord>,
    state: CalendarPanelState,
    drafts: HashMap<i64, EntryDraft>,
    load_revision: u64,
    route_narrow: [bool; 3],
}

impl EventEmitter<CalendarWorkspaceEvent> for CalendarWorkspace {}

impl CalendarWorkspace {
    pub fn new(
        board_id: Option<u32>,
        service: Arc<dyn CalendarService>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            board_id,
            service,
            lists: Vec::new(),
            state: CalendarPanelState::new(window, cx),
            drafts: HashMap::new(),
            load_revision: 0,
            route_narrow: [false; 3],
        }
    }
    fn route_narrow(&self, route: CalendarRoute) -> bool {
        self.route_narrow[match route {
            CalendarRoute::Month => 0,
            CalendarRoute::Item => 1,
            CalendarRoute::Recurring => 2,
        }]
    }
    fn set_route_narrow(&mut self, route: CalendarRoute, narrow: bool) {
        self.route_narrow[match route {
            CalendarRoute::Month => 0,
            CalendarRoute::Item => 1,
            CalendarRoute::Recurring => 2,
        }] = narrow;
    }
    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.load_calendar(self.board_id, window, cx);
    }

    pub fn is_board_calendar(&self) -> bool {
        self.board_id.is_some()
    }

    pub fn create_calendar_reminder(
        &mut self,
        date: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.selected_date = NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok();
        self.state.selected_entry_id = None;
        self.state.selected_reminder_id = None;
        self.state.new_reminder_date = Some(date.clone());
        self.state.reminder_error = None;

        self.state.detail_title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Reminder title"));

        cx.observe(&self.state.detail_title_input, |_, _, cx| cx.notify())
            .detach();

        self.state.detail_due_picker =
            cx.new(|cx| DatePickerState::new(window, cx).date_format("%Y-%m-%d"));

        cx.observe(&self.state.detail_due_picker, |_, _, cx| cx.notify())
            .detach();

        self.state.detail_due_picker.update(cx, |picker, cx| {
            picker.set_date(Date::Single(parse_calendar_date(Some(&date))), window, cx)
        });

        if self.route_narrow(CalendarRoute::Month) {
            cx.emit(CalendarWorkspaceEvent::Navigate(CalendarRoute::Item));
        }

        cx.notify();
    }

    pub fn open_calendar_reminder(
        &mut self,
        reminder_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(reminder) = self
            .state
            .reminders
            .iter()
            .find(|reminder| reminder.id == reminder_id)
            .cloned()
        else {
            return;
        };
        self.state.selected_date = NaiveDate::parse_from_str(&reminder.date, "%Y-%m-%d").ok();
        self.state.selected_entry_id = None;
        self.state.selected_reminder_id = Some(reminder_id);
        self.state.new_reminder_date = None;
        self.state.reminder_error = None;
        self.state.detail_title_input =
            cx.new(|cx| InputState::new(window, cx).default_value(reminder.title));
        cx.observe(&self.state.detail_title_input, |_, _, cx| cx.notify())
            .detach();
        self.state.detail_due_picker =
            cx.new(|cx| DatePickerState::new(window, cx).date_format("%Y-%m-%d"));
        cx.observe(&self.state.detail_due_picker, |_, _, cx| cx.notify())
            .detach();
        self.state.detail_due_picker.update(cx, |picker, cx| {
            picker.set_date(
                Date::Single(parse_calendar_date(Some(&reminder.date))),
                window,
                cx,
            )
        });
        if self.route_narrow(CalendarRoute::Month) {
            cx.emit(CalendarWorkspaceEvent::Navigate(CalendarRoute::Item));
        }
        cx.notify();
    }

    pub(crate) fn save_calendar_reminder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.saving_reminder {
            return;
        }
        let title = self
            .state
            .detail_title_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let Some(date) = date_picker_option(self.state.detail_due_picker.read(cx)) else {
            self.state.reminder_error = Some("Choose a date for this reminder".into());
            cx.notify();
            return;
        };
        if title.is_empty() {
            self.state.reminder_error = Some("Title cannot be empty".into());
            cx.notify();
            return;
        }
        self.state.saving_reminder = true;
        self.state.reminder_error = None;
        let task = self.service.save_reminder(
            cx.background_executor().clone(),
            SaveCalendarReminder {
                id: self.state.selected_reminder_id,
                title,
                date,
            },
        );
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                this.state.saving_reminder = false;
                match result {
                    Ok(Ok(reminder)) => {
                        this.state.selected_reminder_id = Some(reminder.id);
                        this.state.new_reminder_date = None;
                        this.state.reminder_error = None;
                        if let Ok(date) = NaiveDate::parse_from_str(&reminder.date, "%Y-%m-%d") {
                            this.state.month = date.with_day(1).unwrap_or(date);
                            this.state.selected_date = Some(date);
                        }
                        cx.emit(CalendarWorkspaceEvent::ReminderCommitted);
                        this.load_calendar(this.board_id, window, cx);
                    }
                    Ok(Err(error)) => this.state.reminder_error = Some(error.to_string().into()),
                    Err(error) => this.state.reminder_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn delete_calendar_reminder(
        &mut self,
        reminder_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.saving_reminder {
            return;
        }
        self.state.saving_reminder = true;
        self.state.reminder_error = None;
        let task = self
            .service
            .delete_reminder(cx.background_executor().clone(), reminder_id);
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                this.state.saving_reminder = false;
                match result {
                    Ok(Ok(())) => {
                        this.state
                            .reminders
                            .retain(|reminder| reminder.id != reminder_id);
                        this.state.selected_reminder_id = None;
                        this.state.new_reminder_date = None;
                        cx.emit(CalendarWorkspaceEvent::ReminderCommitted);
                        this.load_calendar(this.board_id, window, cx);
                    }
                    Ok(Err(error)) => this.state.reminder_error = Some(error.to_string().into()),
                    Err(error) => this.state.reminder_error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
    fn selected_saving(&self) -> bool {
        self.state
            .selected_entry_id
            .and_then(|id| self.drafts.get(&id))
            .is_some_and(|draft| draft.saving)
    }
    fn selected_error(&self) -> Option<SharedString> {
        self.state
            .selected_entry_id
            .and_then(|id| self.drafts.get(&id))
            .and_then(|draft| draft.error.clone())
    }
    fn selected_dirty(&self, cx: &App) -> bool {
        self.state
            .selected_entry_id
            .and_then(|id| self.drafts.get(&id))
            .is_some_and(|draft| {
                (
                    draft.title.read(cx).value().to_string(),
                    date_picker_value(draft.due_picker.read(cx)),
                ) != draft.baseline
                    || draft.status != draft.baseline_status
            })
    }
    fn discard_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(draft) = self
            .state
            .selected_entry_id
            .and_then(|id| self.drafts.get_mut(&id))
        {
            if draft.saving {
                return;
            }
            draft.title.update(cx, |input, cx| {
                input.set_value(draft.baseline.0.clone(), window, cx)
            });
            draft.due_picker.update(cx, |picker, cx| {
                picker.set_date(
                    Date::Single(parse_calendar_date(Some(&draft.baseline.1))),
                    window,
                    cx,
                )
            });
            draft.status = draft.baseline_status;
            draft.error = None;
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
