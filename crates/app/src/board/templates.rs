use gpui::{AppContext as _, Context, ParentElement, Styled, Window};
use gpui_component::{
    WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::{
        DialogAction, DialogClose, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
    },
    input::{Input, InputState},
    notification::Notification,
    v_flex,
};

use super::BoardView;
use crate::DB;

impl BoardView {
    pub(super) fn show_save_template_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.board_id else {
            return;
        };
        if window.has_active_dialog(cx) {
            return;
        }

        let name_input = cx.new(|cx| InputState::new(window, cx).placeholder("Template name"));
        let dialog_input = name_input.clone();
        let board_view = cx.entity();

        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .w(gpui::px(520.))
                .on_ok({
                    let board_view = board_view.clone();
                    let name_input = dialog_input.clone();
                    move |_, window, cx| {
                        let name = name_input.read(cx).text().to_string().trim().to_string();
                        if name.is_empty() {
                            window.push_notification(
                                Notification::error("Enter a template name."),
                                cx,
                            );
                            name_input.update(cx, |input, cx| input.focus(window, cx));
                            return false;
                        }

                        let db = cx.global::<DB>().conn.clone();
                        let runtime = cx.global::<DB>().runtime.clone();
                        board_view.update(cx, |_, cx| {
                            cx.spawn_in(window, async move |_, window| {
                                let result = runtime
                                    .spawn(async move {
                                        storage::board_templates::save_board_as_template(
                                            db.as_ref(),
                                            board_id,
                                            name,
                                        )
                                        .await
                                    })
                                    .await;
                                window
                                    .update(|window, cx| match result {
                                        Ok(Ok(template)) => window.push_notification(
                                            Notification::success(format!(
                                                "Saved “{}” as a board template.",
                                                template.name
                                            )),
                                            cx,
                                        ),
                                        Ok(Err(error)) => window.push_notification(
                                            Notification::error(format!(
                                                "Could not save the template: {error}"
                                            )),
                                            cx,
                                        ),
                                        Err(error) => window.push_notification(
                                            Notification::error(format!(
                                                "Could not finish saving the template: {error}"
                                            )),
                                            cx,
                                        ),
                                    })
                                    .ok();
                            })
                            .detach();
                        });
                        true
                    }
                })
                .child(
                    DialogHeader::new()
                        .mb_2()
                        .child(DialogTitle::new().child("Save as template"))
                        .child(DialogDescription::new().child(
                            "Reuse this board’s columns and cards when creating a new board.",
                        )),
                )
                .child(v_flex().mb_3().child(Input::new(&dialog_input)))
                .child(
                    DialogFooter::new()
                        .justify_between()
                        .child(
                            DialogClose::new().child(
                                Button::new("cancel-save-board-template")
                                    .label("Cancel")
                                    .outline(),
                            ),
                        )
                        .child(
                            DialogAction::new().child(
                                Button::new("confirm-save-board-template")
                                    .label("Save template")
                                    .primary(),
                            ),
                        ),
                )
        });

        name_input.update(cx, |input, cx| input.focus(window, cx));
    }
}
