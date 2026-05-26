use gpui::{Context, Window};

use super::{BoardView, action::*};

impl BoardView {
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
}
