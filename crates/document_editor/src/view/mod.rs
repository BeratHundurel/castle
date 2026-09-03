mod inspector;
mod outline;
mod preview;
mod source;
mod status_bar;

pub(super) use preview::prepare_markdown_preview_sections;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, ElementExt as _, Icon, IconName, Selectable as _,
    Sizable as _,
    animation::ease_in_out_cubic,
    button::{Button, ButtonVariants as _},
    clipboard::Clipboard,
    h_flex,
    input::{self, Editor, EditorState, Input, RopeExt as _},
    resizable::{h_resizable, resizable_panel, v_resizable},
    scroll::ScrollableElement,
    text::{TextView, TextViewStyle},
    tooltip::Tooltip,
    v_flex,
};
use std::{collections::HashSet, ops::Range, path::Path};

use super::action::{FormatDocument, ToggleFocusMode, ToggleTypewriterScrolling, ToggleZenMode};
use super::document_state::*;
use super::vim::VimMode;
use super::{DocumentEditorView, DocumentInspectorTab, DocumentKind};
use runtime::AppRuntime;
use settings::AppSettings;

#[derive(Clone)]
struct OutlineResizeDrag {
    editor_id: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkdownPreviewVirtualization {
    Blocks,
    Sections,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SplitLayout {
    Horizontal,
    Vertical,
}

impl Render for OutlineResizeDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Focusable for DocumentEditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl DocumentEditorView {
    pub(crate) fn render_editor_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.persistence.is_loading {
            return div()
                .id("document-loading")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("Loading document...")
                .into_any_element();
        }

        if let Some(error) = self.persistence.load_error.clone() {
            return div()
                .id("document-load-error")
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .p_6()
                .text_color(cx.theme().danger)
                .child(error)
                .into_any_element();
        }

        match self.mode {
            EditorMode::Source => self.render_source(cx).into_any_element(),
            EditorMode::Split => self.render_split(cx).into_any_element(),
            EditorMode::Preview => self.render_preview(cx).into_any_element(),
        }
    }
}

impl DocumentEditorView {
    fn render_split(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (outline_in_layout, _) = editor_layout_signature(
            self.view_width,
            self.effective_outline_rendered(),
            self.outline_width,
        );
        let outline_width = outline_width_for_view(self.outline_width, self.view_width);
        let available_width = self.view_width
            - if outline_in_layout {
                outline_width
            } else {
                px(0.)
            };
        let layout = split_layout_for_width(available_width);
        let source_width = self
            .analysis
            .source_bounds_mode
            .filter(|mode| *mode == EditorMode::Split)
            .and_then(|_| self.analysis.source_bounds.map(|bounds| bounds.size.width))
            .unwrap_or_else(|| split_source_width(available_width, layout));
        let preview_width = self
            .analysis
            .preview_bounds_mode
            .filter(|mode| *mode == EditorMode::Split)
            .and_then(|_| self.analysis.preview_bounds.map(|bounds| bounds.size.width))
            .unwrap_or_else(|| split_preview_width(available_width, layout));
        let source = self.render_split_panel(
            IconName::File,
            "Source",
            self.render_source_with_width(source_width, false, cx)
                .into_any_element(),
            cx,
        );
        let preview = self.render_split_panel(
            IconName::Eye,
            "Preview",
            self.render_preview_with_width(preview_width, cx)
                .into_any_element(),
            cx,
        );

        let split = match layout {
            SplitLayout::Horizontal => h_resizable("document-editor-split-horizontal")
                .child(
                    resizable_panel()
                        .size_range(px(320.)..Pixels::MAX)
                        .child(source),
                )
                .child(
                    resizable_panel()
                        .size_range(px(280.)..Pixels::MAX)
                        .child(preview),
                )
                .into_any_element(),
            SplitLayout::Vertical => v_resizable("document-editor-split-vertical")
                .child(
                    resizable_panel()
                        .size_range(px(180.)..Pixels::MAX)
                        .child(source),
                )
                .child(
                    resizable_panel()
                        .size_range(px(180.)..Pixels::MAX)
                        .child(preview),
                )
                .into_any_element(),
        };

        div()
            .id("document-editor-split")
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(split)
    }

