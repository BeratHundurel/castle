use gpui::{Context, EntityInputHandler, Window};
use gpui_component::input::RopeExt;

use super::action::*;
use super::emmet::parse_emmet_abbreviation;
use super::{DocumentEditorView, DocumentKind};

impl DocumentEditorView {
    pub(super) fn on_action_toggle_focus_mode(
        &mut self,
        _: &ToggleFocusMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_focus_mode(cx);
    }

    pub(super) fn on_action_toggle_typewriter_scrolling(
        &mut self,
        _: &ToggleTypewriterScrolling,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_typewriter_scrolling(window, cx);
    }

    pub(super) fn on_action_toggle_zen_mode(
        &mut self,
        _: &ToggleZenMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_zen_mode(window, cx);
    }

    pub(super) fn on_action_toggle_zen_status_bar(
        &mut self,
        _: &ToggleZenStatusBar,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_zen_status_bar(cx);
    }

    pub(super) fn on_action_create_card_from_selection(
        &mut self,
        _: &CreateCardFromSelectionAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.create_card_from_selection(cx);
    }

    pub fn create_card_from_selection(&mut self, cx: &mut Context<Self>) {
        let editor = self.editor.read(cx);
        let selected = editor.selected_value().to_string();
        let source = if selected.trim().is_empty() {
            let text = editor.text().to_string();
            current_line(&text, editor.cursor()).to_string()
        } else {
            selected
        };
        let Some(title) = card_title_from_source(&source) else {
            return;
        };
        cx.emit(super::DocumentEditorEvent::CreateCardFromSelection {
            note_id: self.note_id,
            title,
        });
    }

    pub(super) fn on_action_insert_board_view(
        &mut self,
        _: &InsertBoardViewAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_insert_board_view(cx);
    }

    pub fn request_insert_board_view(&mut self, cx: &mut Context<Self>) {
        if self.kind == DocumentKind::Markdown {
            cx.emit(super::DocumentEditorEvent::InsertBoardView {
                note_id: self.note_id,
            });
        }
    }

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
        if self.analysis.outline_rows.is_empty() {
            return;
        }
        self.analysis.outline_selected = Some(
            self.analysis
                .outline_selected
                .unwrap_or(0)
                .saturating_sub(1),
        );
        if let Some(index) = self.analysis.outline_selected {
            self.analysis
                .outline_scroll_handle
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
        if self.analysis.outline_rows.is_empty() {
            return;
        }
        let next = self
            .analysis
            .outline_selected
            .unwrap_or(0)
            .saturating_add(1)
            .min(self.analysis.outline_rows.len().saturating_sub(1));
        self.analysis.outline_selected = Some(next);
        self.analysis
            .outline_scroll_handle
            .scroll_to_item(next, gpui::ScrollStrategy::Bottom);
        cx.notify();
    }

    pub(super) fn on_action_outline_left(
        &mut self,
        _: &OutlineLeft,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = self.analysis.outline_selected else {
            return;
        };
        let Some(row) = self.analysis.outline_rows.get(selected) else {
            return;
        };
        let Some(node_index) = row.node_index else {
            return;
        };

        if row.expanded && self.analysis.outline.collapse(node_index) {
            self.rebuild_outline_rows();
        } else if let Some(parent_row) = self.analysis.outline.parent_row_index(node_index) {
            self.analysis.outline_selected = Some(parent_row);
        }
        cx.notify();
    }

    pub(super) fn on_action_outline_right(
        &mut self,
        _: &OutlineRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(selected) = self.analysis.outline_selected else {
            return;
        };
        let Some(row) = self.analysis.outline_rows.get(selected) else {
            return;
        };
        let Some(node_index) = row.node_index else {
            return;
        };

        if row.has_children && !row.expanded && self.analysis.outline.expand(node_index) {
            self.rebuild_outline_rows();
        } else if let Some(child_row) = self.analysis.outline.first_child_row_index(node_index) {
            self.analysis.outline_selected = Some(child_row);
        }
        cx.notify();
    }

    pub(super) fn on_action_outline_open(
        &mut self,
        _: &OutlineOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self.analysis.outline_selected {
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

    pub(super) fn on_action_format_document(
        &mut self,
        _: &FormatDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.format_document(window, cx);
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
        if self.kind != DocumentKind::Markdown
            || (self.vim_is_enabled() && self.vim_mode() != super::vim::VimMode::Insert)
        {
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

fn current_line(text: &str, cursor: usize) -> &str {
    let cursor = cursor.min(text.len());
    let start = text[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let end = text[cursor..]
        .find('\n')
        .map_or(text.len(), |index| cursor + index);
    &text[start..end]
}

fn card_title_from_source(source: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let mut line = line.trim();
        while let Some(rest) = line.strip_prefix('#').or_else(|| line.strip_prefix('>')) {
            line = rest.trim_start();
        }
        for marker in ["- [ ] ", "- [x] ", "- [X] ", "- ", "* ", "+ "] {
            if let Some(rest) = line.strip_prefix(marker) {
                line = rest.trim_start();
                break;
            }
        }
        if let Some(separator) = line.find(['.', ')']) {
            let prefix = &line[..separator];
            if !prefix.is_empty() && prefix.chars().all(|character| character.is_ascii_digit()) {
                line = line[separator + 1..].trim_start();
            }
        }
        (!line.is_empty()).then(|| line.to_string())
    })
}

#[cfg(test)]
mod integration_action_tests {
    use super::card_title_from_source;

    #[test]
    fn card_title_strips_common_markdown_markers() {
        assert_eq!(
            card_title_from_source("\n## - [ ] Research API\nMore"),
            Some("Research API".to_string())
        );
        assert_eq!(
            card_title_from_source("12. Ship the release"),
            Some("Ship the release".to_string())
        );
    }
}
