use std::rc::Rc;

use anyhow::{Result, anyhow};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, IndexPath, Sizable as _,
    button::{Button, ButtonVariants as _},
    calendar::Date,
    date_picker::{DatePicker, DatePickerEvent, DatePickerState},
    h_flex,
    input::{
        Escape as InputEscape, IndentInline, InputEvent, OutdentInline, Textarea, TextareaState,
    },
    searchable_list::{SearchableListItem, SearchableVec},
    select::{Select, SelectEvent, SelectState},
    v_flex,
};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, Global, InteractiveElement as _,
    IntoElement, KeyBinding, MouseButton, ParentElement as _, Render, SharedString, Styled as _,
    Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
    actions, div, prelude::FluentBuilder as _, px, size,
};
use runtime::AppRuntime;
use storage::{
    MutationOrigin,
    workspace::api::{BoardSummary, CreateInboxTaskInput, CreateNoteInput, ProjectSummary},
};

const QUICK_CAPTURE_WIDTH: f32 = 560.0;
const QUICK_CAPTURE_HEIGHT: f32 = 420.0;
const QUICK_CAPTURE_MIN_WIDTH: f32 = 440.0;
const QUICK_CAPTURE_MIN_HEIGHT: f32 = 380.0;
const MAX_TITLE_CHARS: usize = 80;
const QUICK_CAPTURE_KEY_CONTEXT: &str = "QuickCapture";

actions!(
    quick_capture,
    [FocusNextInput, FocusPreviousInput, ToggleCaptureKind]
);

struct QuickCaptureKeyBindings;

impl Global for QuickCaptureKeyBindings {}

pub type CaptureSavedHandler = Rc<dyn Fn(&mut App)>;
pub type WindowVisibilityHandler = Rc<dyn Fn(&Window, bool)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CaptureKind {
    Note,
    Task,
}

enum CaptureInput {
    Note(CreateNoteInput),
    Task(CreateInboxTaskInput),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProjectChoice {
    id: Option<i64>,
    label: SharedString,
}

impl SearchableListItem for ProjectChoice {
    type Value = Option<i64>;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }

