use chrono::{Local, NaiveDate};
use gpui::{StyledImage as _, prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
    checkbox::Checkbox,
    date_picker::DatePicker,
    h_flex,
    input::Input,
    menu::DropdownMenu as _,
    popover::Popover,
    scroll::ScrollableElement as _,
    v_flex,
};

mod attachments;
mod card;
mod checklist;
mod description;
mod form;
mod labels;
mod layout;
mod metadata;
mod overlay;
mod properties;

use super::BoardView;
use super::action::*;
use super::drag::*;
use super::dto::EntryDTO;
use super::due_date::{DueDateStatus, due_date_status};
use super::filters::{DueDateFilter, matches_custom_filters, matches_filters};
use crate::color_contrast::accessible_text_colors;

impl Render for BoardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let board = div()
            .id("board-view")
            .relative()
            .size_full()
            .overflow_hidden()
            .on_action(cx.listener(Self::on_delete_card_action))
            .on_action(cx.listener(Self::on_edit_card_action))
            .on_action(cx.listener(Self::on_duplicate_card_action))
            .on_action(cx.listener(Self::on_delete_entry_action))
            .on_action(cx.listener(Self::on_duplicate_entry_action))
            .on_action(cx.listener(Self::on_move_entry_action))
            .on_action(cx.listener(Self::on_rename_board_view_action))
            .on_action(cx.listener(Self::on_set_default_board_view_action))
            .on_action(cx.listener(Self::on_delete_board_view_action));

        let Some(board_id_for_render) = self.board_id else {
            return board.child(self.render_scrollable_board(None, cx));
        };

        board
            .child(self.render_scrollable_board(Some(board_id_for_render), cx))
            .when(self.is_entry_open && self.entry_dialog.open, |this| {
                this.child(self.render_entry_detail_overlay(cx))
            })
    }
}