    fn render_split_panel(
        &self,
        icon: IconName,
        label: &'static str,
        content: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        v_flex()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(
                h_flex()
                    .h_7()
                    .flex_shrink_0()
                    .px_3()
                    .items_center()
                    .gap_1()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.72))
                    .bg(cx.theme().secondary.opacity(0.32))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().muted_foreground)
                    .child(Icon::new(icon).xsmall())
                    .child(label),
            )
            .child(div().flex_1().min_w_0().min_h_0().child(content))
            .into_any_element()
    }
}

impl Render for DocumentEditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.mermaid.release_retired_images_after_frame(window);
        self.sync_vim_setting(window, cx);
        self.sync_vim_search_focus(window, cx);
        let theme_background = cx.theme().background;
        let theme_border = cx.theme().border;
        let theme_input = cx.theme().input;
        let status_line_visible = self
            .zen
            .status_bar_visible(AppSettings::editor_status_line_visible(cx));
        let outline_rendered = self.effective_outline_rendered();
        let vim_context = self.vim_context();

        v_flex()
            .id("document-editor-window")
            .key_context(vim_context.as_str())
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .on_action(cx.listener(Self::on_action_save))
            .on_action(cx.listener(Self::on_action_save_as))
            .on_action(cx.listener(Self::on_action_format_document))
            .on_action(cx.listener(Self::on_action_toggle_mode))
            .on_action(cx.listener(Self::on_action_create_card_from_selection))
            .on_action(cx.listener(Self::on_action_insert_board_view))
            .on_action(cx.listener(Self::on_action_toggle_outline))
            .on_action(cx.listener(Self::on_action_toggle_focus_mode))
            .on_action(cx.listener(Self::on_action_toggle_typewriter_scrolling))
            .on_action(cx.listener(Self::on_action_toggle_zen_mode))
            .on_action(cx.listener(Self::on_action_toggle_zen_status_bar))
            .on_action(cx.listener(Self::on_action_expand_emmet))
            .on_action(cx.listener(Self::on_action_emmet_submit_wrap))
            .on_action(cx.listener(Self::on_action_emmet_cancel_wrap))
            .on_action(cx.listener(Self::apply_format))
            .on_action(cx.listener(Self::on_action_move_line_up))
            .on_action(cx.listener(Self::on_action_move_line_down))
            .on_action(cx.listener(Self::on_action_toggle_task))
            .capture_key_down(cx.listener(Self::on_smart_edit_key_down))
            .on_action(cx.listener(Self::on_action_vim_key))
            .capture_key_down(cx.listener(Self::on_vim_key_down))
            .capture_action(cx.listener(Self::on_action_vim_insert_escape))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .w_full()
                    .on_prepaint({
                        let view = cx.entity();
                        move |bounds, _, cx| {
                            view.update(cx, |this, cx| {
                                if this.view_bounds != Some(bounds) {
                                    let previous_layout = editor_layout_signature(
                                        this.view_width,
                                        this.effective_outline_rendered(),
                                        this.outline_width,
                                    );
                                    let next_layout = editor_layout_signature(
                                        bounds.size.width,
                                        this.effective_outline_rendered(),
                                        this.outline_width,
                                    );
                                    let first_measurement = this.view_bounds.is_none();
                                    this.view_width = bounds.size.width;
                                    this.view_bounds = Some(bounds);
                                    if first_measurement || previous_layout != next_layout {
                                        cx.notify();
                                    } else {
                                        this.schedule_view_layout_refresh(cx);
                                    }
                                }
                            });
                        }
                    })
                    .child(
                        h_flex()
                            .size_full()
                            .min_w_0()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .child(self.render_editor_body(cx)),
                            )
                            .children(
                                editor_layout_signature(
                                    self.view_width,
                                    outline_rendered,
                                    self.outline_width,
                                )
                                .0
                                .then(|| self.render_outline_for_layout(cx)),
                            ),
                    ),
            )
            .children(status_line_visible.then(|| self.render_status_bar(cx)))
            .children(self.zen.enabled.then(|| self.render_zen_toolbar(cx)))
            .children(
                (!status_line_visible && self.vim_is_enabled() && self.mode.shows_source()).then(
                    || {
                        div()
                            .id("vim-mode-overlay")
                            .debug_selector(|| "vim-mode-overlay".to_string())
                            .absolute()
                            .bottom(px(8.))
                            .left(px(8.))
                            .child(self.render_vim_mode_indicator(cx))
                    },
                ),
            )
            .children(
                (self.kind == DocumentKind::Markdown && self.show_emmet_input).then(|| {
                    div()
                        .key_context("EmmetInput")
                        .absolute()
                        .top(px(60.))
                        .left(px(20.))
                        .w(px(300.))
                        .p_2()
                        .bg(theme_background)
                        .border_1()
                        .border_color(theme_border)
                        .rounded_md()
                        .shadow_sm()
                        .child(
                            Input::new(&self.emmet_input)
                                .w_full()
                                .bg(theme_input)
                                .px_2()
                                .py_1()
                                .rounded_sm(),
                        )
                }),
            )
    }
}

