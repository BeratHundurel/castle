use gpui_kit::Action;
use serde::Deserialize;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub struct CommandPaletteAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub struct OpenWorkspaceSearchAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub struct SwitchThemeAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub struct CloseCommandPaletteAction;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub struct SelectPrevCommandPaletteItem;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = castle, no_json)]
pub struct SelectNextCommandPaletteItem;
