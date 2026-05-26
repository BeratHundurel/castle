use gpui::{Context, Window};

use super::{SidebarView, action::*};

impl SidebarView {
    pub(super) fn on_delete_board_action(
        &mut self,
        action: &DeleteBoardAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_board(cx, action.0);
    }

    pub(super) fn on_edit_board_action(
        &mut self,
        action: &EditBoardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_renaming_board(action, window, cx);
    }

    pub(super) fn on_move_board_action(
        &mut self,
        action: &MoveBoardAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_board(cx, action.board_id, action.project_id);
    }

    pub(super) fn on_move_note_action(
        &mut self,
        action: &MoveNoteAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_note(cx, action.note_id, action.project_id);
    }

    pub(super) fn on_delete_note_action(
        &mut self,
        action: &DeleteNoteAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_note(cx, action.0);
    }

    pub(super) fn on_edit_note_action(
        &mut self,
        action: &EditNoteAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_renaming_note(action, window, cx);
    }
}