    fn matches(&self, query: &str) -> bool {
        self.label.to_lowercase().contains(&query.to_lowercase())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BoardChoice {
    id: Option<i64>,
    label: SharedString,
}

impl SearchableListItem for BoardChoice {
    type Value = Option<i64>;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.id
    }

    fn matches(&self, query: &str) -> bool {
        self.label.to_lowercase().contains(&query.to_lowercase())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CapturePhase {
    Ready,
    Saving,
}

pub struct QuickCaptureView {
    textarea: Entity<TextareaState>,
    project_select: Entity<SelectState<SearchableVec<ProjectChoice>>>,
    board_select: Entity<SelectState<SearchableVec<BoardChoice>>>,
    due_date_picker: Entity<DatePickerState>,
    projects: Vec<ProjectSummary>,
    boards: Vec<BoardSummary>,
    selected_project_id: Option<i64>,
    workspace_loading: bool,
    workspace_load_error: Option<SharedString>,
    capture_kind: CaptureKind,
    phase: CapturePhase,
    has_content: bool,
    due_on: Option<String>,
    error: Option<SharedString>,
    capture_saved: CaptureSavedHandler,
    set_window_visible: WindowVisibilityHandler,
}

impl QuickCaptureView {
    fn new(
        window: &mut Window,
        capture_saved: CaptureSavedHandler,
        set_window_visible: WindowVisibilityHandler,
        cx: &mut Context<Self>,
    ) -> Self {
        if !cx.has_global::<QuickCaptureKeyBindings>() {
            cx.set_global(QuickCaptureKeyBindings);
            cx.bind_keys([KeyBinding::new(
                "ctrl-tab",
                ToggleCaptureKind,
                Some(QUICK_CAPTURE_KEY_CONTEXT),
            )]);
            cx.bind_keys([
                KeyBinding::new("tab", FocusNextInput, Some(QUICK_CAPTURE_KEY_CONTEXT)),
                KeyBinding::new(
                    "shift-tab",
                    FocusPreviousInput,
                    Some(QUICK_CAPTURE_KEY_CONTEXT),
                ),
            ]);
        }

        let textarea = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(4, 10)
                .submit_on_enter(true)
                .placeholder("Write a note…")
        });
        let project_select = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(project_choices(&[])),
                Some(IndexPath::default().row(0)),
                window,
                cx,
            )
            .searchable(true)
        });
        let board_select = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(board_choices(&[])),
                Some(IndexPath::default().row(0)),
                window,
                cx,
            )
            .searchable(true)
        });
        let due_date_picker = cx.new(|cx| DatePickerState::new(window, cx));

        cx.subscribe_in(
            &textarea,
            window,
            |this, textarea, event: &InputEvent, window, cx| match event {
                InputEvent::Change => {
                    this.has_content = !textarea.read(cx).text().to_string().trim().is_empty();
                    this.error = None;
                    cx.notify();
                }
                InputEvent::PressEnter {
                    secondary: false,
                    shift: false,
                } => this.save(window, cx),
                _ => {}
            },
        )
        .detach();

        cx.subscribe_in(
            &project_select,
            window,
            |this, _, event: &SelectEvent<SearchableVec<ProjectChoice>>, window, cx| {
                let SelectEvent::Confirm(project_id) = event;
                this.selected_project_id = project_id.flatten();
                this.update_board_options(window, cx);
                this.error = None;
                cx.notify();
            },
        )
        .detach();

        cx.subscribe_in(
            &due_date_picker,
            window,
            |this, _, event: &DatePickerEvent, _, cx| {
                if let DatePickerEvent::Change(Date::Single(date)) = event {
                    this.due_on = date.map(|date| date.format("%Y-%m-%d").to_string());
                    this.error = None;
                    cx.notify();
                }
            },
        )
        .detach();

        window.set_window_title("Quick Capture");
        let mut this = Self {
            textarea,
            project_select,
            board_select,
            due_date_picker,
            projects: Vec::new(),
            boards: Vec::new(),
            selected_project_id: None,
            workspace_loading: true,
            workspace_load_error: None,
            capture_kind: CaptureKind::Note,
            phase: CapturePhase::Ready,
            has_content: false,
            due_on: None,
            error: None,
            capture_saved,
            set_window_visible,
        };
        this.load_workspace_options(window, cx);
        this
    }

    pub fn present(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.phase == CapturePhase::Saving {
            return;
        }
        self.reset_capture(window, cx);
        if !self.workspace_loading {
            self.load_workspace_options(window, cx);
        }
        cx.activate(true);
        window.activate_window();
        self.textarea
            .update(cx, |textarea, cx| textarea.focus(window, cx));
        cx.notify();
    }

    fn load_workspace_options(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.workspace_loading = true;
        self.workspace_load_error = None;
        let task = cx.global::<AppRuntime>().spawn_store(
            cx.background_executor(),
            move |store| async move {
                let projects = store.list_projects().await?;
                let boards = store.list_boards(None).await?;
                Ok::<_, anyhow::Error>((projects, boards))
            },
        );

        cx.spawn_in(window, async move |this, window| {
            let result = match task.await {
                Ok(result) => result,
                Err(error) => Err(anyhow!("storage task failed: {error}")),
            };
            window
                .update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.finish_workspace_load(result, window, cx)
                    })
                    .ok();
                })
                .ok();
        })
        .detach();
    }

    fn finish_workspace_load(
        &mut self,
        result: Result<(Vec<ProjectSummary>, Vec<BoardSummary>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.workspace_loading = false;
        match result {
            Ok((projects, boards)) => {
                self.projects = projects;
                self.boards = boards;
                self.workspace_load_error = None;
                self.project_select.update(cx, |select, cx| {
                    select.set_items(
                        SearchableVec::new(project_choices(&self.projects)),
                        window,
                        cx,
                    );
                    select.set_selected_value(&None, window, cx);
                });
                self.update_board_options(window, cx);
            }
            Err(error) => {
                self.workspace_load_error = Some(
                    format!("Could not load optional project and board choices: {error}").into(),
                );
            }
        }
        cx.notify();
    }

    fn update_board_options(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let choices = board_choices(
            &self
                .boards
                .iter()
                .filter(|board| {
                    self.selected_project_id
                        .is_none_or(|project_id| board.project_id == Some(project_id))
                })
                .cloned()
                .collect::<Vec<_>>(),
        );
        self.board_select.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(choices), window, cx);
            select.set_selected_value(&None, window, cx);
        });
    }

    fn set_capture_kind(&mut self, kind: CaptureKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.phase == CapturePhase::Saving || self.capture_kind == kind {
            return;
        }
        self.capture_kind = kind;
        self.error = None;
        let placeholder = match kind {
            CaptureKind::Note => "Write a note…",
            CaptureKind::Task => "What needs to be done?",
        };
        self.textarea.update(cx, |textarea, cx| {
            textarea.set_placeholder(placeholder, window, cx);
        });
        cx.notify();
    }

    fn select_capture_kind(
        &mut self,
        kind: CaptureKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.phase == CapturePhase::Saving {
            return;
        }

        self.set_capture_kind(kind, window, cx);
        if kind == CaptureKind::Task && !self.workspace_loading {
            self.project_select
                .update(cx, |select, cx| select.focus(window, cx));
        } else {
            self.textarea
                .update(cx, |textarea, cx| textarea.focus(window, cx));
        }
    }

    fn close(&mut self, window: &mut Window, _: &mut Context<Self>) {
        if self.phase == CapturePhase::Saving {
            return;
        }
        (self.set_window_visible)(window, false);
    }

    fn reset_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.phase = CapturePhase::Ready;
        self.capture_kind = CaptureKind::Note;
        self.textarea.update(cx, |textarea, cx| {
            textarea.set_placeholder("Write a note…", window, cx);
        });
        self.selected_project_id = None;
        self.has_content = false;
        self.due_on = None;
        self.error = None;
        self.textarea.update(cx, |textarea, cx| {
            textarea.set_value("", window, cx);
        });
        self.project_select.update(cx, |select, cx| {
            select.set_selected_value(&None, window, cx);
        });
        self.update_board_options(window, cx);
        self.due_date_picker.update(cx, |picker, cx| {
            picker.set_date(Date::Single(None), window, cx);
        });
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.phase == CapturePhase::Saving {
            return;
        }

        let content = self.textarea.read(cx).value().to_string();
        let project_id = self
            .project_select
            .read(cx)
            .selected_value()
            .copied()
            .flatten();
        let board_id = self
            .board_select
            .read(cx)
            .selected_value()
            .copied()
            .flatten();
        let input = match self.capture_kind {
            CaptureKind::Note => capture_note_input(&content, project_id).map(CaptureInput::Note),
            CaptureKind::Task => {
                capture_task_input(&content, project_id, board_id, self.due_on.clone())
                    .map(CaptureInput::Task)
            }
        };
        let Some(input) = input else {
            return;
        };

        let item_label = match &input {
            CaptureInput::Note(_) => "note",
            CaptureInput::Task(_) => "task",
        };
        self.phase = CapturePhase::Saving;
        self.error = None;
        cx.notify();

        let task = cx.global::<AppRuntime>().spawn_store(
            cx.background_executor(),
            move |store| async move {
                let mutations = store.mutations(MutationOrigin::LocalApp);
                match input {
                    CaptureInput::Note(input) => mutations.create_note(input).await.map(|_| ()),
                    CaptureInput::Task(input) => {
                        mutations.create_inbox_task(input).await.map(|_| ())
                    }
                }
            },
        );

        cx.spawn_in(window, async move |this, window| {
            let result = match task.await {
                Ok(result) => result,
                Err(error) => Err(anyhow!("storage task failed: {error}")),
            };
            window
                .update(|window, cx| {
                    this.update(cx, |this, cx| {
                        this.finish_save(result, item_label, window, cx)
                    })
                    .ok();
                })
                .ok();
        })
        .detach();
    }

    fn finish_save(
        &mut self,
        result: Result<()>,
        item_label: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => {
                self.reset_capture(window, cx);
                (self.set_window_visible)(window, false);
                (self.capture_saved)(cx);
            }
            Err(error) => {
                self.phase = CapturePhase::Ready;
                self.error = Some(format!("Could not save {item_label}: {error}").into());
                cx.notify();
            }
        }
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let saving = self.phase == CapturePhase::Saving;
        h_flex()
            .items_center()
            .gap_3()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .on_mouse_down(MouseButton::Left, |_event, window, _cx| {
                        window.start_window_move();
                    })
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child("Quick Capture"),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .rounded(theme.radius)
                    .bg(theme.secondary)
                    .p_1()
                    .child(
                        Button::new("quick-capture-note-type")
                            .label("Note")
                            .small()
                            .tab_stop(false)
                            .tooltip("Switch type with Ctrl+Tab")
                            .disabled(saving)
                            .when(self.capture_kind == CaptureKind::Note, |button| {
                                button.primary()
                            })
                            .when(self.capture_kind != CaptureKind::Note, |button| {
                                button.ghost()
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.select_capture_kind(CaptureKind::Note, window, cx)
                            })),
                    )
                    .child(
                        Button::new("quick-capture-task-type")
                            .label("Task")
                            .small()
                            .tab_stop(false)
                            .tooltip("Switch type with Ctrl+Tab")
                            .disabled(saving)
                            .when(self.capture_kind == CaptureKind::Task, |button| {
                                button.primary()
                            })
                            .when(self.capture_kind != CaptureKind::Task, |button| {
                                button.ghost()
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.select_capture_kind(CaptureKind::Task, window, cx)
                            })),
                    ),
            )
            .child(
                Button::new("quick-capture-close")
                    .icon(IconName::Close)
                    .ghost()
                    .xsmall()
                    .tab_stop(false)
                    .tooltip("Close · Esc")
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
    }

    fn render_task_options(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let saving = self.phase == CapturePhase::Saving;
        let theme = cx.theme().clone();
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("Project"),
                                )
                                .child(
                                    Select::new(&self.project_select)
                                        .id("quick-capture-project")
                                        .placeholder("No project")
                                        .accessibility_label("Project")
                                        .search_placeholder("Search projects")
                                        .menu_max_h(px(220.))
                                        .disabled(saving || self.workspace_loading)
                                        .small()
                                        .w_full(),
                                ),
                        ),
                    )
                    .child(
                        div().flex_1().min_w_0().child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("Save to"),
                                )
                                .child(
                                    Select::new(&self.board_select)
                                        .id("quick-capture-board")
                                        .placeholder("Inbox")
                                        .accessibility_label("Save to")
                                        .search_placeholder("Search boards")
                                        .menu_max_h(px(220.))
                                        .disabled(saving || self.workspace_loading)
                                        .small()
                                        .w_full(),
                                ),
                        ),
                    )
                    .child(
                        div().w(px(140.)).flex_shrink_0().child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("Due date"),
                                )
                                .child(
                                    DatePicker::new(&self.due_date_picker)
                                        .placeholder("Optional")
                                        .cleanable(true)
                                        .disabled(saving)
                                        .small(),
                                ),
                        ),
                    ),
            )
            .when_some(self.workspace_load_error.clone(), |this, error| {
                this.child(div().text_xs().text_color(theme.danger).child(error))
            })
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let saving = self.phase == CapturePhase::Saving;
        let error = self.error.clone();
        let footer_message = error.clone().unwrap_or_else(|| {
            "Tab moves between fields · Shift+Tab goes back · Ctrl+Tab switches type".into()
        });
        let save_label = match (saving, self.capture_kind) {
            (true, _) => "Saving…",
            (false, CaptureKind::Note) => "Save note",
            (false, CaptureKind::Task) => "Save task",
        };
        h_flex()
            .items_center()
            .justify_between()
            .gap_3()
            .px_5()
            .py_3()
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_xs()
                    .text_color(error.map_or(theme.muted_foreground, |_| theme.danger))
                    .child(footer_message),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("quick-capture-cancel")
                            .label("Cancel")
                            .ghost()
                            .small()
                            .disabled(saving)
                            .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
                    )
                    .child(
                        Button::new("quick-capture-save")
                            .label(save_label)
                            .primary()
                            .small()
                            .loading(saving)
                            .disabled(saving || !self.has_content)
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}

