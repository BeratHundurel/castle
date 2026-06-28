use gpui::Action;
use serde::Deserialize;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct CommandPaletteAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct OpenWorkspaceSearchAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct CloseCommandPaletteAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct SelectPrevCommandPaletteItem;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub(crate) struct SelectNextCommandPaletteItem;
