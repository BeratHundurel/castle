use chrono::{Local, NaiveDate};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable, WindowExt as _,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    checkbox::Checkbox,
    date_picker::DatePicker,
    h_flex,
    input::{Editor, Input},
    menu::DropdownMenu as _,
    notification::Notification,
    popover::Popover,
    scroll::ScrollableElement as _,
    text::{TextView, TextViewStyle},
    v_flex,
};

mod card;
mod checklist;
mod description;
mod form;
mod labels;
mod layout;
mod metadata;
mod overlay;

use super::BoardView;
use super::action::*;
use super::color_contrast::accessible_text_colors;
use super::drag::*;
use super::due_date::{DueDateStatus, due_date_status};
use super::filters::DueDateFilter;
use super::model::BoardCardState;
use workspace::WorkspaceDragInfo;

impl Render for BoardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let board = div()
            .id("board-view")
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .overflow_hidden()
            .on_action(cx.listener(Self::on_delete_card_action))
            .on_action(cx.listener(Self::on_edit_card_action))
            .on_action(cx.listener(Self::on_duplicate_card_action))
            .on_action(cx.listener(Self::on_copy_list_internal_link_action))
            .on_action(cx.listener(Self::on_copy_card_internal_link_action))
            .on_action(cx.listener(Self::on_copy_board_internal_link_action))
            .on_action(cx.listener(Self::on_delete_entry_action))
            .on_action(cx.listener(Self::on_duplicate_entry_action))
            .on_action(cx.listener(Self::on_move_entry_action))
            .on_action(cx.listener(Self::on_rename_board_view_action))
            .on_action(cx.listener(Self::on_set_default_board_view_action))
            .on_action(cx.listener(Self::on_delete_board_view_action));

        let Some(board_id_for_render) = self.data.board_id else {
            return board.child(self.render_scrollable_board(None, cx));
        };

        board
            .child(self.render_scrollable_board(Some(board_id_for_render), cx))
            .when(
                self.entry_editing.open && self.entry_editing.dialog.open,
                |this| this.child(self.render_entry_detail_overlay(cx)),
            )
    }
}
