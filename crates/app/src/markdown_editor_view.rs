use gpui::{
    Action, App, AppContext, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyBinding, ParentElement, PathPromptOptions, Render, SharedString, Styled,
    Subscription, Task, Window, actions, div, px,
};
use gpui_component::{
    ActiveTheme as _, IconName, Selectable as _, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    input::{Input, InputEvent, InputState, TabSize},
    resizable::{h_resizable, resizable_panel},
    text::{TextView, TextViewState},
    v_flex,
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

actions!(
    markdown_editor,
    [
        NewMarkdownNote,
        OpenMarkdownFile,
        SaveMarkdownFile,
        SaveMarkdownFileAs,
        ToggleEditorMode,
    ]
);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = markdown_editor, no_json)]
struct ApplyMarkdownFormat(MarkdownFormat);

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
enum MarkdownFormat {
    HeadingOne,
    HeadingTwo,
    HeadingThree,
    Bold,
    Italic,
    InlineCode,
    Link,
    BulletList,
    OrderedList,
    Quote,
    CodeBlock,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorMode {
    Split,
    Source,
    Preview,
}

#[derive(Clone, PartialEq, Eq)]
enum SaveState {
    Saved,
    Dirty,
    Saving,
    Error(SharedString),
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct DocumentStats {
    lines: usize,
    words: usize,
    characters: usize,
}

pub(crate) struct MarkdownEditorView {
    focus_handle: FocusHandle,
    editor: Entity<InputState>,
    title_input: Entity<InputState>,
    preview: Entity<TextViewState>,
    mode: EditorMode,
    current_path: Option<PathBuf>,
    last_saved: SharedString,
    save_state: SaveState,
    stats: DocumentStats,
    auto_save_epoch: u64,
    _auto_save_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

const DEFAULT_NOTE: &str = r#"# Untitled note

Start writing Markdown here.

- Use the toolbar for common Markdown snippets.
- Switch between **Split**, **Source**, and **Preview** modes.
- Save to a `.md` file and the editor will auto-save after edits.

```rust
fn main() {
    println!("hello markdown");
}
```
"#;

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-n", NewMarkdownNote, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-n", NewMarkdownNote, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-o", OpenMarkdownFile, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-o", OpenMarkdownFile, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-s", SaveMarkdownFile, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-s", SaveMarkdownFile, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-s", SaveMarkdownFileAs, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-s", SaveMarkdownFileAs, Some("MarkdownEditor")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-e", ToggleEditorMode, Some("MarkdownEditor")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-e", ToggleEditorMode, Some("MarkdownEditor")),
    ]);
}

