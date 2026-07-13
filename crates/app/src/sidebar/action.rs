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

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct RenameProjectAction(pub(crate) u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct DeleteProjectAction(pub(crate) u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct ToggleBoardPinnedAction {
    pub(crate) board_id: u32,
    pub(crate) pinned: bool,
}

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct ToggleNotePinnedAction {
    pub(crate) note_id: u32,
    pub(crate) pinned: bool,
}

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct MoveProjectUpAction(pub(crate) u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
pub(crate) struct MoveProjectDownAction(pub(crate) u32);