impl DocumentEditorView {
    fn render_vim_mode_indicator(&self, cx: &mut Context<Self>) -> AnyElement {
        let (label, color) = match self.vim_mode() {
            VimMode::Normal => ("NORMAL", cx.theme().accent_foreground),
            VimMode::Insert => ("INSERT", cx.theme().success),
            VimMode::Visual => ("VISUAL", cx.theme().warning),
            VimMode::VisualLine => ("VISUAL LINE", cx.theme().warning),
        };
        let command = self.vim_state.state.command_text();
        h_flex()
            .id("vim-mode-indicator")
            .h_5()
            .px_2()
            .gap_1()
            .items_center()
            .rounded_sm()
            .bg(color.opacity(0.14))
            .text_color(color)
            .text_xs()
            .child(label)
            .children(
                (!command.is_empty())
                    .then(|| div().text_color(cx.theme().muted_foreground).child(command)),
            )
            .into_any_element()
    }

    fn render_outline_for_layout(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.zen.enabled {
            let outline_width = outline_width_for_view(self.outline_width, self.view_width);
            return div()
                .h_full()
                .flex_shrink_0()
                .overflow_hidden()
                .w(outline_width)
                .child(self.render_outline(cx))
                .into_any_element();
        }
        self.render_outline_transition(cx)
    }

    fn render_zen_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let supports_outline = self.kind.supports_outline();
        h_flex()
            .id("zen-toolbar")
            .absolute()
            .top(px(8.))
            .right(px(8.))
            .px_1()
            .py_1()
            .gap_1()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background.opacity(0.92))
            .child(
                Button::new("toggle-zen-status-bar")
                    .icon(IconName::PanelBottom)
                    .ghost()
                    .xsmall()
                    .selected(self.zen.show_status_bar)
                    .tooltip("Show status bar in zen mode")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_zen_status_bar(cx);
                    })),
            )
            .children(supports_outline.then(|| {
                Button::new("toggle-zen-outline")
                    .icon(IconName::PanelRight)
                    .ghost()
                    .xsmall()
                    .selected(self.zen.show_outline)
                    .tooltip("Show outline in zen mode")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_zen_outline(window, cx);
                    }))
            }))
            .child(
                Button::new("exit-zen-mode")
                    .icon(IconName::Minimize)
                    .ghost()
                    .xsmall()
                    .tooltip(format!("Exit zen mode ({})", zen_shortcut()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_zen_mode(window, cx);
                    })),
            )
            .into_any_element()
    }
}

fn zen_shortcut() -> String {
    if cfg!(target_os = "macos") {
        "Cmd+Alt+Z".to_string()
    } else {
        "Ctrl+Alt+Z".to_string()
    }
}

