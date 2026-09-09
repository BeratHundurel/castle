use chrono::{Datelike, Duration, Local, NaiveDate};
use gpui_kit::component::{
    ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
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
use runtime::AppRuntime;
use storage::workspace::api::{
    CreateRecurringTaskInput, EntryLifecycleState, RecurringTaskInput, SetEntryLifecycleInput,
    UpdateEntryInput,
};

use super::BoardView;

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
    pub(crate) open: bool,
    pub(crate) loading: bool,
    pub(crate) creating: bool,
    pub(crate) error: Option<SharedString>,
    pub(crate) month: NaiveDate,
    pub(crate) entries: Vec<storage::calendar::CalendarEntryRecord>,
    pub(crate) recurring: Vec<storage::calendar::RecurringTaskRecord>,
    recurrence_entry_select: Entity<CalendarEntrySelectState>,
    pub(crate) recurrence_start_input: Entity<InputState>,
    pub(crate) recurrence_rule_input: Entity<InputState>,
    pub(crate) recurrence_until_input: Entity<InputState>,
    pub(crate) recurrence_on_schedule: bool,
    pub(crate) selected_entry_id: Option<i64>,
    pub(crate) detail_title_input: Entity<InputState>,
    pub(crate) detail_due_input: Entity<InputState>,
    pub(crate) detail_saving: bool,
    pub(crate) detail_error: Option<SharedString>,
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
    pub(crate) fn new(window: &mut Window, cx: &mut Context<BoardView>) -> Self {
        let today = Local::now().date_naive();
        let month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
        Self {
            open: false,
            loading: false,
            creating: false,
            error: None,
            month,
            entries: Vec::new(),
            recurring: Vec::new(),
            recurrence_entry_select: cx.new(|cx| {
                SelectState::new(SearchableVec::new(Vec::new()), None, window, cx).searchable(true)
            }),
            recurrence_start_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("First occurrence (YYYY-MM-DD)")),
            recurrence_rule_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("daily / every 2 weeks on mon, wed")
            }),
            recurrence_until_input: cx
                .new(|cx| InputState::new(window, cx).placeholder("Until (optional)")),
            recurrence_on_schedule: false,
            selected_entry_id: None,
            detail_title_input: cx.new(|cx| InputState::new(window, cx).placeholder("Title")),
            detail_due_input: cx.new(|cx| {
                InputState::new(window, cx).placeholder("Due date (YYYY-MM-DD, optional)")
            }),
            detail_saving: false,
            detail_error: None,
        }
    }
}

