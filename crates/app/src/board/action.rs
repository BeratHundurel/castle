use gpui::Action;
use serde::Deserialize;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = board, no_json)]
pub(crate) struct DeleteCardAction(pub(crate) u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = board, no_json)]
pub(crate) struct EditCardAction(pub(crate) u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = board, no_json)]
pub(crate) struct DuplicateCardAction(pub(crate) u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = board, no_json)]
pub(crate) struct DeleteEntryAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = board, no_json)]
pub(crate) struct DuplicateEntryAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = board, no_json)]
pub(crate) struct MoveEntryAction {
    pub(crate) entry_id: u32,
    pub(crate) target_card_id: u32,
}