fn vim_overlay(
    bounds: Bounds<Pixels>,
    source_bounds: Bounds<Pixels>,
    color: Hsla,
    cursor: bool,
) -> impl IntoElement {
    let left = bounds.origin.x - source_bounds.origin.x;
    let top = bounds.origin.y - source_bounds.origin.y;
    let width = if cursor {
        bounds.size.width.max(px(7.))
    } else {
        bounds.size.width.max(px(2.))
    };
    div()
        .absolute()
        .left(left)
        .top(top)
        .w(width)
        .h(bounds.size.height)
        .bg(color)
}

pub(super) fn vim_selection_bounds(
    editor: &EditorState,
    range: Range<usize>,
    source_bounds: Bounds<Pixels>,
) -> Vec<Bounds<Pixels>> {
    if range.end.saturating_sub(range.start) <= 2
        && editor
            .text()
            .slice(range.clone())
            .chars()
            .all(|ch| matches!(ch, '\r' | '\n'))
    {
        return vim_cursor_bounds(editor, range.start).into_iter().collect();
    }
    let (Some(start), Some(end)) = (
        editor.range_to_bounds(&(range.start..range.start)),
        editor.range_to_bounds(&(range.end..range.end)),
    ) else {
        return editor.range_to_bounds(&range).into_iter().collect();
    };
    let line_height = editor.line_height().unwrap_or(start.size.height);
    let visible_top = source_bounds.top();
    let visible_bottom = source_bounds.bottom();
    let is_visible =
        |origin_y: Pixels| origin_y < visible_bottom && origin_y + line_height > visible_top;
    if end.origin.y == start.origin.y {
        if !is_visible(start.origin.y) {
            return Vec::new();
        }
        return vec![Bounds::new(
            start.origin,
            size((end.origin.x - start.origin.x).max(px(2.)), line_height),
        )];
    }

    let padding = source_horizontal_padding(source_bounds.size.width);
    let content_left = source_bounds.origin.x + padding;
    let content_right = source_bounds.origin.x + source_bounds.size.width - padding;
    let mut bounds = Vec::new();
    if is_visible(start.origin.y) {
        bounds.push(Bounds::new(
            start.origin,
            size((content_right - start.origin.x).max(px(2.)), line_height),
        ));
    }
    let mut y = start.origin.y + line_height;
    if y < visible_top {
        let hidden_rows = ((visible_top - y) / line_height).floor();
        y += line_height * hidden_rows;
        while y + line_height <= visible_top {
            y += line_height;
        }
    }
    let full_row_end = end.origin.y.min(visible_bottom);
    while y + px(0.5) < full_row_end {
        bounds.push(Bounds::new(
            point(content_left, y),
            size((content_right - content_left).max(px(2.)), line_height),
        ));
        y += line_height;
    }
    if let Some(tail_width) = vim_selection_tail_width(content_left, end.origin.x)
        && is_visible(end.origin.y)
    {
        bounds.push(Bounds::new(
            point(content_left, end.origin.y),
            size(tail_width, line_height),
        ));
    }
    bounds
}

fn vim_selection_tail_width(content_left: Pixels, end_x: Pixels) -> Option<Pixels> {
    let width = end_x - content_left;
    (width > px(0.5)).then_some(width)
}

pub(super) fn vim_cursor_bounds(editor: &EditorState, cursor: usize) -> Option<Bounds<Pixels>> {
    let caret = editor.range_to_bounds(&(cursor..cursor))?;
    let line_height = editor.line_height().unwrap_or(caret.size.height);
    let character = match editor.text().char_at(cursor) {
        Some('\r' | '\n') | None => caret,
        Some(ch) => editor
            .range_to_bounds(&(cursor..cursor + ch.len_utf8()))
            .unwrap_or(caret),
    };
    Some(normalize_vim_cursor_bounds(character, caret, line_height))
}

