use std::sync::{Arc, Mutex};

use super::{
    CalendarEntryRecord, CalendarListEntry, CalendarListRecord, CalendarListRole, CalendarPage,
    CalendarRoute, CalendarService, CalendarSnapshot, CalendarTask, CalendarWorkspace,
    CalendarWorkspaceEvent, CreateCalendarRecurrence, EntryLifecycleState, RecurringTaskRecord,
    SaveCalendarEntry,
};
use chrono::NaiveDate;
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, ScrollDelta, ScrollWheelEvent,
    Styled, TestAppContext, VisualTestContext, Window, div, point, px, size,
};

struct RetainedCalendarPages {
    month: Entity<CalendarPage>,
    recurring: Entity<CalendarPage>,
}

impl Render for RetainedCalendarPages {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .child(div().w(px(1200.)).h_full().child(self.month.clone()))
            .child(div().w(px(600.)).h_full().child(self.recurring.clone()))
    }
}

fn entry(id: i64) -> CalendarEntryRecord {
    CalendarEntryRecord {
        entry_id: id,
        title: format!("Calendar item {id}"),
        description: "Details\n".repeat(80),
        start_on: None,
        due_on: "2026-09-12".into(),
        list_id: 7,
        list_title: "Planning".into(),
        board_id: 7,
        board_title: "Calendar board".into(),
        workflow_role: CalendarListRole::Neutral,
        completed_at: None,
        cancelled_at: None,
        recurrence_series_id: None,
        occurrence_key: None,
    }
}

#[derive(Clone)]
struct TestCalendarService {
    snapshot: Arc<Mutex<CalendarSnapshot>>,
    fail_writes: bool,
}

impl TestCalendarService {
    fn failing() -> Arc<Self> {
        Arc::new(Self {
            snapshot: Arc::new(Mutex::new(CalendarSnapshot::default())),
            fail_writes: true,
        })
    }

    fn in_memory(snapshot: CalendarSnapshot) -> Arc<Self> {
        Arc::new(Self {
            snapshot: Arc::new(Mutex::new(snapshot)),
            fail_writes: false,
        })
    }

    fn task<T: Send + 'static>(
        executor: gpui_kit::BackgroundExecutor,
        result: anyhow::Result<T>,
    ) -> CalendarTask<T> {
        executor.spawn(async move { Ok(result) })
    }
}

impl CalendarService for TestCalendarService {
    fn load(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        _: u32,
        _: NaiveDate,
    ) -> CalendarTask<CalendarSnapshot> {
        let result = self
            .snapshot
            .lock()
            .map(|snapshot| snapshot.clone())
            .map_err(|_| anyhow::anyhow!("calendar test service lock poisoned"));
        Self::task(executor, result)
    }

