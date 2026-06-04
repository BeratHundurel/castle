use gpui::Action;
use serde::Deserialize;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct CloseTabAction(pub(crate) u64);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct CloseOtherTabsAction(pub(crate) u64);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct CloseAllTabsAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct CycleNextTab;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct CyclePrevTab;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct ToggleSidebarAction;
