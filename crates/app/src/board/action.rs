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
