use gpui::{Context, Styled, Window};
use gpui_component::{
    ActiveTheme, Icon, IconName, WindowExt,
    button::{Button, ButtonVariant, ButtonVariants as _},
    dialog::DialogButtonProps,
    notification::Notification,
};

use super::{SidebarView, action::*};

struct TrashUndoNotification;

impl SidebarView {
    pub(super) fn on_toggle_board_pinned_action(
        &mut self,
        action: &ToggleBoardPinnedAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_board_pinned(action.board_id, action.pinned, cx);
    }

    pub(super) fn on_toggle_note_pinned_action(
        &mut self,
        action: &ToggleNotePinnedAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_note_pinned(action.note_id, action.pinned, cx);
    }

    pub(super) fn on_delete_board_action(
        &mut self,
        action: &DeleteBoardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = self
            .projects
            .iter()
            .flat_map(|project| project.boards.iter())
            .chain(self.standalone_boards.iter())
            .find(|board| board.id == action.0)
            .map(|board| board.title.clone())
            .unwrap_or_else(|| "this board".into());

        let view = cx.entity();
        let board_id = action.0;
        window.open_alert_dialog(cx, move |alert, _, cx| {
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
                .title(format!("Move board ‘{title}’ to Trash"))
                .description("The board and everything inside it will be hidden until you restore it from Trash.")
                .button_props(DialogButtonProps::default().ok_variant(ButtonVariant::Danger).ok_text("Move to Trash").cancel_text("Cancel").show_cancel(true))
                .on_ok({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| this.delete_board(cx, board_id));
                        true
                    }
                })
        });
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = self
            .projects
            .iter()
            .flat_map(|project| project.notes.iter())
            .chain(self.standalone_notes.iter())
            .find(|note| note.id == action.0)
            .map(|note| note.title.clone())
            .unwrap_or_else(|| "Note".into());
        self.delete_note(action.0, title, window, cx);
    }

    pub(super) fn on_edit_note_action(
        &mut self,
        action: &EditNoteAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_renaming_note(action, window, cx);
    }

    pub(super) fn on_rename_project_action(
        &mut self,
        action: &RenameProjectAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_renaming_project(action, window, cx);
    }

    pub(super) fn on_delete_project_action(
        &mut self,
        action: &DeleteProjectAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.projects.iter().find(|project| project.id == action.0) else {
            return;
        };
        let title = project.name.clone();
        let child_count = project.boards.len() + project.notes.len();
        let view = cx.entity();
        let project_id = action.0;
        window.open_alert_dialog(cx, move |alert, _, cx| {
            alert
                .icon(Icon::new(IconName::TriangleAlert).text_color(cx.theme().danger))
                .title(format!("Move project ‘{title}’ to Trash"))
                .description(format!(
                    "This hides the project and its {child_count} item(s) until you restore it."
                ))
                .button_props(
                    DialogButtonProps::default()
                        .ok_variant(ButtonVariant::Danger)
                        .ok_text("Move to Trash")
                        .cancel_text("Cancel")
                        .show_cancel(true),
                )
                .on_ok({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| this.delete_project(cx, project_id));
                        true
                    }
                })
        });
    }

    pub(super) fn push_trash_undo(
        &self,
        kind: crate::trash::TrashItemKind,
        id: u32,
        title: gpui::SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sidebar = cx.entity();
        window.push_notification(
            Notification::info(format!("Moved {title} to Trash"))
                .id::<TrashUndoNotification>()
                .action(move |_, _, cx| {
                    let sidebar = sidebar.clone();
                    Button::new("undo-move-to-trash")
                        .label("Undo")
                        .primary()
                        .on_click(cx.listener(move |notification, _, window, cx| {
                            sidebar.update(cx, |this, cx| this.restore_trashed(kind, id, cx));
                            notification.dismiss(window, cx);
                        }))
                })
                .autohide(true),
            cx,
        );
    }

    pub(super) fn on_move_project_up_action(
        &mut self,
        action: &MoveProjectUpAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_project_up(cx, action.0);
    }

    pub(super) fn on_move_project_down_action(
        &mut self,
        action: &MoveProjectDownAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_project_down(cx, action.0);
    }
}