impl BoardView {
    pub(crate) fn open_calendar_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        self.calendar_panel.open = true;
        self.calendar_panel.error = None;
        self.sync_recurrence_entry_picker(window, cx);
        self.load_calendar(board_id, window, cx);
        cx.notify();
    }

    pub(crate) fn close_calendar_panel(&mut self, cx: &mut Context<Self>) {
        self.calendar_panel.open = false;
        self.calendar_panel.error = None;
        self.calendar_panel.selected_entry_id = None;
        self.calendar_panel.detail_saving = false;
        self.calendar_panel.detail_error = None;
        cx.notify();
    }

    fn calendar_entry_options(&self) -> Vec<CalendarEntryOption> {
        self.data
            .lists
            .iter()
            .flat_map(|list| {
                list.entries.iter().map(|entry| CalendarEntryOption {
                    entry_id: i64::from(entry.id),
                    entry_title: entry.title.clone(),
                    list_title: list.title.clone(),
                })
            })
            .collect()
    }

    fn calendar_entry_label(&self, entry_id: i64) -> String {
        self.data
            .lists
            .iter()
            .flat_map(|list| {
                list.entries
                    .iter()
                    .map(move |entry| (entry, list.title.as_ref()))
            })
            .find(|(entry, _)| i64::from(entry.id) == entry_id)
            .map(|(entry, list_title)| format!("{} · {}", entry.title, list_title))
            .unwrap_or_else(|| format!("Entry {entry_id}"))
    }

    fn sync_recurrence_entry_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let options = self.calendar_entry_options();
        let selected_entry_id = self
            .calendar_panel
            .recurrence_entry_select
            .read(cx)
            .selected_value()
            .copied()
            .filter(|entry_id| options.iter().any(|option| option.entry_id == *entry_id));
        let picker = self.calendar_panel.recurrence_entry_select.clone();
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
        self.calendar_panel
            .recurrence_entry_select
            .update(cx, |picker, cx| picker.set_selected_index(None, window, cx));
        self.calendar_panel
            .recurrence_start_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.calendar_panel
            .recurrence_rule_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.calendar_panel
            .recurrence_until_input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    pub(crate) fn open_calendar_entry(
        &mut self,
        entry_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .calendar_panel
            .entries
            .iter()
            .find(|entry| entry.entry_id == entry_id)
            .cloned()
        else {
            return;
        };
        self.calendar_panel.selected_entry_id = Some(entry_id);
        self.calendar_panel.detail_saving = false;
        self.calendar_panel.detail_error = None;
        let title_input = self.calendar_panel.detail_title_input.clone();
        let due_input = self.calendar_panel.detail_due_input.clone();
        let title = entry.title;
        let due_on = entry.due_on;
        cx.defer_in(window, move |_, window, cx| {
            title_input.update(cx, |input, cx| input.set_value(title, window, cx));
            due_input.update(cx, |input, cx| input.set_value(due_on, window, cx));
        });
        cx.notify();
    }

    pub(crate) fn close_calendar_entry(&mut self, cx: &mut Context<Self>) {
        self.calendar_panel.selected_entry_id = None;
        self.calendar_panel.detail_error = None;
        cx.notify();
    }

    fn open_recurrence_for_entry(
        &mut self,
        entry_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .data
            .lists
            .iter()
            .flat_map(|list| list.entries.iter())
            .find(|entry| i64::from(entry.id) == entry_id)
            .cloned()
        else {
            return;
        };
        self.calendar_panel.selected_entry_id = None;
        self.calendar_panel.detail_error = None;
        self.calendar_panel.error = None;
        self.sync_recurrence_entry_picker(window, cx);
        let picker = self.calendar_panel.recurrence_entry_select.clone();
        picker.update(cx, |picker, cx| {
            picker.set_selected_value(&entry_id, window, cx);
        });
        let start_on = entry.due_on.unwrap_or_default();
        self.calendar_panel
            .recurrence_start_input
            .update(cx, |input, cx| input.set_value(start_on, window, cx));
        cx.notify();
    }

    fn selected_calendar_entry(&self) -> Option<&storage::calendar::CalendarEntryRecord> {
        self.calendar_panel.selected_entry_id.and_then(|entry_id| {
            self.calendar_panel
                .entries
                .iter()
                .find(|entry| entry.entry_id == entry_id)
        })
    }

    pub(crate) fn save_calendar_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        let Some(entry_id) = self.calendar_panel.selected_entry_id else {
            return;
        };
        let title = self
            .calendar_panel
            .detail_title_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let due_text = self
            .calendar_panel
            .detail_due_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        if let Err(error) = validate_calendar_entry_draft(&title, &due_text) {
            self.calendar_panel.detail_error = Some(error.into());
            cx.notify();
            return;
        }
        let Some(app_runtime) = cx.try_global::<AppRuntime>().cloned() else {
            self.calendar_panel.detail_error = Some("Calendar services are unavailable".into());
            cx.notify();
            return;
        };
        self.calendar_panel.detail_saving = true;
        self.calendar_panel.detail_error = None;
        let clear_due_on = due_text.is_empty();
        let input = UpdateEntryInput {
            entry_id,
            title: Some(title),
            description: None,
            due_on: (!clear_due_on).then_some(due_text),
            clear_due_on,
        };
        let task = app_runtime.spawn_store(cx.background_executor(), move |store| async move {
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .update_entry(input)
                .await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.data.board_id != Some(board_id) || !this.calendar_panel.open {
                    return;
                }
                this.calendar_panel.detail_saving = false;
                match result {
                    Ok(Ok(_)) => {
                        this.calendar_panel.detail_error = None;
                        this.reload_board(board_id, cx);
                        this.load_calendar(board_id, window, cx);
                    }
                    Ok(Err(error)) => {
                        this.calendar_panel.detail_error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.calendar_panel.detail_error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn set_calendar_entry_lifecycle(
        &mut self,
        state: EntryLifecycleState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        let Some(entry_id) = self.calendar_panel.selected_entry_id else {
            return;
        };
        let Some(app_runtime) = cx.try_global::<AppRuntime>().cloned() else {
            self.calendar_panel.detail_error = Some("Calendar services are unavailable".into());
            cx.notify();
            return;
        };
        self.calendar_panel.detail_saving = true;
        self.calendar_panel.detail_error = None;
        let input = SetEntryLifecycleInput { entry_id, state };
        let task = app_runtime.spawn_store(cx.background_executor(), move |store| async move {
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .set_entry_lifecycle(input)
                .await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.data.board_id != Some(board_id) || !this.calendar_panel.open {
                    return;
                }
                this.calendar_panel.detail_saving = false;
                match result {
                    Ok(Ok(_)) => {
                        this.calendar_panel.detail_error = None;
                        this.reload_board(board_id, cx);
                        this.load_calendar(board_id, window, cx);
                    }
                    Ok(Err(error)) => {
                        this.calendar_panel.detail_error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.calendar_panel.detail_error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn load_calendar(&mut self, board_id: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.calendar_panel.loading = true;
        let month = self.calendar_panel.month;
        let from = month;
        let through = first_day_of_next_month(month) - Duration::days(1);
        let from_text = from.to_string();
        let through_text = through.to_string();
        let today_text = Local::now().date_naive().to_string();
        let Some(app_runtime) = cx.try_global::<AppRuntime>().cloned() else {
            self.calendar_panel.loading = false;
            self.calendar_panel.error = Some("Calendar services are unavailable".into());
            cx.notify();
            return;
        };
        let task = app_runtime.spawn_store(cx.background_executor(), move |store| async move {
            let today = calendar::CalendarDate::parse(&today_text)
                .map_err(|error| anyhow::anyhow!(error))?;
            storage::calendar::materialize_scheduled_recurring_tasks(&store, today).await?;
            storage::workflow::run_scheduled_workflows(
                &store,
                Some(i64::from(board_id)),
                "calendar_open",
                workflow::EventOrigin::Scheduler,
            )
            .await?;
            let entries = storage::calendar::load_entries(
                &store,
                Some(&from_text),
                Some(&through_text),
                Some(i64::from(board_id)),
            );
            let recurring = storage::calendar::list_recurring_tasks(&store, i64::from(board_id));
            tokio::try_join!(entries, recurring)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.data.board_id != Some(board_id) || !this.calendar_panel.open {
                    return;
                }
                this.calendar_panel.loading = false;
                match result {
                    Ok(Ok((entries, recurring))) => {
                        this.calendar_panel.entries = entries;
                        this.calendar_panel.recurring = recurring;
                        this.calendar_panel.error = None;
                        if this
                            .calendar_panel
                            .selected_entry_id
                            .is_some_and(|entry_id| {
                                !this
                                    .calendar_panel
                                    .entries
                                    .iter()
                                    .any(|entry| entry.entry_id == entry_id)
                            })
                        {
                            this.calendar_panel.selected_entry_id = None;
                            this.calendar_panel.detail_error = None;
                        }
                        this.sync_recurrence_entry_picker(window, cx);
                        this.reload_board(board_id, cx);
                    }
                    Ok(Err(error)) => {
                        this.calendar_panel.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.calendar_panel.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
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
        self.calendar_panel.month = shift_month(self.calendar_panel.month, delta);
        if let Some(board_id) = self.data.board_id {
            self.load_calendar(board_id, window, cx);
        }
        cx.notify();
    }

    pub(crate) fn create_recurring_task_from_form(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        let Some(entry_id) = self
            .calendar_panel
            .recurrence_entry_select
            .read(cx)
            .selected_value()
            .copied()
        else {
            self.calendar_panel.error = Some(if self.calendar_entry_options().is_empty() {
                "There are no entries on this board to repeat".into()
            } else {
                "Choose an entry to repeat".into()
            });
            cx.notify();
            return;
        };
        let rule = self
            .calendar_panel
            .recurrence_rule_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let start_text = self
            .calendar_panel
            .recurrence_start_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let until_text = self
            .calendar_panel
            .recurrence_until_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        let Some(entry) = self
            .data
            .lists
            .iter()
            .flat_map(|list| list.entries.iter())
            .find(|entry| i64::from(entry.id) == entry_id)
        else {
            self.calendar_panel.error = Some("That entry is not on this board".into());
            cx.notify();
            return;
        };
        let start_on = if start_text.is_empty() {
            entry.due_on.as_deref().map(str::to_string)
        } else {
            Some(start_text)
        };
        let Some(start_on) = start_on else {
            self.calendar_panel.error =
                Some("Set a first occurrence date for entries without a due date".into());
            cx.notify();
            return;
        };
        let input = CreateRecurringTaskInput {
            entry_id,
            start_on,
            rule,
            until_on: (!until_text.is_empty()).then_some(until_text),
            occurrence_limit: None,
            generation_mode: Some(if self.calendar_panel.recurrence_on_schedule {
                "on_schedule".to_string()
            } else {
                "on_completion".to_string()
            }),
        };
        self.calendar_panel.creating = true;
        self.calendar_panel.error = None;
        let Some(app_runtime) = cx.try_global::<AppRuntime>().cloned() else {
            self.calendar_panel.creating = false;
            self.calendar_panel.error = Some("Calendar services are unavailable".into());
            cx.notify();
            return;
        };
        let task = app_runtime.spawn_store(cx.background_executor(), move |store| async move {
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .create_recurring_task(input)
                .await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.data.board_id != Some(board_id) || !this.calendar_panel.open {
                    return;
                }
                this.calendar_panel.creating = false;
                match result {
                    Ok(Ok(_)) => {
                        this.clear_recurrence_form(window, cx);
                        this.load_calendar(board_id, window, cx);
                    }
                    Ok(Err(error)) => {
                        this.calendar_panel.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.calendar_panel.error = Some(error.to_string().into());
                    }
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
        let Some(board_id) = self.data.board_id else {
            return;
        };
        let input = RecurringTaskInput { entry_id };
        let Some(app_runtime) = cx.try_global::<AppRuntime>().cloned() else {
            self.calendar_panel.error = Some("Calendar services are unavailable".into());
            cx.notify();
            return;
        };
        let task = app_runtime.spawn_store(cx.background_executor(), move |store| async move {
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .delete_recurring_task(input)
                .await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.data.board_id != Some(board_id) || !this.calendar_panel.open {
                    return;
                }
                match result {
                    Ok(Ok(())) => this.load_calendar(board_id, window, cx),
                    Ok(Err(error)) => this.calendar_panel.error = Some(error.to_string().into()),
                    Err(error) => this.calendar_panel.error = Some(error.to_string().into()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn toggle_recurrence_generation_mode(&mut self, cx: &mut Context<Self>) {
        self.calendar_panel.recurrence_on_schedule = !self.calendar_panel.recurrence_on_schedule;
        cx.notify();
    }

    pub(crate) fn reschedule_calendar_entry(
        &mut self,
        entry_id: i64,
        due_on: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.data.board_id else {
            return;
        };
        self.calendar_panel.loading = true;
        self.calendar_panel.error = None;
        let input = storage::workspace::api::SetEntryScheduleInput {
            entry_id,
            start_on: None,
            due_on: Some(due_on),
            clear_start_on: false,
            clear_due_on: false,
        };
        let Some(app_runtime) = cx.try_global::<AppRuntime>().cloned() else {
            self.calendar_panel.loading = false;
            self.calendar_panel.error = Some("Calendar services are unavailable".into());
            cx.notify();
            return;
        };
        let task = app_runtime.spawn_store(cx.background_executor(), move |store| async move {
            store
                .mutations(storage::MutationOrigin::LocalApp)
                .set_entry_schedule(input)
                .await
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            this.update_in(cx, |this, window, cx| {
                if this.data.board_id != Some(board_id) || !this.calendar_panel.open {
                    return;
                }
                match result {
                    Ok(Ok(_)) => {
                        this.reload_board(board_id, cx);
                        this.load_calendar(board_id, window, cx);
                    }
                    Ok(Err(error)) => {
                        this.calendar_panel.loading = false;
                        this.calendar_panel.error = Some(error.to_string().into());
                    }
                    Err(error) => {
                        this.calendar_panel.loading = false;
                        this.calendar_panel.error = Some(error.to_string().into());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn render_calendar_panel_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        if !self.calendar_panel.open {
            return div().into_any_element();
        }
        let theme = cx.theme().clone();
        let board_title = self
            .data
            .board_id
            .and_then(|board_id| {
                self.data
                    .lists
                    .iter()
                    .find(|list| list.board_id == board_id)
            })
            .map(|_| "Board calendar")
            .unwrap_or("Calendar");
        let month_label = self.calendar_panel.month.format("%B %Y").to_string();
        let month = self.calendar_panel.month;
        let calendar_grid = self.render_calendar_grid(month, cx);

        div()
            .id("calendar-panel-overlay")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme.overlay.opacity(0.78))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(
                v_flex()
                    .id("calendar-panel")
                    .w(px(1120.))
                    .max_w(gpui_kit::relative(0.96))
                    .h(px(720.))
                    .max_h(gpui_kit::relative(0.94))
                    .overflow_hidden()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border.opacity(0.78))
                    .bg(theme.popover)
                    .text_color(theme.popover_foreground)
                    .shadow_lg()
                    .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation()
                    })
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(
                        h_flex()
                            .min_h_12()
                            .px_4()
                            .gap_3()
                            .items_center()
                            .border_b_1()
                            .border_color(theme.border.opacity(0.72))
                            .child(
                                Icon::new(IconName::Calendar)
                                    .small()
                                    .text_color(theme.primary),
                            )
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(board_title))
                            .child(div().flex_1())
                            .child(
                                Button::new("calendar-previous-month")
                                    .icon(IconName::ChevronLeft)
                                    .ghost()
                                    .small()
                                    .tooltip("Previous month")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.shift_calendar_month(-1, window, cx);
                                    })),
                            )
                            .child(
                                div()
                                    .w(px(130.))
                                    .text_center()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(month_label),
                            )
                            .child(
                                Button::new("calendar-next-month")
                                    .icon(IconName::ChevronRight)
                                    .ghost()
                                    .small()
                                    .tooltip("Next month")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.shift_calendar_month(1, window, cx);
                                    })),
                            )
                            .child(
                                Button::new("calendar-close")
                                    .icon(IconName::Close)
                                    .ghost()
                                    .small()
                                    .tooltip("Close calendar")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.close_calendar_panel(cx);
                                    })),
                            ),
                    )
                    .when_some(self.calendar_panel.error.clone(), |this, error| {
                        this.child(
                            div()
                                .px_4()
                                .py_2()
                                .text_sm()
                                .text_color(theme.danger)
                                .bg(theme.danger.opacity(0.08))
                                .child(error),
                        )
                    })
                    .child(
                        h_flex()
                            .flex_1()
                            .min_h_0()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .p_4()
                                    .gap_3()
                                    .overflow_y_scrollbar()
                                    .child(calendar_grid),
                            )
                            .child(self.render_calendar_sidebar(cx)),
                    ),
            )
            .into_any_element()
    }

    fn render_calendar_grid(&self, month: NaiveDate, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let weekday_headers = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let first_weekday = month.weekday().num_days_from_monday() as i64;
        let days_in_month = (first_day_of_next_month(month) - Duration::days(1)).day();
        let cells = (0..42).map(|index| {
            let day_offset = index as i64 - first_weekday;
            let day = month + Duration::days(day_offset);
            let in_month = day.month() == month.month();
            let date_text = if in_month {
                day.day().to_string()
            } else {
                String::new()
            };
            let day_entries = if in_month {
                self.calendar_panel
                    .entries
                    .iter()
                    .filter(|entry| entry.due_on == day.to_string())
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let mut cell = v_flex()
                .id(SharedString::from(format!("calendar-day-{index}")))
                .flex_1()
                .min_h(px(78.))
                .min_w_0()
                .gap_1()
                .p_1p5()
                .border_1()
                .border_color(theme.border.opacity(if in_month { 0.68 } else { 0.28 }))
                .bg(if in_month {
                    theme.background
                } else {
                    theme.background.opacity(0.36)
                })
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if in_month {
                            theme.foreground
                        } else {
                            theme.muted_foreground.opacity(0.58)
                        })
                        .child(date_text),
                )
                .drag_over::<CalendarEntryDrag>(|this, _, _, cx| {
                    this.border_2()
                        .border_color(cx.theme().primary)
                        .bg(cx.theme().drop_target)
                })
                .on_drop(
                    cx.listener(move |this, info: &CalendarEntryDrag, window, cx| {
                        this.reschedule_calendar_entry(info.entry_id, day.to_string(), window, cx);
                    }),
                );
            for entry in day_entries.into_iter().take(3) {
                let entry_id = entry.entry_id;
                let selected = self.calendar_panel.selected_entry_id == Some(entry_id);
                let accent = match entry.workflow_role {
                    storage::board::ListWorkflowRole::Neutral => theme.primary,
                    storage::board::ListWorkflowRole::Done => theme.success,
                    storage::board::ListWorkflowRole::Cancelled => theme.danger,
                };
                let label = if entry.occurrence_key.is_some() {
                    format!("↻ {}", entry.title)
                } else {
                    entry.title.clone()
                };
                let can_drag = entry.occurrence_key.is_none();
                let mut entry_view = div()
                    .id(SharedString::from(format!(
                        "calendar-entry-{}-{}",
                        entry.entry_id, day
                    )))
                    .debug_selector(|| format!("calendar-entry-{entry_id}-{day}"))
                    .w_full()
                    .px_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .bg(accent.opacity(if selected { 0.28 } else { 0.12 }))
                    .text_xs()
                    .text_color(accent)
                    .hover(|this| this.bg(accent.opacity(0.24)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_calendar_entry(entry_id, window, cx);
                    }))
                    .child(label);
                if can_drag {
                    entry_view = entry_view.on_drag(
                        CalendarEntryDrag {
                            entry_id: entry.entry_id,
                            title: entry.title.clone().into(),
                        },
                        |drag, _, _, cx| cx.new(|_| drag.clone()),
                    );
                }
                cell = cell.child(entry_view);
            }
            cell.into_any_element()
        });
        let weeks = (0..6).map(|week| {
            h_flex()
                .flex_1()
                .min_h_0()
                .children(cells.clone().skip(week * 7).take(7))
                .into_any_element()
        });
        v_flex()
            .w_full()
            .gap_1()
            .child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .children(weekday_headers.into_iter().map(|weekday| {
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(weekday)
                            .into_any_element()
                    })),
            )
            .children(weeks)
            .children((days_in_month == 0).then(|| div()))
            .into_any_element()
    }

    fn render_calendar_recurrence_form(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let has_entries = !self.calendar_entry_options().is_empty();
        let recurrence_picker = Select::new(&self.calendar_panel.recurrence_entry_select)
            .id("calendar-recurrence-entry-picker")
            .placeholder(if has_entries {
                "Choose an entry"
            } else {
                "No entries available"
            })
            .accessibility_label("Entry to repeat")
            .search_placeholder("Search entries by title or list")
            .menu_max_h(px(240.))
            .disabled(!has_entries)
            .w_full();

        v_flex()
            .id("calendar-add-recurring-section")
            .debug_selector(|| "calendar-add-recurring-section".into())
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(theme.border.opacity(0.72))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Add recurring task"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Choose a board entry, then define when its next copy is created."),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child("Entry"),
            )
            .child(
                div()
                    .debug_selector(|| "calendar-recurrence-entry-picker".into())
                    .child(recurrence_picker),
            )
            .when(!has_entries, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.warning)
                        .child("Add an entry to this board before creating a recurring task."),
                )
            })
            .when(has_entries, |this| {
                this.child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.muted_foreground)
                        .child("First occurrence"),
                )
            })
            .child(Input::new(&self.calendar_panel.recurrence_start_input).w_full())
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child("Rule"),
            )
            .child(Input::new(&self.calendar_panel.recurrence_rule_input).w_full())
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child("Until (optional)"),
            )
            .child(Input::new(&self.calendar_panel.recurrence_until_input).w_full())
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Leave the first date blank to use the entry's due date."),
            )
            .child(
                Button::new("recurrence-generation-mode")
                    .label(if self.calendar_panel.recurrence_on_schedule {
                        "Generate on schedule"
                    } else {
                        "Generate after completion"
                    })
                    .small()
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_recurrence_generation_mode(cx);
                    })),
            )
            .child(
                Button::new("create-recurring-task")
                    .icon(IconName::Plus)
                    .label(if self.calendar_panel.creating {
                        "Creating..."
                    } else {
                        "Create recurrence"
                    })
                    .primary()
                    .small()
                    .disabled(self.calendar_panel.creating || !has_entries)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.create_recurring_task_from_form(window, cx);
                    })),
            )
            .into_any_element()
    }

    fn render_calendar_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let selected_entry = self.selected_calendar_entry().cloned();
        let show_recurrence_form = selected_entry.is_none();

        let selected_item_section = match selected_entry {
            Some(entry) => v_flex()
                .id("calendar-selected-item-section")
                .gap_2()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(theme.border.opacity(0.72))
                .bg(theme.background.opacity(0.28))
                .child(self.render_calendar_entry_details(&entry, cx))
                .into_any_element(),
            None => v_flex()
                .id("calendar-selected-item-section")
                .debug_selector(|| "calendar-selected-item-section".into())
                .gap_1()
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(theme.border.opacity(0.72))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Selected item"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Click an event in the calendar to inspect and edit it."),
                )
                .into_any_element(),
        };

        let recurring_items = if self.calendar_panel.recurring.is_empty() {
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("No recurring schedules yet.")
                .into_any_element()
        } else {
            v_flex()
                .gap_1()
                .children(self.calendar_panel.recurring.iter().map(|recurring| {
                    let entry_id = recurring.entry_id;
                    h_flex()
                        .id(SharedString::from(format!("recurring-task-{entry_id}")))
                        .w_full()
                        .gap_1()
                        .items_center()
                        .p_1()
                        .rounded_sm()
                        .bg(theme.background)
                        .child(div().flex_1().min_w_0().text_xs().child(format!(
                            "{} · next {}",
                            self.calendar_entry_label(entry_id),
                            recurring.next_on
                        )))
                        .child(
                            Button::new(SharedString::from(format!(
                                "delete-recurrence-{entry_id}"
                            )))
                            .icon(IconName::Delete)
                            .ghost()
                            .compact()
                            .tooltip("Stop recurring task")
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.delete_recurring_task(entry_id, window, cx);
                                },
                            )),
                        )
                        .into_any_element()
                }))
                .into_any_element()
        };

        let sidebar_content = v_flex()
            .w_full()
            .gap_3()
            .p_3()
            .child(selected_item_section)
            .child(
                v_flex()
                    .id("calendar-recurring-tasks-section")
                    .debug_selector(|| "calendar-recurring-tasks-section".into())
                    .gap_2()
                    .p_3()
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border.opacity(0.72))
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Recurring tasks"),
                            )
                            .when(!self.calendar_panel.recurring.is_empty(), |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(self.calendar_panel.recurring.len().to_string()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Schedules that generate or advance this board's entries."),
                    )
                    .child(recurring_items),
            )
            .when(show_recurrence_form, |this| {
                this.child(self.render_calendar_recurrence_form(cx))
            });

        div()
            .id("calendar-sidebar-scroll")
            .debug_selector(|| "calendar-sidebar-scroll-owner".into())
            .w(px(320.))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border.opacity(0.72))
            .overflow_y_scrollbar()
            .child(sidebar_content)
            .into_any_element()
    }

    fn render_calendar_detail_header(&self, entry_id: i64, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .items_center()
            .justify_between()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Item details"),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        div()
                            .debug_selector(|| "calendar-repeat-entry".into())
                            .child(
                                Button::new("calendar-repeat-entry")
                                    .label("Repeat")
                                    .outline()
                                    .small()
                                    .tooltip("Create a recurring schedule for this item")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_recurrence_for_entry(entry_id, window, cx);
                                    })),
                            ),
                    )
                    .child(
                        Button::new("calendar-close-entry-details")
                            .icon(IconName::Close)
                            .ghost()
                            .compact()
                            .tooltip("Close item details")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_calendar_entry(cx);
                            })),
                    ),
            )
            .into_any_element()
    }

    fn render_calendar_lifecycle_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        h_flex()
            .gap_1()
            .child(
                Button::new("calendar-entry-status-open")
                    .label("Open")
                    .outline()
                    .small()
                    .disabled(self.calendar_panel.detail_saving)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_calendar_entry_lifecycle(EntryLifecycleState::Open, window, cx);
                    })),
            )
            .child(
                Button::new("calendar-entry-status-done")
                    .label("Done")
                    .outline()
                    .small()
                    .disabled(self.calendar_panel.detail_saving)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_calendar_entry_lifecycle(
                            EntryLifecycleState::Completed,
                            window,
                            cx,
                        );
                    })),
            )
            .child(
                Button::new("calendar-entry-status-cancelled")
                    .label("Cancelled")
                    .outline()
                    .small()
                    .disabled(self.calendar_panel.detail_saving)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.set_calendar_entry_lifecycle(
                            EntryLifecycleState::Cancelled,
                            window,
                            cx,
                        );
                    })),
            )
            .into_any_element()
    }

    fn render_calendar_entry_details(
        &self,
        entry: &storage::calendar::CalendarEntryRecord,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let status = calendar_entry_lifecycle(entry);
        let status_label = calendar_entry_lifecycle_label(status);
        let status_color = match status {
            EntryLifecycleState::Open => theme.primary,
            EntryLifecycleState::Completed => theme.success,
            EntryLifecycleState::Cancelled => theme.danger,
        };
        let entry_id = entry.entry_id;
        let occurrence_note = entry
            .occurrence_key
            .as_ref()
            .map(|_| "Recurring occurrence · edits apply to the source card");

        v_flex()
            .id("calendar-entry-details")
            .debug_selector(|| "calendar-entry-details".into())
            .gap_2()
            .pb_3()
            .border_b_1()
            .border_color(theme.border.opacity(0.72))
            .child(self.render_calendar_detail_header(entry_id, cx))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("{} · {}", entry.list_title, entry.due_on)),
            )
            .when(!entry.description.trim().is_empty(), |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(entry.description.clone()),
                )
            })
            .when_some(occurrence_note, |this, note| {
                this.child(div().text_xs().text_color(theme.warning).child(note))
            })
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child("Title"),
            )
            .child(Input::new(&self.calendar_panel.detail_title_input).w_full())
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.muted_foreground)
                    .child("Due date"),
            )
            .child(Input::new(&self.calendar_panel.detail_due_input).w_full())
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Use YYYY-MM-DD. Leave blank to remove the due date."),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(status_color)
                            .child(format!("Status: {status_label}")),
                    )
                    .child(
                        Button::new("save-calendar-entry")
                            .label(if self.calendar_panel.detail_saving {
                                "Saving..."
                            } else {
                                "Save"
                            })
                            .primary()
                            .small()
                            .disabled(self.calendar_panel.detail_saving)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.save_calendar_entry(window, cx);
                            })),
                    ),
            )
            .child(self.render_calendar_lifecycle_actions(cx))
            .when_some(self.calendar_panel.detail_error.clone(), |this, error| {
                this.child(div().text_xs().text_color(theme.danger).child(error))
            })
            .child(
                div()
                    .id(SharedString::from(format!(
                        "calendar-entry-detail-id-{entry_id}"
                    )))
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!("Entry #{entry_id}")),
            )
            .into_any_element()
    }
}