impl Render for QuickCaptureView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let textarea_focused = self.textarea.read(cx).focus_handle(cx).is_focused(window);
        let textarea_label = match self.capture_kind {
            CaptureKind::Note => "Note content",
            CaptureKind::Task => "Task title and details",
        };
        let textarea = Textarea::new(&self.textarea)
            .appearance(false)
            .bordered(false)
            .h_full()
            .aria_label(textarea_label)
            .text_color(theme.popover_foreground);

        v_flex()
            .id("quick-capture")
            .size_full()
            .overflow_hidden()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_lg()
            .on_action(cx.listener(|this, _: &InputEscape, window, cx| {
                this.close(window, cx);
            }))
            .key_context(QUICK_CAPTURE_KEY_CONTEXT)
            .on_action(cx.listener(|_, _: &FocusNextInput, window, cx| {
                cx.stop_propagation();
                window.focus_next(cx);
            }))
            .on_action(cx.listener(|_, _: &FocusPreviousInput, window, cx| {
                cx.stop_propagation();
                window.focus_prev(cx);
            }))
            .on_action(cx.listener(|_, _: &IndentInline, window, cx| {
                cx.stop_propagation();
                window.focus_next(cx);
            }))
            .on_action(cx.listener(|_, _: &OutdentInline, window, cx| {
                cx.stop_propagation();
                window.focus_prev(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleCaptureKind, window, cx| {
                let next_kind = match this.capture_kind {
                    CaptureKind::Note => CaptureKind::Task,
                    CaptureKind::Task => CaptureKind::Note,
                };
                this.select_capture_kind(next_kind, window, cx);
            }))
            .child(self.render_header(cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_3()
                    .px_5()
                    .py_3()
                    .when(self.capture_kind == CaptureKind::Task, |this| {
                        this.child(self.render_task_options(cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .rounded(theme.radius)
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.background)
                            .p_2()
                            .when(textarea_focused, |this| this.border_color(theme.ring))
                            .child(textarea),
                    ),
            )
            .child(self.render_footer(cx))
    }
}

pub fn open_window(
    capture_saved: CaptureSavedHandler,
    set_window_visible: WindowVisibilityHandler,
    cx: &mut App,
) -> Result<gpui_kit::WindowHandle<QuickCaptureView>> {
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::centered(
                size(px(QUICK_CAPTURE_WIDTH), px(QUICK_CAPTURE_HEIGHT)),
                cx,
            )),
            titlebar: None,
            focus: false,
            show: false,
            kind: WindowKind::Floating,
            is_movable: true,
            app_owns_titlebar_drag: true,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Opaque,
            window_min_size: Some(size(
                px(QUICK_CAPTURE_MIN_WIDTH),
                px(QUICK_CAPTURE_MIN_HEIGHT),
            )),
            window_decorations: Some(WindowDecorations::Client),
            ..Default::default()
        },
        move |window, cx| {
            cx.new(|cx| QuickCaptureView::new(window, capture_saved, set_window_visible, cx))
        },
    )
}

