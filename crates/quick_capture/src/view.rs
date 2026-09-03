use std::rc::Rc;

use anyhow::{Result, anyhow};
use gpui::{
    App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, Render, SharedString, Styled as _, Window, WindowBackgroundAppearance,
    WindowBounds, WindowDecorations, WindowKind, WindowOptions, div, px, size,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Escape as InputEscape, InputEvent, Textarea, TextareaState},
    v_flex,
};
use runtime::AppRuntime;
use storage::{
    MutationOrigin,
    workspace::api::{CreateNoteInput, NoteDetail},
};

const QUICK_CAPTURE_WIDTH: f32 = 560.0;
const QUICK_CAPTURE_HEIGHT: f32 = 360.0;
const QUICK_CAPTURE_MIN_WIDTH: f32 = 440.0;
const QUICK_CAPTURE_MIN_HEIGHT: f32 = 280.0;
const MAX_NOTE_TITLE_CHARS: usize = 80;

pub type NoteCreatedHandler = Rc<dyn Fn(&mut App)>;
pub type WindowVisibilityHandler = Rc<dyn Fn(&Window, bool)>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CapturePhase {
    Ready,
    Saving,
}

pub struct QuickCaptureView {
    textarea: Entity<TextareaState>,
    phase: CapturePhase,
    has_content: bool,
    error: Option<SharedString>,
    note_created: NoteCreatedHandler,
    set_window_visible: WindowVisibilityHandler,
}

impl QuickCaptureView {
    fn new(
        window: &mut Window,
        note_created: NoteCreatedHandler,
        set_window_visible: WindowVisibilityHandler,
        cx: &mut Context<Self>,
    ) -> Self {
        let textarea = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(4, 10)
                .submit_on_enter(true)
                .placeholder("Write a note…")
        });
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

        window.set_window_title("Quick Capture");
        Self {
            textarea,
            phase: CapturePhase::Ready,
            has_content: false,
            error: None,
            note_created,
            set_window_visible,
        }
    }

    pub fn present(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.phase == CapturePhase::Saving {
            return;
        }
        self.reset_capture(window, cx);
        cx.activate(true);
        window.activate_window();
        self.textarea
            .update(cx, |textarea, cx| textarea.focus(window, cx));
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, _: &mut Context<Self>) {
        if self.phase == CapturePhase::Saving {
            return;
        }
        (self.set_window_visible)(window, false);
    }

    fn reset_capture(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.phase = CapturePhase::Ready;
        self.has_content = false;
        self.error = None;
        self.textarea.update(cx, |textarea, cx| {
            textarea.set_value("", window, cx);
        });
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.phase == CapturePhase::Saving {
            return;
        }

        let content = self.textarea.read(cx).value().to_string();
        let Some(input) = capture_input(&content) else {
            return;
        };

        self.phase = CapturePhase::Saving;
        self.error = None;
        cx.notify();

        let task = cx
            .global::<AppRuntime>()
            .spawn_store(move |store| async move {
                store
                    .mutations(MutationOrigin::LocalApp)
                    .create_note(input)
                    .await
            });

        cx.spawn_in(window, async move |this, window| {
            let result = match task.await {
                Ok(result) => result,
                Err(error) => Err(anyhow!("storage task failed: {error}")),
            };
            window
                .update(|window, cx| {
                    this.update(cx, |this, cx| this.finish_save(result, window, cx))
                        .ok();
                })
                .ok();
        })
        .detach();
    }

    fn finish_save(
        &mut self,
        result: Result<NoteDetail>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(_) => {
                self.reset_capture(window, cx);
                (self.set_window_visible)(window, false);
                (self.note_created)(cx);
            }
            Err(error) => {
                self.phase = CapturePhase::Ready;
                self.error = Some(format!("Could not save note: {error}").into());
                cx.notify();
            }
        }
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
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
                        v_flex()
                            .gap_0()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child("Quick Capture"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("A note in one keystroke"),
                            ),
                    ),
            )
            .child(
                Button::new("quick-capture-close")
                    .icon(IconName::Close)
                    .ghost()
                    .xsmall()
                    .tooltip("Close")
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            )
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let saving = self.phase == CapturePhase::Saving;
        let error = self.error.clone();
        let footer_message = error
            .clone()
            .unwrap_or_else(|| "Enter to save · Shift+Enter for a new line · Esc to close".into());
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
                            .label(if saving { "Saving…" } else { "Save note" })
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let textarea = Textarea::new(&self.textarea)
            .appearance(false)
            .bordered(false)
            .h(px(148.))
            .text_color(theme.popover_foreground);

        div()
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
            .child(self.render_header(cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_2()
                    .px_5()
                    .py_4()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("What’s on your mind?"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .rounded(theme.radius)
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.background)
                            .p_2()
                            .child(textarea),
                    ),
            )
            .child(self.render_footer(cx))
    }
}

pub fn open_window(
    note_created: NoteCreatedHandler,
    set_window_visible: WindowVisibilityHandler,
    cx: &mut App,
) -> Result<gpui::WindowHandle<QuickCaptureView>> {
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
            cx.new(|cx| QuickCaptureView::new(window, note_created, set_window_visible, cx))
        },
    )
}

fn capture_input(content: &str) -> Option<CreateNoteInput> {
    let content = content.trim().to_string();
    if content.is_empty() {
        return None;
    }

    Some(CreateNoteInput {
        title: note_title(&content),
        content,
        project_id: None,
    })
}

fn note_title(content: &str) -> String {
    let title = match content.lines().find(|line| !line.trim().is_empty()) {
        Some(line) => line.trim().trim_start_matches('#').trim(),
        None => "Quick capture",
    };
    let title: String = title.chars().take(MAX_NOTE_TITLE_CHARS).collect();
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