fn calendar_entry_lifecycle(entry: &storage::calendar::CalendarEntryRecord) -> EntryLifecycleState {
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

fn validate_calendar_entry_draft(title: &str, due_on: &str) -> Result<(), &'static str> {
    if title.trim().is_empty() {
        return Err("Title cannot be empty");
    }
    if !due_on.trim().is_empty() && NaiveDate::parse_from_str(due_on.trim(), "%Y-%m-%d").is_err() {
        return Err("Due date must use YYYY-MM-DD");
    }
    Ok(())
}

fn first_day_of_next_month(month: NaiveDate) -> NaiveDate {
    shift_month(month, 1)
}

fn shift_month(month: NaiveDate, delta: i32) -> NaiveDate {
    let index = month.year() * 12 + month.month0() as i32 + delta;
    let year = index.div_euclid(12);
    let month_number = index.rem_euclid(12) as u32 + 1;
    NaiveDate::from_ymd_opt(year, month_number, 1).unwrap_or(month)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{
        ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext, component::Root, point,
        size,
    };

    fn test_entry() -> storage::calendar::CalendarEntryRecord {
        storage::calendar::CalendarEntryRecord {
            entry_id: 42,
            title: "Calendar review".to_string(),
            description: "Review the calendar interaction".to_string(),
            start_on: None,
            due_on: "2026-09-12".to_string(),
            list_id: 7,
            list_title: "Planning".to_string(),
            board_id: 7,
            board_title: "Calendar board".to_string(),
            workflow_role: storage::board::ListWorkflowRole::Neutral,
            completed_at: None,
            cancelled_at: None,
            recurrence_series_id: None,
            occurrence_key: None,
        }
    }

    fn test_card() -> crate::model::BoardCardState {
        crate::model::BoardCardState {
            id: 42,
            title: "Calendar review".into(),
            description: "Review the calendar interaction".into(),
            card_id: 7,
            position: 0,
            start_on: None,
            due_on: Some("2026-09-12".into()),
            completed_at: None,
            cancelled_at: None,
            archived: false,
            reminder_enabled: false,
            labels: Vec::new(),
            checklist_items: Vec::new(),
            attachments: Vec::new(),
            related_notes: Vec::new(),
        }
    }

    #[gpui_kit::test]
    fn clicking_calendar_entry_opens_editable_details(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("calendar test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.update(|cx| {
            cx.set_global(gpui_kit::component::Theme::default());
            gpui_kit::init(cx);
        });
        let mut board_view = None;
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                let view = BoardView::view(window, cx);
                board_view = Some(view.clone());
                view.update(cx, |board, cx| {
                    board.data.board_id = Some(7);
                    board.data.lists = vec![crate::model::BoardListState {
                        id: 7,
                        title: "Planning".into(),
                        board_id: 7,
                        position: 0,
                        workflow_role: storage::board::ListWorkflowRole::Neutral,
                        entries: vec![test_card()],
                    }];
                    board.calendar_panel.month = NaiveDate::from_ymd_opt(2026, 9, 1)
                        .expect("calendar test month should be valid");
                    board.calendar_panel.entries = vec![test_entry()];
                    board.calendar_panel.open = true;
                    cx.notify();
                });
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("calendar test window should open")
        });
        let view = board_view.expect("calendar test view should exist");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(1_200.), px(768.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let entry_bounds = cx
            .debug_bounds("calendar-entry-42-2026-09-12")
            .expect("calendar entry should be rendered");
        cx.simulate_click(entry_bounds.center(), gpui_kit::Modifiers::default());
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let details = view.read_with(&cx, |board, cx| {
            (
                board.calendar_panel.selected_entry_id,
                board
                    .calendar_panel
                    .detail_title_input
                    .read(cx)
                    .value()
                    .to_string(),
                board
                    .calendar_panel
                    .detail_due_input
                    .read(cx)
                    .value()
                    .to_string(),
            )
        });
        assert_eq!(
            details,
            (
                Some(42),
                "Calendar review".to_string(),
                "2026-09-12".to_string()
            )
        );
        assert!(cx.debug_bounds("calendar-entry-details").is_some());
        assert!(cx.debug_bounds("calendar-repeat-entry").is_some());
        assert!(cx.debug_bounds("calendar-add-recurring-section").is_none());

        let repeat_bounds = cx
            .debug_bounds("calendar-repeat-entry")
            .expect("repeat action should remain interactive");
        cx.simulate_click(repeat_bounds.center(), gpui_kit::Modifiers::default());
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let recurrence_state = view.read_with(&cx, |board, cx| {
            (
                board.calendar_panel.selected_entry_id,
                board
                    .calendar_panel
                    .recurrence_entry_select
                    .read(cx)
                    .selected_value()
                    .copied(),
                board
                    .calendar_panel
                    .recurrence_start_input
                    .read(cx)
                    .value()
                    .to_string(),
            )
        });
        assert_eq!(recurrence_state, (None, Some(42), "2026-09-12".to_string()));
        assert!(cx.debug_bounds("calendar-entry-details").is_none());
        assert!(cx.debug_bounds("calendar-add-recurring-section").is_some());
    }

    #[gpui_kit::test]
    fn calendar_sidebar_owns_scroll_and_picker_selection_keeps_entry_id(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Runtime::new().expect("calendar test runtime should start");
        let _runtime_guard = runtime.enter();
        cx.update(|cx| {
            cx.set_global(gpui_kit::component::Theme::default());
            gpui_kit::init(cx);
        });
        let mut board_view = None;
        let window = cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                let view = BoardView::view(window, cx);
                board_view = Some(view.clone());
                view.update(cx, |board, cx| {
                    let mut second_card = test_card();
                    second_card.id = 43;
                    second_card.title = "Ship calendar notes".into();
                    second_card.due_on = None;
                    board.data.board_id = Some(7);
                    board.data.lists = vec![crate::model::BoardListState {
                        id: 7,
                        title: "Planning".into(),
                        board_id: 7,
                        position: 0,
                        workflow_role: storage::board::ListWorkflowRole::Neutral,
                        entries: vec![test_card(), second_card],
                    }];
                    board.calendar_panel.month = NaiveDate::from_ymd_opt(2026, 9, 1)
                        .expect("calendar test month should be valid");
                    board.calendar_panel.entries = vec![test_entry()];
                    board.calendar_panel.open = true;
                    cx.notify();
                });
                view.update(cx, |board, cx| {
                    board.sync_recurrence_entry_picker(window, cx)
                });
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("calendar test window should open")
        });
        let view = board_view.expect("calendar test view should exist");
        let mut cx = VisualTestContext::from_window(window.into(), cx);
        cx.simulate_resize(size(px(1_200.), px(768.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let sidebar = cx
            .debug_bounds("calendar-sidebar-scroll-owner")
            .expect("calendar sidebar should have one scroll owner");
        let add_section = cx
            .debug_bounds("calendar-add-recurring-section")
            .expect("recurrence section should render inside the sidebar");
        assert_eq!(sidebar.size.width, px(320.));
        assert!(add_section.left() > sidebar.left());
        assert!(add_section.right() < sidebar.right());

        let picker = cx
            .debug_bounds("calendar-recurrence-entry-picker")
            .expect("entry picker should render");
        cx.simulate_click(picker.center(), gpui_kit::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let option = cx
            .debug_bounds("calendar-entry-picker-option-43")
            .expect("picker should show the entry title option");
        cx.simulate_click(option.center(), gpui_kit::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });

        let selected_entry_id = view.read_with(&cx, |board, cx| {
            board
                .calendar_panel
                .recurrence_entry_select
                .read(cx)
                .selected_value()
                .copied()
        });
        assert_eq!(selected_entry_id, Some(43));

        cx.simulate_resize(size(px(1_200.), px(480.)));
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let content_before = cx
            .debug_bounds("calendar-selected-item-section")
            .expect("sidebar content should be visible");
        let sidebar_wheel = ScrollWheelEvent {
            position: point(sidebar.center().x, content_before.top() + px(20.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(-24.))),
            ..Default::default()
        };
        cx.simulate_event(sidebar_wheel);
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        let content_after = cx
            .debug_bounds("calendar-selected-item-section")
            .expect("sidebar content should remain mounted");
        assert!(
            content_after.top() < content_before.top(),
            "wheel input must move sidebar content"
        );
    }

    #[test]
    fn calendar_entry_draft_validation_covers_title_and_due_date() {
        assert_eq!(
            validate_calendar_entry_draft("", "2026-09-12").expect_err("empty title should fail"),
            "Title cannot be empty"
        );
        assert_eq!(
            validate_calendar_entry_draft("Review", "tomorrow")
                .expect_err("invalid due date should fail"),
            "Due date must use YYYY-MM-DD"
        );
        assert!(validate_calendar_entry_draft("Review", "").is_ok());
        assert!(validate_calendar_entry_draft("Review", "2026-09-12").is_ok());
    }
}