fn project_choices(projects: &[ProjectSummary]) -> Vec<ProjectChoice> {
    std::iter::once(ProjectChoice {
        id: None,
        label: "No project".into(),
    })
    .chain(projects.iter().map(|project| ProjectChoice {
        id: Some(project.id),
        label: project.name.clone().into(),
    }))
    .collect()
}

fn board_choices(boards: &[BoardSummary]) -> Vec<BoardChoice> {
    std::iter::once(BoardChoice {
        id: None,
        label: "Inbox".into(),
    })
    .chain(boards.iter().map(|board| {
        BoardChoice {
            id: Some(board.id),
            label: board
                .project_name
                .as_ref()
                .map(|project| format!("{project} / {}", board.title))
                .unwrap_or_else(|| board.title.clone())
                .into(),
        }
    }))
    .collect()
}

#[cfg(test)]
fn capture_input(content: &str) -> Option<CreateNoteInput> {
    capture_note_input(content, None)
}

fn capture_note_input(content: &str, project_id: Option<i64>) -> Option<CreateNoteInput> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return None;
    }

    Some(CreateNoteInput {
        title: note_title(&content),
        content,
        project_id,
    })
}

fn capture_task_input(
    content: &str,
    project_id: Option<i64>,
    board_id: Option<i64>,
    due_on: Option<String>,
) -> Option<CreateInboxTaskInput> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    let title_line = content.lines().position(|line| !line.trim().is_empty())?;
    let description = content
        .lines()
        .skip(title_line + 1)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    Some(CreateInboxTaskInput {
        title: note_title(content),
        description,
        project_id,
        board_id,
        due_on,
    })
}

fn note_title(content: &str) -> String {
    let title = match content.lines().find(|line| !line.trim().is_empty()) {
        Some(line) => line.trim().trim_start_matches('#').trim(),
        None => "Quick capture",
    };
    let title: String = title.chars().take(MAX_TITLE_CHARS).collect();
    if title.is_empty() {
        "Quick capture".to_string()
    } else {
        title
    }
}

#[cfg(test)]
mod tests {
    use super::{capture_input, note_title};

    #[test]
    fn capture_input_rejects_blank_content() {
        assert!(capture_input(" \n\t ").is_none());
    }

    #[test]
    fn capture_input_derives_a_short_title_and_preserves_markdown() {
        let input = capture_input("  # Ship the release\n\n- Verify the installer  ")
            .expect("non-empty capture should produce a note");

        assert_eq!(input.title, "Ship the release");
        assert_eq!(
            input.content,
            "# Ship the release\n\n- Verify the installer"
        );
        assert!(input.project_id.is_none());
    }

    #[test]
    fn note_title_falls_back_for_empty_lines() {
        assert_eq!(note_title("\n\n"), "Quick capture");
    }
}
