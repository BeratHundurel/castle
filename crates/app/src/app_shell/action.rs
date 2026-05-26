use gpui::Action;
use serde::Deserialize;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(super) struct CloseTabAction(pub(super) u64);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(super) struct CloseOtherTabsAction(pub(super) u64);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(super) struct CloseAllTabsAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(super) struct CycleNextTab;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(super) struct CyclePrevTab;