    fn save_entry(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        request: SaveCalendarEntry,
    ) -> CalendarTask<()> {
        if self.fail_writes {
            return Self::task(executor, Err(anyhow::anyhow!("service unavailable")));
        }
        let result = self
            .snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("calendar test service lock poisoned"))
            .map(|mut snapshot| {
                if let Some(entry) = snapshot
                    .entries
                    .iter_mut()
                    .find(|entry| entry.entry_id == request.entry_id)
                {
                    entry.title = request.title;
                    entry.due_on = request.due_on.unwrap_or_default();
                    entry.completed_at = match request.status {
                        EntryLifecycleState::Completed => Some(1),
                        _ => None,
                    };
                    entry.cancelled_at = match request.status {
                        EntryLifecycleState::Cancelled => Some(1),
                        _ => None,
                    };
                }
            });
        Self::task(executor, result)
    }

    fn create_recurring_task(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        request: CreateCalendarRecurrence,
    ) -> CalendarTask<()> {
        if self.fail_writes {
            return Self::task(executor, Err(anyhow::anyhow!("service unavailable")));
        }
        let result = (|| {
            let start_on = super::CalendarDate::parse(&request.start_on)
                .map_err(|error| anyhow::anyhow!(error))?;
            let rule = super::RecurrenceRule::parse(&request.rule)
                .map_err(|error| anyhow::anyhow!(error))?;
            let until_on = request
                .until_on
                .as_deref()
                .map(super::CalendarDate::parse)
                .transpose()
                .map_err(|error| anyhow::anyhow!(error))?;
            let mut snapshot = self
                .snapshot
                .lock()
                .map_err(|_| anyhow::anyhow!("calendar test service lock poisoned"))?;
            let id = snapshot
                .recurring
                .iter()
                .map(|task| task.id)
                .max()
                .unwrap_or(0)
                + 1;
            snapshot.recurring.push(RecurringTaskRecord {
                id,
                entry_id: request.entry_id,
                start_on,
                rule,
                next_on: start_on,
                until_on,
                occurrence_limit: None,
                generation_mode: request.generation_mode,
                enabled: true,
            });
            Ok(())
        })();
        Self::task(executor, result)
    }

    fn delete_recurring_task(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        entry_id: i64,
    ) -> CalendarTask<()> {
        if self.fail_writes {
            return Self::task(executor, Err(anyhow::anyhow!("service unavailable")));
        }
        let result = self
            .snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("calendar test service lock poisoned"))
            .map(|mut snapshot| snapshot.recurring.retain(|task| task.entry_id != entry_id));
        Self::task(executor, result)
    }

    fn reschedule_entry(
        &self,
        executor: gpui_kit::BackgroundExecutor,
        entry_id: i64,
        due_on: String,
    ) -> CalendarTask<()> {
        if self.fail_writes {
            return Self::task(executor, Err(anyhow::anyhow!("service unavailable")));
        }
        let result = self
            .snapshot
            .lock()
            .map_err(|_| anyhow::anyhow!("calendar test service lock poisoned"))
            .map(|mut snapshot| {
                if let Some(entry) = snapshot
                    .entries
                    .iter_mut()
                    .find(|entry| entry.entry_id == entry_id)
                {
                    entry.due_on = due_on;
                }
            });
        Self::task(executor, result)
    }
}

#[gpui_kit::test]
fn retained_routes_do_not_share_responsive_measurement(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let _guard = runtime.enter();
    let service = TestCalendarService::failing();
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service, window, cx));
            let month = cx.new(|cx| CalendarPage::new(model.clone(), CalendarRoute::Month, cx));
            let recurring = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Recurring, cx));
            let pages = cx.new(|_| RetainedCalendarPages { month, recurring });
            cx.new(|cx| gpui_kit::component::Root::new(pages, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1800.), px(700.)));

    for _ in 0..2 {
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
    }

    assert!(
        cx.debug_bounds("calendar-up-next").is_some(),
        "a narrow retained route must not remove Upcoming from the wide month route"
    );
}

