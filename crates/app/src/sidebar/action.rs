use gpui::Action;
use serde::Deserialize;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct DeleteBoardAction(pub(crate) u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct EditBoardAction(pub(crate) u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct MoveBoardAction {
    pub(crate) board_id: u32,
    pub(crate) project_id: Option<u32>,
}

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct MoveNoteAction {
    pub(crate) note_id: u32,
    pub(crate) project_id: Option<u32>,
}

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct DeleteNoteAction(pub(crate) u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct EditNoteAction(pub(crate) u32);
