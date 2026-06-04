use gpui::{Context, Window};

use super::{BoardView, action::*};

impl BoardView {
    pub(super) fn open_entry_dialog(
        &mut self,
        entry_id: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_entry_open = true;
        self.entry_dialog.open = true;
        self.entry_dialog.entry_id = Some(entry_id);
        self.entry_dialog.editing = false;
        self.sync_entry_edit_inputs(window, cx);
        cx.notify();
    }

    pub(super) fn close_entry_dialog(&mut self, cx: &mut Context<Self>) {
        self.is_entry_open = false;
        self.entry_dialog.open = false;
        self.entry_dialog.entry_id = None;
        self.entry_dialog.editing = false;
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
        let Some((title, description)) = self
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
    }

    pub(super) fn on_delete_card_action(
        &mut self,
        action: &DeleteCardAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_card(cx, action.0)
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
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_selected_entry(cx);
    }
}