#[gpui_kit::test]
fn calendar_click_scroll_and_drafts(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let _guard = runtime.enter();
    let service = TestCalendarService::failing();
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service.clone(), window, cx));
            model.update(cx, |model, cx| {
                model.state.month = NaiveDate::from_ymd_opt(2026, 9, 1).expect("date");
                model.state.entries = vec![entry(42), entry(43)];
                cx.notify();
            });
            workspace = Some(model.clone());
            let page = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Month, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let model = workspace.expect("workspace");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.), px(600.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("calendar-sidebar-scroll-owner").is_none());
    let agenda_row = cx
        .debug_bounds("calendar-agenda-row-42")
        .expect("agenda row");
    let agenda_title = cx
        .debug_bounds("calendar-agenda-title-42")
        .expect("agenda title");
    assert_eq!(
        agenda_title.left(),
        agenda_row.left(),
        "agenda titles should align with their metadata instead of centering in the rail"
    );
    let upcoming_width = cx
        .debug_bounds("calendar-up-next")
        .expect("upcoming sidebar")
        .size
        .width;
    let bounds = cx
        .debug_bounds("calendar-entry-42-2026-09-12")
        .expect("event");
    cx.simulate_click(bounds.center(), Default::default());
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("calendar-sidebar-scroll-owner").is_some());
    assert!(cx.debug_bounds("calendar-add-recurring-section").is_none());
    assert_eq!(
        cx.debug_bounds("calendar-sidebar-scroll-owner")
            .expect("details sidebar")
            .size
            .width,
        upcoming_width,
        "switching from Upcoming to item details must not resize the calendar sidebar"
    );
    let before = cx.debug_bounds("calendar-detail-content").expect("details");
    let viewport = cx
        .debug_bounds("calendar-sidebar-scroll-owner")
        .expect("viewport");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-100.))),
        ..Default::default()
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    assert!(
        cx.debug_bounds("calendar-detail-content")
            .expect("details")
            .top()
            < before.top()
    );
    cx.update(|window, cx| {
        model.update(cx, |model, cx| {
            model.state.detail_title_input.update(cx, |input, cx| {
                input.set_value("Unpublished edit", window, cx)
            });
            model.open_calendar_entry(43, window, cx);
            model.open_calendar_entry(42, window, cx);
            assert_eq!(
                model.state.detail_title_input.read(cx).value().as_ref(),
                "Unpublished edit"
            );
            assert!(model.selected_dirty(cx));
            model.save_calendar_entry(window, cx);
        })
    });
    cx.run_until_parked();
    assert!(model.read_with(&cx, |model, _| model.selected_error().is_some()));
    cx.update(|window, cx| {
        model.update(cx, |model, cx| {
            assert!(model.selected_dirty(cx));
            model.discard_entry(window, cx);
            assert!(!model.selected_dirty(cx));
            model.set_calendar_entry_lifecycle(EntryLifecycleState::Completed, window, cx);
            assert!(
                model.selected_dirty(cx),
                "Status must remain a draft until Save"
            );
            model.discard_entry(window, cx);
            assert!(!model.selected_dirty(cx));
        })
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    let events = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let recorded = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&model, move |_, event, _| {
            recorded.borrow_mut().push(event.clone())
        })
        .detach()
    });
    cx.update(|window, cx| model.update(cx, |model, cx| model.open_calendar_entry(43, window, cx)));
    assert!(
        events
            .borrow()
            .iter()
            .any(|event| matches!(event, CalendarWorkspaceEvent::Navigate(CalendarRoute::Item)))
    );
}

#[gpui_kit::test]
fn calendar_loading_does_not_shift_month_content(cx: &mut TestAppContext) {
    let service = TestCalendarService::failing();
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service.clone(), window, cx));
            model.update(cx, |model, cx| {
                model.state.month = NaiveDate::from_ymd_opt(2026, 9, 1).expect("date");
                model.state.entries = vec![entry(42)];
                cx.notify();
            });
            workspace = Some(model.clone());
            let page = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Month, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let model = workspace.expect("workspace");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.), px(600.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let before = cx
        .debug_bounds("calendar-entry-42-2026-09-12")
        .expect("calendar entry");

    cx.update(|_, cx| {
        model.update(cx, |model, cx| {
            model.state.loading = true;
            cx.notify();
        });
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let during = cx
        .debug_bounds("calendar-entry-42-2026-09-12")
        .expect("calendar entry");

    assert_eq!(
        before.top(),
        during.top(),
        "refreshing the calendar must not move existing month content"
    );
}

#[gpui_kit::test]
fn calendar_month_owns_only_vertical_scroll_gestures(cx: &mut TestAppContext) {
    let service = TestCalendarService::failing();
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service.clone(), window, cx));
            model.update(cx, |model, cx| {
                model.state.month = NaiveDate::from_ymd_opt(2026, 9, 1).expect("date");
                model.state.entries = vec![entry(42)];
                cx.notify();
            });
            let page = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Month, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.), px(420.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });

    let viewport = cx
        .debug_bounds("calendar-month-scroll-owner")
        .expect("month viewport");
    let before = cx
        .debug_bounds("calendar-entry-42-2026-09-12")
        .expect("calendar entry");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(-120.), px(0.))),
        ..Default::default()
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let after_horizontal = cx
        .debug_bounds("calendar-entry-42-2026-09-12")
        .expect("calendar entry after horizontal gesture");
    assert_eq!(
        after_horizontal.top(),
        before.top(),
        "horizontal trackpad movement must not scroll the month vertically"
    );

    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-120.))),
        ..Default::default()
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    assert!(
        cx.debug_bounds("calendar-entry-42-2026-09-12")
            .expect("calendar entry after vertical gesture")
            .top()
            < after_horizontal.top(),
        "vertical wheel movement should scroll the month"
    );
}