fn normalize_vim_cursor_bounds(
    character: Bounds<Pixels>,
    caret: Bounds<Pixels>,
    line_height: Pixels,
) -> Bounds<Pixels> {
    let bounds = if character.size.height > line_height + px(0.5) {
        caret
    } else {
        character
    };
    Bounds::new(
        bounds.origin,
        size(bounds.size.width.max(px(7.)), line_height),
    )
}

fn save_state_status(
    save_state: &SaveState,
    cx: &mut Context<DocumentEditorView>,
) -> (IconName, Hsla, SharedString) {
    match save_state {
        SaveState::Saved => (IconName::CircleCheck, cx.theme().success, "Saved".into()),
        SaveState::Dirty => (IconName::Asterisk, cx.theme().warning, "Unsaved".into()),
        SaveState::Saving => (IconName::Loader, cx.theme().info, "Saving".into()),
        SaveState::Missing => (
            IconName::TriangleAlert,
            cx.theme().warning,
            "File missing".into(),
        ),
        SaveState::Error(_) => (
            IconName::TriangleAlert,
            cx.theme().danger,
            "Save failed".into(),
        ),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct StatusPath {
    directory: Option<String>,
    file_name: String,
    tooltip: String,
}

fn status_path(path: Option<&Path>, kind: DocumentKind) -> StatusPath {
    let Some(path) = path else {
        return StatusPath {
            directory: None,
            file_name: "Not saved yet".to_string(),
            tooltip: "This note has not been saved to a file".to_string(),
        };
    };

    let tooltip = readable_full_path(path);
    let trimmed = tooltip.trim_end_matches(['/', '\\']);
    let (parent, file_name) = trimmed
        .rfind(['/', '\\'])
        .map(|separator| (&trimmed[..separator], &trimmed[separator + 1..]))
        .unwrap_or(("", trimmed));
    let directory = parent
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .filter(|directory| !directory.is_empty() && !directory.ends_with(':'))
        .map(str::to_string);
    let file_name = status_file_name(file_name, kind);

    StatusPath {
        directory,
        file_name,
        tooltip,
    }
}

fn readable_full_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if let Some(path) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{path}")
    } else {
        path.strip_prefix(r"\\?\")
            .unwrap_or(path.as_ref())
            .to_string()
    }
}

fn status_file_name(file_name: &str, kind: DocumentKind) -> String {
    if kind == DocumentKind::Markdown
        && let Some((stem, extension)) = file_name.rsplit_once('.')
        && matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown")
    {
        return stem.to_string();
    }

    file_name.to_string()
}

fn status_metric(icon: IconName, label: String) -> impl IntoElement {
    h_flex()
        .items_center()
        .gap_1()
        .child(Icon::new(icon).xsmall())
        .child(label)
}

fn source_horizontal_padding(source_width: Pixels) -> Pixels {
    const EDITOR_MAX_WIDTH: f32 = 920.;
    const EDITOR_GUTTER: f32 = 20.;

    px(((source_width.as_f32() - EDITOR_MAX_WIDTH) / 2. + EDITOR_GUTTER).max(EDITOR_GUTTER))
}

fn markdown_preview_horizontal_padding(preview_width: Pixels) -> Pixels {
    const PREVIEW_MAX_WIDTH: f32 = 920.;
    const PREVIEW_GUTTER: f32 = 24.;

    px(((preview_width.as_f32() - PREVIEW_MAX_WIDTH) / 2. + PREVIEW_GUTTER).max(PREVIEW_GUTTER))
}

fn markdown_preview_virtualization(outline_is_empty: bool) -> MarkdownPreviewVirtualization {
    if outline_is_empty {
        MarkdownPreviewVirtualization::Blocks
    } else {
        MarkdownPreviewVirtualization::Sections
    }
}

fn markdown_preview_block_gap() -> Rems {
    rems(1.)
}

fn markdown_preview_section_top_padding(index: usize) -> Rems {
    if index == 0 {
        rems(0.)
    } else {
        markdown_preview_block_gap()
    }
}

const SPLIT_HORIZONTAL_MIN_WIDTH: Pixels = px(680.);

pub(super) fn editor_layout_signature(
    width: Pixels,
    outline_rendered: bool,
    requested_outline_width: Pixels,
) -> (bool, bool) {
    let outline_in_layout = outline_rendered && width >= px(760.);
    let available_width = width
        - if outline_in_layout {
            outline_width_for_view(requested_outline_width, width)
        } else {
            px(0.)
        };
    (
        outline_in_layout,
        split_layout_for_width(available_width) == SplitLayout::Horizontal,
    )
}

fn split_layout_for_width(width: Pixels) -> SplitLayout {
    if width >= SPLIT_HORIZONTAL_MIN_WIDTH {
        SplitLayout::Horizontal
    } else {
        SplitLayout::Vertical
    }
}

fn split_source_width(width: Pixels, layout: SplitLayout) -> Pixels {
    match layout {
        SplitLayout::Horizontal => width / 2.,
        SplitLayout::Vertical => width,
    }
}

fn split_preview_width(width: Pixels, layout: SplitLayout) -> Pixels {
    match layout {
        SplitLayout::Horizontal => width - split_source_width(width, layout),
        SplitLayout::Vertical => width,
    }
}

fn outline_width_for_view(requested_width: Pixels, view_width: Pixels) -> Pixels {
    let available_width = (view_width - super::EDITOR_MIN_WIDTH_WITH_OUTLINE)
        .max(super::OUTLINE_MIN_WIDTH)
        .min(super::OUTLINE_MAX_WIDTH);
    requested_width.clamp(super::OUTLINE_MIN_WIDTH, available_width)
}

fn outline_row_left_padding(depth: usize) -> Pixels {
    px(8.) + super::OUTLINE_INDENT_STEP * depth as f32
}

fn markdown_preview_style(font_size: Pixels) -> TextViewStyle {
    TextViewStyle {
        heading_base_font_size: font_size,
        code_block: StyleRefinement::default().text_size(font_size),
        ..Default::default()
    }
}

fn link_section_title(label: &str, cx: &App) -> impl IntoElement {
    h_flex()
        .h_9()
        .px_3()
        .mt_1()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(label.to_string())
}

fn link_group_title(label: &str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .pt_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground.opacity(0.8))
        .child(label.to_string())
}