impl MarkdownEditorView {
    pub(crate) fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            InputState::new(window, cx)
                .code_editor("markdown")
                .line_number(true)
                .tab_size(TabSize {
                    tab_size: 2,
                    ..Default::default()
                })
                .soft_wrap(true)
                .searchable(true)
                .placeholder("Write Markdown...")
                .default_value(DEFAULT_NOTE)
        });

        let title_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Untitled note")
                .default_value("Untitled note")
        });

        let preview = cx.new(|cx| {
            TextViewState::markdown(DEFAULT_NOTE, cx)
                .scrollable(true)
                .selectable(true)
        });

        let focus_handle = cx.focus_handle();
        let editor_focus = editor.focus_handle(cx);
        window.defer(cx, move |window, cx| {
            editor_focus.focus(window, cx);
        });

        let _subscriptions = vec![
            cx.subscribe(&editor, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = input.read(cx).value();
                    this.update_from_editor(value, cx);
                }
            }),
            cx.subscribe(&title_input, |this, input, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && this.current_path.is_none() {
                    let name = input.read(cx).value();
                    let title = name.trim();
                    if !title.is_empty() {
                        this.save_state = SaveState::Dirty;
                        cx.notify();
                    }
                }
            }),
        ];

        Self {
            focus_handle,
            editor,
            title_input,
            preview,
            mode: EditorMode::Split,
            current_path: None,
            last_saved: DEFAULT_NOTE.into(),
            save_state: SaveState::Saved,
            stats: DocumentStats::from_text(DEFAULT_NOTE),
            auto_save_epoch: 0,
            _auto_save_task: None,
            _subscriptions,
        }
    }

    fn update_from_editor(&mut self, value: SharedString, cx: &mut Context<Self>) {
        self.preview.update(cx, |preview, cx| {
            preview.set_text(value.as_ref(), cx);
        });
        self.stats = DocumentStats::from_text(value.as_ref());
        self.save_state = if value == self.last_saved {
            SaveState::Saved
        } else {
            SaveState::Dirty
        };
        self.schedule_auto_save(cx);
        cx.notify();
    }

    fn schedule_auto_save(&mut self, cx: &mut Context<Self>) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        let epoch = self.auto_save_epoch;

        self._auto_save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(900))
                .await;

            let save_request = this
                .update(cx, |this, cx| {
                    if this.auto_save_epoch != epoch || !matches!(this.save_state, SaveState::Dirty)
                    {
                        return None;
                    }

                    let path = this.current_path.clone()?;
                    let content = this.editor.read(cx).value();
                    this.save_state = SaveState::Saving;
                    cx.notify();
                    Some((path, content))
                })
                .ok()
                .flatten();

            let Some((path, content)) = save_request else {
                return;
            };

            let write_content = content.to_string();
            let result = std::fs::write(&path, write_content).map_err(|err| err.to_string());

            this.update(cx, |this, cx| {
                if this.auto_save_epoch == epoch {
                    this.finish_save(path, content, result, cx);
                }
            })
            .ok();
        }));
    }

    fn finish_save(
        &mut self,
        path: PathBuf,
        content: SharedString,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(()) => {
                self.current_path = Some(path.clone());
                let current_content = self.editor.read(cx).value();
                self.last_saved = content.clone();
                if current_content == content {
                    self.save_state = SaveState::Saved;
                } else {
                    self.save_state = SaveState::Dirty;
                    self.schedule_auto_save(cx);
                }
            }
            Err(err) => {
                self.save_state = SaveState::Error(err.into());
            }
        }
        cx.notify();
    }

    fn set_title_from_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled note");

        self.title_input.update(cx, |input, cx| {
            input.set_value(title, window, cx);
        });
    }

    fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        self.current_path = None;
        self.last_saved = DEFAULT_NOTE.into();
        self.save_state = SaveState::Saved;
        self.stats = DocumentStats::from_text(DEFAULT_NOTE);

        self.title_input.update(cx, |input, cx| {
            input.set_value("Untitled note", window, cx);
        });
        self.editor.update(cx, |editor, cx| {
            editor.set_value(DEFAULT_NOTE, window, cx);
            editor.focus(window, cx);
        });
        self.preview.update(cx, |preview, cx| {
            preview.set_text(DEFAULT_NOTE, cx);
        });
        cx.notify();
    }

    fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Markdown file".into()),
        });

        let view = cx.entity();
        cx.spawn_in(window, async move |_, window| {
            let paths = paths.await.ok()?.ok()??;
            let path = paths.first()?.clone();
            let result = std::fs::read_to_string(&path).map_err(|err| err.to_string());

            window
                .update(|window, cx| {
                    view.update(cx, |this, cx| match result {
                        Ok(content) => this.load_file(path, content, window, cx),
                        Err(err) => {
                            this.save_state = SaveState::Error(err.into());
                            cx.notify();
                        }
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    fn load_file(
        &mut self,
        path: PathBuf,
        content: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        self.current_path = Some(path.clone());
        self.last_saved = content.clone().into();
        self.save_state = SaveState::Saved;
        self.stats = DocumentStats::from_text(&content);
        self.set_title_from_path(&path, window, cx);

        self.editor.update(cx, |editor, cx| {
            editor.set_highlighter("markdown", cx);
            editor.set_value(content.clone(), window, cx);
            editor.focus(window, cx);
        });
        self.preview.update(cx, |preview, cx| {
            preview.set_text(&content, cx);
        });
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.current_path.clone() {
            self.save_to_path(path, cx);
        } else {
            self.save_as(window, cx);
        }
    }

    fn save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let start_dir = self
            .current_path
            .as_ref()
            .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        let file_name = self.suggested_file_name(cx);
        let receiver = cx.prompt_for_new_path(&start_dir, Some(&file_name));
        let view = cx.entity();

        cx.spawn_in(window, async move |_, window| {
            let path = receiver.await.ok().into_iter().flatten().flatten().next()?;
            window
                .update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.set_title_from_path(&path, window, cx);
                        this.save_to_path(path, cx);
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    fn save_to_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.auto_save_epoch = self.auto_save_epoch.saturating_add(1);
        let content = self.editor.read(cx).value();
        self.save_state = SaveState::Saving;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let write_content = content.to_string();
            let result = std::fs::write(&path, write_content).map_err(|err| err.to_string());

            this.update(cx, |this, cx| {
                this.finish_save(path, content, result, cx);
            })
            .ok();
        })
        .detach();
    }

    fn suggested_file_name(&self, cx: &mut Context<Self>) -> String {
        let raw = self.title_input.read(cx).value().to_string();
        let title = raw.trim();
        let stem = if title.is_empty() { "untitled" } else { title };
        let mut file_name = String::with_capacity(stem.len() + 3);

        for ch in stem.chars() {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                file_name.push(ch.to_ascii_lowercase());
            } else if ch.is_whitespace() {
                file_name.push('-');
            }
        }

        if file_name.is_empty() {
            file_name.push_str("untitled");
        }
        if !file_name.ends_with(".md") {
            file_name.push_str(".md");
        }
        file_name
    }

    fn toggle_mode(&mut self, cx: &mut Context<Self>) {
        self.mode = match self.mode {
            EditorMode::Split => EditorMode::Source,
            EditorMode::Source => EditorMode::Preview,
            EditorMode::Preview => EditorMode::Split,
        };
        cx.notify();
    }

    fn apply_format(
        &mut self,
        action: &ApplyMarkdownFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self.editor.read(cx).selected_value().to_string();
        let replacement = match action.0 {
            MarkdownFormat::HeadingOne => Self::prefix_block(&selected, "# ", "Heading"),
            MarkdownFormat::HeadingTwo => Self::prefix_block(&selected, "## ", "Heading"),
            MarkdownFormat::HeadingThree => Self::prefix_block(&selected, "### ", "Heading"),
            MarkdownFormat::Bold => Self::wrap_inline(&selected, "**", "**", "bold text"),
            MarkdownFormat::Italic => Self::wrap_inline(&selected, "*", "*", "italic text"),
            MarkdownFormat::InlineCode => Self::wrap_inline(&selected, "`", "`", "code"),
            MarkdownFormat::Link => Self::wrap_inline(&selected, "[", "](https://)", "link text"),
            MarkdownFormat::BulletList => Self::prefix_lines(&selected, "- ", "List item"),
            MarkdownFormat::OrderedList => Self::numbered_lines(&selected),
            MarkdownFormat::Quote => Self::prefix_lines(&selected, "> ", "Quote"),
            MarkdownFormat::CodeBlock => Self::code_block(&selected),
        };

        self.editor.update(cx, |editor, cx| {
            editor.replace(replacement, window, cx);
            editor.focus(window, cx);
        });
    }

    fn wrap_inline(selected: &str, prefix: &str, suffix: &str, placeholder: &str) -> String {
        let body = if selected.is_empty() {
            placeholder
        } else {
            selected
        };
        format!("{prefix}{body}{suffix}")
    }

    fn prefix_block(selected: &str, prefix: &str, placeholder: &str) -> String {
        let body = selected.trim_start_matches('#').trim_start();
        let body = if body.is_empty() { placeholder } else { body };
        format!("{prefix}{body}")
    }

    fn prefix_lines(selected: &str, prefix: &str, placeholder: &str) -> String {
        if selected.is_empty() {
            return format!("{prefix}{placeholder}");
        }

        selected
            .lines()
            .map(|line| format!("{prefix}{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn numbered_lines(selected: &str) -> String {
        if selected.is_empty() {
            return "1. List item".to_string();
        }

        selected
            .lines()
            .enumerate()
            .map(|(index, line)| format!("{}. {}", index + 1, line))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn code_block(selected: &str) -> String {
        let body = if selected.is_empty() {
            "code"
        } else {
            selected
        };
        format!("```\n{body}\n```")
    }

    fn on_action_new(&mut self, _: &NewMarkdownNote, window: &mut Window, cx: &mut Context<Self>) {
        self.new_note(window, cx);
    }

    fn on_action_open(
        &mut self,
        _: &OpenMarkdownFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file(window, cx);
    }

    fn on_action_save(
        &mut self,
        _: &SaveMarkdownFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save(window, cx);
    }

    fn on_action_save_as(
        &mut self,
        _: &SaveMarkdownFileAs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_as(window, cx);
    }

    fn on_action_toggle_mode(
        &mut self,
        _: &ToggleEditorMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_mode(cx);
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.mode;
        let save_state = self.save_state.clone();

        h_flex()
            .id("markdown-toolbar")
            .w_full()
            .gap_2()
            .items_center()
            .justify_between()
            .py_2()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("new-note")
                            .icon(IconName::Plus)
                            .ghost()
                            .small()
                            .tooltip("New note (Ctrl+N)")
                            .on_click(cx.listener(|this, _, window, cx| this.new_note(window, cx))),
                    )
                    .child(
                        Button::new("open-note")
                            .icon(IconName::FolderOpen)
                            .ghost()
                            .small()
                            .tooltip("Open Markdown (Ctrl+O)")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_file(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("save-note")
                            .icon(IconName::Check)
                            .ghost()
                            .small()
                            .tooltip("Save (Ctrl+S)")
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    )
                    .child(
                        Button::new("save-note-as")
                            .label("Save as")
                            .ghost()
                            .small()
                            .tooltip("Save as (Ctrl+Shift+S)")
                            .on_click(cx.listener(|this, _, window, cx| this.save_as(window, cx))),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(format_button("h1", "H1", MarkdownFormat::HeadingOne))
                    .child(format_button("h2", "H2", MarkdownFormat::HeadingTwo))
                    .child(format_button("h3", "H3", MarkdownFormat::HeadingThree))
                    .child(format_button("bold", "B", MarkdownFormat::Bold))
                    .child(format_button("italic", "I", MarkdownFormat::Italic))
                    .child(format_button("code", "Code", MarkdownFormat::InlineCode))
                    .child(format_button("link", "Link", MarkdownFormat::Link))
                    .child(format_button(
                        "bullet",
                        "• List",
                        MarkdownFormat::BulletList,
                    ))
                    .child(format_button(
                        "ordered",
                        "1. List",
                        MarkdownFormat::OrderedList,
                    ))
                    .child(format_button("quote", "Quote", MarkdownFormat::Quote))
                    .child(format_button(
                        "code-block",
                        "Block",
                        MarkdownFormat::CodeBlock,
                    )),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        Button::new("mode-split")
                            .label("Split")
                            .ghost()
                            .small()
                            .selected(mode == EditorMode::Split)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.mode = EditorMode::Split;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("mode-source")
                            .label("Source")
                            .ghost()
                            .small()
                            .selected(mode == EditorMode::Source)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.mode = EditorMode::Source;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("mode-preview")
                            .label("Preview")
                            .ghost()
                            .small()
                            .selected(mode == EditorMode::Preview)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.mode = EditorMode::Preview;
                                cx.notify();
                            })),
                    )
                    .child(status_badge(save_state, cx)),
            )
    }

    fn render_source(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("markdown-source")
            .size_full()
            .font_family(cx.theme().mono_font_family.clone())
            .text_size(cx.theme().mono_font_size)
            .bg(cx.theme().background)
            .child(
                Input::new(&self.editor)
                    .h_full()
                    .p_0()
                    .border_0()
                    .focus_bordered(false),
            )
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("markdown-preview")
            .size_full()
            .bg(cx.theme().background)
            .child(
                TextView::new(&self.preview)
                    .code_block_actions(|code_block, _window, _cx| {
                        Clipboard::new("copy-code").value(code_block.code().clone())
                    })
                    .p_5()
                    .scrollable(true)
                    .selectable(true),
            )
    }

    fn render_editor_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        match self.mode {
            EditorMode::Split => h_resizable("markdown-editor-split")
                .child(resizable_panel().child(self.render_source(cx)))
                .child(resizable_panel().child(self.render_preview(cx)))
                .into_any_element(),
            EditorMode::Source => self.render_source(cx).into_any_element(),
            EditorMode::Preview => self.render_preview(cx).into_any_element(),
        }
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let path = self
            .current_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Not saved yet".to_string());

        h_flex()
            .id("markdown-status-bar")
            .w_full()
            .items_center()
            .justify_between()
            .gap_3()
            .px_3()
            .py_1p5()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .text_color(cx.theme().muted_foreground)
            .text_xs()
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(SharedString::from(path)),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(format!("{} lines", self.stats.lines))
                    .child(format!("{} words", self.stats.words))
                    .child(format!("{} chars", self.stats.characters))
                    .child("Ctrl+E toggles mode"),
            )
    }
}