#[gpui_kit::test]
fn calendar_scrollbar_stays_aligned_with_its_viewport_at_bottom(cx: &mut TestAppContext) {
    let service = TestCalendarService::failing();
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service.clone(), window, cx));
            model.update(cx, |model, cx| {
                model.state.month = NaiveDate::from_ymd_opt(2026, 9, 1).expect("date");
                model.state.entries = vec![entry(42)];
                cx.notify();
            });
            let page = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Month, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(800.), px(420.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });

    let viewport = cx
        .debug_bounds("calendar-month-scroll-owner")
        .expect("month viewport");
    let before = cx.debug_bounds("scrollbar-overlay").expect("scrollbar");
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
        ..Default::default()
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let after = cx
        .debug_bounds("scrollbar-overlay")
        .expect("scrollbar after scrolling");

    assert_eq!(
        before, after,
        "the scrollbar overlay must not move with content"
    );
    assert_eq!(
        after, viewport,
        "the scrollbar should use the visible month viewport at the bottom"
    );
}

#[gpui_kit::test]
fn calendar_header_uses_one_typographic_scale(cx: &mut TestAppContext) {
    let service = TestCalendarService::in_memory(CalendarSnapshot::default());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service, window, cx));
            model.update(cx, |model, cx| {
                model.state.month = NaiveDate::from_ymd_opt(2026, 9, 1).expect("date");
                cx.notify();
            });
            let page = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Month, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1400.), px(768.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });

    let title = cx
        .debug_bounds("calendar-page-title")
        .expect("calendar title");
    let month = cx
        .debug_bounds("calendar-month-label")
        .expect("calendar month");
    assert_eq!(
        cx.debug_bounds("calendar-header")
            .expect("calendar header")
            .size
            .height,
        px(52.),
        "calendar should use the shared compact header rhythm"
    );
    assert_eq!(
        title.size.height, month.size.height,
        "page identity and calendar state should use one header type scale"
    );
}

#[gpui_kit::test]
fn empty_calendar_month_keeps_the_grid_primary_and_guidance_in_the_sidebar(
    cx: &mut TestAppContext,
) {
    let service = TestCalendarService::in_memory(CalendarSnapshot::default());
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service.clone(), window, cx));
            model.update(cx, |model, cx| {
                model.state.month = NaiveDate::from_ymd_opt(2026, 9, 1).expect("date");
                cx.notify();
            });
            workspace = Some(model.clone());
            let page = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Month, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let workspace = workspace.expect("workspace");
    cx.simulate_resize(size(px(1200.), px(600.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });

    assert!(
        cx.debug_bounds("calendar-month-empty-state").is_none(),
        "empty-month guidance should not displace the calendar grid"
    );
    assert!(
        cx.debug_bounds("calendar-up-next").is_some(),
        "the planning rail should remain useful before an item is selected"
    );
    assert!(
        cx.debug_bounds("calendar-upcoming-empty-guidance")
            .is_some(),
        "empty-month guidance should live at the bottom of Upcoming"
    );
    let before_refresh = cx
        .debug_bounds("calendar-upcoming-empty-guidance")
        .expect("empty guidance before refresh");
    cx.update(|_, cx| {
        workspace.update(cx, |model, cx| {
            model.state.loading = true;
            cx.notify();
        });
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    assert_eq!(
        cx.debug_bounds("calendar-upcoming-empty-guidance")
            .expect("empty guidance during refresh"),
        before_refresh,
        "a background refresh must not remove and reinsert the Upcoming footer"
    );
}

#[gpui_kit::test]
fn calendar_refresh_only_reports_actual_board_changes(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let service = TestCalendarService::in_memory(CalendarSnapshot::default());
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service.clone(), window, cx));
            workspace = Some(model.clone());
            let page = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Month, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let workspace = workspace.expect("workspace");
    let committed = std::rc::Rc::new(std::cell::Cell::new(false));
    let recorded = committed.clone();
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.update(|window, cx| {
        cx.subscribe(&workspace, move |_, event, _| {
            if matches!(event, CalendarWorkspaceEvent::Committed(_)) {
                recorded.set(true);
            }
        })
        .detach();
        workspace.update(cx, |model, cx| model.refresh(window, cx));
    });
    cx.run_until_parked();

    assert!(
        !committed.get(),
        "a read-only refresh should not trigger a broad board reload"
    );

    service
        .snapshot
        .lock()
        .expect("calendar test service")
        .board_changed = true;
    cx.update(|window, cx| {
        workspace.update(cx, |model, cx| model.refresh(window, cx));
    });
    cx.run_until_parked();
    assert!(
        committed.get(),
        "materialized calendar work should still refresh the owning board"
    );
}