fn link_empty_state(label: &str, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .pb_3()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(label.to_string())
}

fn reserves_disclosure_space(kind: DocumentKind, has_children: bool) -> bool {
    kind == DocumentKind::Json && !has_children
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, point, px, rems, size};

    use super::{
        DocumentKind, MarkdownPreviewVirtualization, SplitLayout, markdown_preview_block_gap,
        markdown_preview_horizontal_padding, markdown_preview_section_top_padding,
        markdown_preview_virtualization, normalize_vim_cursor_bounds, outline_row_left_padding,
        outline_width_for_view, reserves_disclosure_space, split_layout_for_width,
        split_preview_width, split_source_width, status_path, vim_selection_tail_width,
    };
    use std::path::Path;

    #[test]
    fn markdown_rows_do_not_reserve_json_disclosure_space() {
        assert!(!reserves_disclosure_space(DocumentKind::Markdown, false));
        assert!(reserves_disclosure_space(DocumentKind::Json, false));
        assert!(!reserves_disclosure_space(DocumentKind::Json, true));
    }

    #[test]
    fn outline_width_respects_panel_and_editor_constraints() {
        assert_eq!(outline_width_for_view(px(120.), px(1_200.)), px(176.));
        assert_eq!(outline_width_for_view(px(900.), px(1_200.)), px(480.));
        assert_eq!(outline_width_for_view(px(480.), px(760.)), px(400.));
    }

    #[test]
    fn nested_outline_rows_use_compact_indentation() {
        assert_eq!(outline_row_left_padding(0), px(8.));
        assert_eq!(outline_row_left_padding(3), px(32.));
    }

    #[test]
    fn markdown_preview_keeps_reading_width_and_minimum_gutter() {
        assert_eq!(markdown_preview_horizontal_padding(px(800.)), px(24.));
        assert_eq!(markdown_preview_horizontal_padding(px(1_200.)), px(164.));
    }

    #[test]
    fn markdown_preview_preserves_outline_navigation_with_section_virtualization() {
        assert_eq!(
            markdown_preview_virtualization(true),
            MarkdownPreviewVirtualization::Blocks
        );
        assert_eq!(
            markdown_preview_virtualization(false),
            MarkdownPreviewVirtualization::Sections
        );
    }

    #[test]
    fn markdown_preview_restores_block_spacing_between_virtualized_sections() {
        assert_eq!(markdown_preview_section_top_padding(0), rems(0.));
        assert_eq!(
            markdown_preview_section_top_padding(1),
            markdown_preview_block_gap()
        );
    }

    #[test]
    fn split_layout_keeps_both_panes_side_by_side_when_they_fit() {
        assert_eq!(split_layout_for_width(px(680.)), SplitLayout::Horizontal);
        assert_eq!(
            split_source_width(px(1_000.), SplitLayout::Horizontal),
            px(500.)
        );
        assert_eq!(
            split_preview_width(px(1_000.), SplitLayout::Horizontal),
            px(500.)
        );
    }

    #[test]
    fn split_layout_stacks_panes_when_a_narrow_window_would_hurt_writing() {
        assert_eq!(split_layout_for_width(px(679.)), SplitLayout::Vertical);
        assert_eq!(
            split_source_width(px(600.), SplitLayout::Vertical),
            px(600.)
        );
        assert_eq!(
            split_preview_width(px(600.), SplitLayout::Vertical),
            px(600.)
        );
    }

    #[test]
    fn status_path_replaces_storage_prefix_with_a_compact_breadcrumb() {
        let path = status_path(
            Some(Path::new(
                r"\\?\C:\Users\Berat\Documents\Obsidian Vault\Cover Letter\Cover Letter Variant.md",
            )),
            DocumentKind::Markdown,
        );

        assert_eq!(path.directory.as_deref(), Some("Cover Letter"));
        assert_eq!(path.file_name, "Cover Letter Variant");
        assert_eq!(
            path.tooltip,
            r"C:\Users\Berat\Documents\Obsidian Vault\Cover Letter\Cover Letter Variant.md"
        );
    }

    #[test]
    fn status_path_preserves_meaningful_non_markdown_extensions() {
        let path = status_path(
            Some(Path::new("workspace/settings.json")),
            DocumentKind::Json,
        );

        assert_eq!(path.directory.as_deref(), Some("workspace"));
        assert_eq!(path.file_name, "settings.json");
        assert_eq!(path.tooltip, "workspace/settings.json");
    }

    #[test]
    fn status_path_has_a_clear_label_before_the_first_save() {
        let path = status_path(None, DocumentKind::Markdown);

        assert_eq!(path.directory, None);
        assert_eq!(path.file_name, "Not saved yet");
        assert_eq!(path.tooltip, "This note has not been saved to a file");
    }

    #[test]
    fn vim_cursor_uses_single_row_caret_geometry_for_cross_row_ranges() {
        let line_height = px(18.);
        let caret = Bounds::new(point(px(10.), px(20.)), size(px(0.), line_height));
        let cross_row = Bounds::new(point(px(10.), px(20.)), size(px(80.), px(36.)));
        let normalized = normalize_vim_cursor_bounds(cross_row, caret, line_height);

        assert_eq!(normalized.origin, caret.origin);
        assert_eq!(normalized.size, size(px(7.), line_height));
    }

    #[test]
    fn vim_selection_does_not_paint_a_sliver_at_an_exclusive_row_boundary() {
        assert_eq!(vim_selection_tail_width(px(40.), px(40.)), None);
        assert_eq!(vim_selection_tail_width(px(40.), px(40.4)), None);
        assert_eq!(vim_selection_tail_width(px(40.), px(52.)), Some(px(12.)));
    }
}
