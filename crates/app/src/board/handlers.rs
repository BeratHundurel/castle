use chrono::NaiveDate;
use gpui::{Context, Styled, Window};
use gpui_component::{
    ActiveTheme, Icon, IconName, WindowExt, button::ButtonVariant, calendar::Date,
    dialog::DialogButtonProps,
};

use super::filters::DueDateFilter;
use super::{BoardView, action::*};

impl BoardView {
    pub(super) fn start_adding_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.is_adding_list = true;
        self.new_list_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn start_managing_labels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.entry_dialog.managing_labels = true;
        self.renaming_label_id = None;
        self.new_label_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn stop_managing_labels(&mut self, cx: &mut Context<Self>) {
        self.entry_dialog.managing_labels = false;
        self.renaming_label_id = None;
        cx.notify();
    }

    pub(super) fn start_renaming_board_label(
        &mut self,
        label_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(label) = self.board_labels.iter().find(|label| label.id == label_id) else {
            return;
        };

        self.renaming_label_id = Some(label_id);
        self.rename_label_input.update(cx, |input, cx| {
            input.set_value(label.name.clone(), window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn select_label_color(&mut self, color: &str, cx: &mut Context<Self>) {
        self.selected_label_color = color.into();
        cx.notify();
    }

    pub(super) fn set_filter_panel_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.filter_panel_open = open;
        cx.notify();
    }

    pub(super) fn set_label_filter(
        &mut self,
        label_id: u32,
        selected: bool,
        cx: &mut Context<Self>,
    ) {
        if selected {
            self.filters.label_ids.insert(label_id);
        } else {
            self.filters.label_ids.remove(&label_id);
        }
        cx.notify();
    }

    pub(super) fn set_due_date_filter(
        &mut self,
        filter: DueDateFilter,
        selected: bool,
        cx: &mut Context<Self>,
    ) {
        if selected {
            self.filters.due_dates.insert(filter);
        } else {
            self.filters.due_dates.remove(&filter);
        }
        cx.notify();
    }

    pub(super) fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.filters.clear();
        cx.notify();
    }

    pub(super) fn focus_checklist_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_checklist_item_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    }

    pub(super) fn start_renaming_checklist_item(
        &mut self,
        item_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self
            .cards
            .iter()
            .flat_map(|list| list.entries.iter())
            .flat_map(|card| card.checklist_items.iter())
            .find(|item| item.id == item_id)
        else {
            return;
        };
        self.renaming_checklist_item_id = Some(item_id);
        self.rename_checklist_item_input.update(cx, |input, cx| {
            input.set_value(item.title.clone(), window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn open_entry_dialog(
        &mut self,
        entry_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_entry_open = true;
        self.entry_dialog.open = true;
        self.entry_dialog.entry_id = Some(entry_id);
        self.entry_dialog.editing = false;
        self.entry_dialog.managing_labels = false;
        self.sync_entry_edit_inputs(window, cx);
        cx.notify();
    }

    pub(super) fn close_entry_dialog(&mut self, cx: &mut Context<Self>) {
        self.is_entry_open = false;
        self.entry_dialog.open = false;
        self.entry_dialog.entry_id = None;
        self.entry_dialog.editing = false;
        self.entry_dialog.managing_labels = false;
        cx.notify();
    }

    pub(super) fn start_editing_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.entry_dialog.editing = true;
        self.sync_entry_edit_inputs(window, cx);
        self.entry_title_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn cancel_editing_entry(&mut self, cx: &mut Context<Self>) {
        self.entry_dialog.editing = false;
        cx.notify();
    }

    fn sync_entry_edit_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((title, description, due_on)) = self
            .entry_dialog
            .entry_id
            .and_then(|entry_id| self.entry_values(entry_id))
        else {
            return;
        };

        self.entry_title_input.update(cx, |input, cx| {
            input.set_value(title, window, cx);
        });
        self.entry_description_input.update(cx, |input, cx| {
            input.set_value(description, window, cx);
        });
        self.due_date_picker.update(cx, |picker, cx| {
            let due_on = due_on
                .as_deref()
                .and_then(|value| NaiveDate::parse_from_str(value, "%Y-%m-%d").ok());
            picker.set_date(Date::Single(due_on), window, cx);
        });
    }

    pub(super) fn on_delete_card_action(
        &mut self,
        action: &DeleteCardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(list) = self.cards.iter().find(|list| list.id == action.0) else {
            return;
        };
        let title = list.title.clone();
        let card_count = list.entries.len();
        let view = cx.entity();
        let list_id = action.0;
        window.open_alert_dialog(cx, move |alert, _, cx| {
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
                .title(format!("Move list ‘{title}’ to Trash"))
                .description(format!(
                    "This hides the list and its {card_count} card(s) until you restore it from Trash."
                ))
                .button_props(
                    DialogButtonProps::default()
                        .ok_variant(ButtonVariant::Danger)
                        .ok_text("Move to Trash")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| this.delete_card(cx, list_id));
                        true
                    }
                })
        });
    }

    pub(super) fn on_duplicate_card_action(
        &mut self,
        action: &DuplicateCardAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.duplicate_card(action.0, cx);
    }

    pub(super) fn start_renaming_card(
        &mut self,
        action: &EditCardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(card) = self.cards.iter().find(|card| card.id == action.0) else {
            return;
        };

        self.renaming_card_id = Some(card.id);
        self.rename_card_input.update(cx, |input, cx| {
            input.set_value(card.title.clone(), window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    pub(super) fn on_edit_card_action(
        &mut self,
        action: &EditCardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_renaming_card(action, window, cx)
    }

    pub(super) fn on_delete_entry_action(
        &mut self,
        _: &DeleteEntryAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };
        let Some((title, _, _)) = self.entry_values(entry_id) else {
            return;
        };
        let view = cx.entity();
        window.open_alert_dialog(cx, move |alert, _, cx| {
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
                .title(format!("Move card ‘{title}’ to Trash"))
                .description(
                    "This hides the card and its checklist until you restore it from Trash.",
                )
                .button_props(
                    DialogButtonProps::default()
                        .ok_variant(ButtonVariant::Danger)
                        .ok_text("Move to Trash")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| this.delete_selected_entry(cx));
                        true
                    }
                })
        });
    }

    pub(super) fn on_duplicate_entry_action(
        &mut self,
        _: &DuplicateEntryAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.duplicate_selected_entry(cx);
    }

    pub(super) fn on_move_entry_action(
        &mut self,
        action: &MoveEntryAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_entry_to_list_end(action.entry_id, action.target_card_id, cx);
    }
}