#[gpui_kit::test]
fn recurrence_validation_stays_inside_the_form(cx: &mut TestAppContext) {
    let service = TestCalendarService::in_memory(CalendarSnapshot::default());
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service.clone(), window, cx));
            model.update(cx, |model, cx| {
                model.lists = vec![CalendarListRecord {
                    id: 7,
                    title: "Planning".into(),
                    entries: vec![CalendarListEntry {
                        id: 42,
                        title: "Calendar item".into(),
                        due_on: Some("2026-09-12".into()),
                    }],
                }];
                model.open_recurrence_form(window, cx);
            });
            workspace = Some(model.clone());
            let page = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Recurring, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let workspace = workspace.expect("workspace");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(1200.), px(600.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let before = cx
        .debug_bounds("calendar-add-recurring-section")
        .expect("recurrence form");

    cx.update(|window, cx| {
        workspace.update(cx, |model, cx| {
            model.create_recurring_task_from_form(window, cx)
        });
        window.draw(cx).clear(cx);
    });

    assert_eq!(
        cx.debug_bounds("calendar-add-recurring-section")
            .expect("recurrence form after validation")
            .top(),
        before.top(),
        "form validation must not insert a page-level row and shift the workspace"
    );
    assert!(
        cx.debug_bounds("calendar-recurrence-error").is_some(),
        "the validation message should appear beside the recurrence fields"
    );
}