impl Focusable for MarkdownEditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MarkdownEditorView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("markdown-editor-window")
            .key_context("MarkdownEditor")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .on_action(cx.listener(Self::on_action_new))
            .on_action(cx.listener(Self::on_action_open))
            .on_action(cx.listener(Self::on_action_save))
            .on_action(cx.listener(Self::on_action_save_as))
            .on_action(cx.listener(Self::on_action_toggle_mode))
            .on_action(cx.listener(Self::apply_format))
            .child(
                TitleBar::new().bg(cx.theme().title_bar).child(
                    h_flex()
                        .id("markdown-title")
                        .items_center()
                        .gap_2()
                        .h_full()
                        .w_full()
                        .child(IconName::BookOpen)
                        .child(
                            Input::new(&self.title_input)
                                .border_0()
                                .focus_bordered(false)
                                .bg(cx.theme().title_bar)
                                .w(px(280.)),
                        ),
                ),
            )
            .child(self.render_toolbar(cx))
            .child(div().flex_1().min_h_0().child(self.render_editor_body(cx)))
            .child(self.render_status_bar(cx))
    }
}

impl DocumentStats {
    fn from_text(text: &str) -> Self {
        Self {
            lines: text.lines().count().max(1),
            words: text.split_whitespace().count(),
            characters: text.chars().count(),
        }
    }
}

fn format_button(id: &'static str, label: &'static str, format: MarkdownFormat) -> Button {
    Button::new(format!("format-{id}"))
        .label(label)
        .ghost()
        .small()
        .on_click(move |_, window, cx| {
            window.dispatch_action(Box::new(ApplyMarkdownFormat(format)), cx);
        })
}

fn status_badge(save_state: SaveState, cx: &mut Context<MarkdownEditorView>) -> impl IntoElement {
    match save_state {
        SaveState::Saved => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().success)
            .child(IconName::CircleCheck)
            .child("Saved")
            .into_any_element(),
        SaveState::Dirty => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().warning)
            .child(IconName::Asterisk)
            .child("Unsaved")
            .into_any_element(),
        SaveState::Saving => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().info)
            .child(IconName::Loader)
            .child("Saving")
            .into_any_element(),
        SaveState::Error(err) => h_flex()
            .id("save-status")
            .gap_1()
            .items_center()
            .text_color(cx.theme().danger)
            .child(IconName::TriangleAlert)
            .child(err)
            .into_any_element(),
    }
}
