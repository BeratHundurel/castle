use gpui::{Context, EntityInputHandler, Window};
use gpui_component::input::RopeExt;

use super::action::*;
use super::emmet::parse_emmet_abbreviation;
use super::{DocumentEditorView, DocumentKind};

impl DocumentEditorView {
    pub(super) fn on_action_toggle_outline(
        &mut self,
        _: &ToggleDocumentOutline,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_outline(window, cx);
    }

    pub(super) fn on_action_outline_previous(
        &mut self,
        _: &OutlinePrevious,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.outline_rows.is_empty() {
            return;
        }
        self.outline_selected = Some(self.outline_selected.unwrap_or(0).saturating_sub(1));
        if let Some(index) = self.outline_selected {
            self.outline_scroll_handle
                .scroll_to_item(index, gpui::ScrollStrategy::Top);
        }
        cx.notify();
    }

    pub(super) fn on_action_outline_next(
        &mut self,
        _: &OutlineNext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.outline_rows.is_empty() {
            return;
        }
        let next = self
            .outline_selected
            .unwrap_or(0)
            .saturating_add(1)
            .min(self.outline_rows.len().saturating_sub(1));
        self.outline_selected = Some(next);
        self.outline_scroll_handle
            .scroll_to_item(next, gpui::ScrollStrategy::Bottom);
        cx.notify();
    }

    pub(super) fn on_action_outline_left(
        &mut self,
        _: &OutlineLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = self.outline_selected else {
            return;
        };
        let Some(row) = self.outline_rows.get(selected) else {
            return;
        };
        let Some(node_index) = row.node_index else {
            return;
        };

        if row.expanded && self.outline.collapse(node_index) {
            self.rebuild_outline_rows();
        } else if let Some(parent_row) = self.outline.parent_row_index(node_index) {
            self.outline_selected = Some(parent_row);
        }
        cx.notify();
    }

    pub(super) fn on_action_outline_right(
        &mut self,
        _: &OutlineRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = self.outline_selected else {
            return;
        };
        let Some(row) = self.outline_rows.get(selected) else {
            return;
        };
        let Some(node_index) = row.node_index else {
            return;
        };

        if row.has_children && !row.expanded && self.outline.expand(node_index) {
            self.rebuild_outline_rows();
        } else if let Some(child_row) = self.outline.first_child_row_index(node_index) {
            self.outline_selected = Some(child_row);
        }
        cx.notify();
    }

    pub(super) fn on_action_outline_open(
        &mut self,
        _: &OutlineOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.outline_selected {
            self.select_outline_item(index, window, cx);
        }
    }

    pub(super) fn on_action_outline_close(
        &mut self,
        _: &OutlineClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_active_mode(window, cx);
    }

    pub(super) fn on_action_save(
        &mut self,
        _: &SaveDocumentFile,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save(cx);
    }

    pub(super) fn on_action_save_as(
        &mut self,
        _: &SaveDocumentFileAs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.save_as(window, cx);
    }

    pub(super) fn on_action_toggle_mode(
        &mut self,
        _: &ToggleDocumentPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_mode(window, cx);
    }

    pub(super) fn on_action_expand_emmet(
        &mut self,
        _: &ExpandEmmet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind != DocumentKind::Markdown {
            return;
        }
        let selected = self.editor.read(cx).selected_value().to_string();
        let editor_has_selection = !selected.is_empty();

        if editor_has_selection {
            self.show_emmet_input = true;
            let range = self.editor.read(cx).selected_range();
            self.emmet_replacement_range = Some(range);

            self.emmet_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
            cx.notify();
            return;
        }

        let editor = self.editor.read(cx);
        let offset = editor.cursor();
        let text = editor.text().to_string();

        let prefix = &text[..offset];
        let mut start = offset;
        for (idx, ch) in prefix.char_indices().rev() {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '#' || ch == '>' {
                start = idx;
            } else {
                break;
            }
        }

        let (word, replacement_start_offset) = if start < offset {
            (text[start..offset].to_string(), Some(start))
        } else {
            (String::new(), None)
        };

        if !word.is_empty() {
            let replacement = parse_emmet_abbreviation(&word, "");
            self.editor.update(cx, |editor, cx| {
                if let Some(start) = replacement_start_offset {
                    let end = editor.cursor();
                    let rope = editor.text();
                    let start_utf16 = rope.offset_to_offset_utf16(start);
                    let end_utf16 = rope.offset_to_offset_utf16(end);

                    EntityInputHandler::replace_text_in_range(
                        editor,
                        Some(start_utf16..end_utf16),
                        &replacement,
                        window,
                        cx,
                    );
                }
                editor.focus(window, cx);
            });
        } else {
            self.show_emmet_input = true;
            let range = editor.selected_range();
            self.emmet_replacement_range = Some(range);
            self.emmet_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
            cx.notify();
        }
    }

    pub(super) fn on_action_emmet_submit_wrap(
        &mut self,
        _: &EmmetSubmitWrap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind != DocumentKind::Markdown || !self.show_emmet_input {
            return;
        }

        let abbreviation = self.emmet_input.read(cx).value();

        if let Some(range) = self.emmet_replacement_range.clone() {
            self.editor.update(cx, |editor, cx| {
                let rope = editor.text();
                let content = rope.slice(range.clone()).to_string();
                let replacement = parse_emmet_abbreviation(&abbreviation, &content);
                let start_utf16 = rope.offset_to_offset_utf16(range.start);
                let end_utf16 = rope.offset_to_offset_utf16(range.end);

                EntityInputHandler::replace_text_in_range(
                    editor,
                    Some(start_utf16..end_utf16),
                    &replacement,
                    window,
                    cx,
                );
                editor.focus(window, cx);
            });
        }

        self.show_emmet_input = false;
        self.emmet_replacement_range = None;
        cx.notify();
    }

    pub(super) fn on_action_emmet_cancel_wrap(
        &mut self,
        _: &EmmetCancelWrap,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.kind == DocumentKind::Markdown && self.show_emmet_input {
            self.show_emmet_input = false;
            self.emmet_replacement_range = None;
            self.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
            cx.notify();
        }
    }
}