#[gpui_kit::test]
fn recurring_view_form_scrolls_at_small_height(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let _guard = runtime.enter();
    let service = TestCalendarService::in_memory(CalendarSnapshot::default());
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service.clone(), window, cx));
            model.update(cx, |model, cx| {
                model.state.recurrence_form_open = true;
                cx.notify();
            });
            let page = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Recurring, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    cx.simulate_resize(size(px(800.), px(360.)));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let before = cx
        .debug_bounds("calendar-add-recurring-section")
        .expect("form");
    let task_list = cx
        .debug_bounds("recurring-task-list")
        .expect("schedule list");
    let form_column = cx
        .debug_bounds("recurring-form-column")
        .expect("recurrence form column");
    assert!(
        form_column.bottom() < task_list.top(),
        "compact recurring-task views should stack the form before the schedule list"
    );
    cx.simulate_event(ScrollWheelEvent {
        position: point(px(250.), px(220.)),
        delta: ScrollDelta::Pixels(point(px(0.), px(-120.))),
        ..Default::default()
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    assert!(
        cx.debug_bounds("calendar-add-recurring-section")
            .expect("form")
            .top()
            < before.top()
    );
}

#[cfg(target_os = "windows")]
#[test]
fn calendar_pages_fit_windows_stack() {
    for test in [
        "tests::calendar_click_scroll_and_drafts",
        "tests::calendar_month_owns_only_vertical_scroll_gestures",
        "tests::recurring_view_form_scrolls_at_small_height",
    ] {
        let output = std::process::Command::new(std::env::current_exe().expect("executable"))
            .args(["--exact", test, "--nocapture"])
            .env("RUST_MIN_STACK", "1048576")
            .output()
            .expect("launch");
        assert!(
            output.status.success(),
            "{test}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[gpui_kit::test]
fn calendar_save_and_recurrence_complete_after_selection_changes(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let first_id = 42;
    let second_id = 43;
    let service = TestCalendarService::in_memory(CalendarSnapshot {
        entries: vec![entry(first_id), entry(second_id)],
        recurring: Vec::new(),
        lists: vec![CalendarListRecord {
            id: 7,
            title: "Planning".into(),
            entries: vec![
                CalendarListEntry {
                    id: first_id as u32,
                    title: "Calendar item 42".into(),
                    due_on: Some("2026-09-12".into()),
                },
                CalendarListEntry {
                    id: second_id as u32,
                    title: "Calendar item 43".into(),
                    due_on: Some("2026-09-12".into()),
                },
            ],
        }],
        board_changed: false,
    });
    let service_state = service.clone();
    let mut workspace = None;
    let window = cx.update(|cx| {
        gpui_kit::init(cx);
        cx.open_window(Default::default(), |window, cx| {
            let model = cx.new(|cx| CalendarWorkspace::new(7, service.clone(), window, cx));
            model.update(cx, |model, cx| {
                model.state.month = NaiveDate::from_ymd_opt(2026, 9, 1).expect("date");
                model.refresh(window, cx);
            });
            workspace = Some(model.clone());
            let page = cx.new(|cx| CalendarPage::new(model, CalendarRoute::Month, cx));
            cx.new(|cx| gpui_kit::component::Root::new(page, window, cx))
        })
        .expect("window")
    });
    let model = workspace.expect("model");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let wait = |cx: &mut VisualTestContext| {
        for _ in 0..1000 {
            cx.run_until_parked();
            if model.read_with(cx, |model, _| {
                !model.state.loading
                    && !model.state.creating
                    && model.drafts.values().all(|draft| !draft.saving)
            }) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("calendar request did not finish");
    };
    wait(&mut cx);
    cx.update(|window, cx| {
        model.update(cx, |model, cx| {
            model.open_calendar_entry(first_id, window, cx);
            model
                .state
                .detail_title_input
                .update(cx, |input, cx| input.set_value("Saved title", window, cx));
            model.save_calendar_entry(window, cx);
            model
                .state
                .detail_title_input
                .update(cx, |input, cx| input.set_value("Newer draft", window, cx));
            model.open_calendar_entry(second_id, window, cx);
        })
    });
    wait(&mut cx);
    cx.update(|window, cx| {
        model.update(cx, |model, cx| {
            assert_eq!(model.state.selected_entry_id, Some(second_id));
            model.open_calendar_entry(first_id, window, cx);
            assert_eq!(
                model.state.detail_title_input.read(cx).value().as_ref(),
                "Newer draft"
            );
            assert!(model.selected_dirty(cx));
            assert_eq!(model.drafts[&first_id].baseline.0, "Saved title");
            model.open_recurrence_for_entry(second_id, window, cx);
            model.state.recurrence_rule_input.update(cx, |input, cx| {
                input.set_value("every week on mon", window, cx)
            });
            model.create_recurring_task_from_form(window, cx);
            model.open_calendar_entry(first_id, window, cx);
        })
    });
    wait(&mut cx);
    model.read_with(&cx, |model, _| {
        assert!(model.state.error.is_none(), "{:?}", model.state.error);
        assert_eq!(model.state.recurring.len(), 1);
    });
    let snapshot = service_state.snapshot.lock().expect("service state");
    assert!(
        snapshot
            .entries
            .iter()
            .any(|entry| entry.entry_id == first_id && entry.title == "Saved title")
    );
    assert_eq!(snapshot.recurring.len(), 1);
}
