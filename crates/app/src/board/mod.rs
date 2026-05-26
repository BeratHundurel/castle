mod action;
mod drag;
mod dto;
mod handlers;
mod interactions;
mod persistence;
mod render;

use dto::*;
use gpui::*;
use gpui_component::input::{InputEvent, InputState};

pub(crate) struct BoardView {
    board_id: Option<u32>,
    cards: Vec<CardDTO>,
    is_adding_list: bool,
    new_list_input: Entity<InputState>,
    dialog_title_input: Entity<InputState>,
    dialog_description_input: Entity<InputState>,
    rename_card_input: Entity<InputState>,
    renaming_card_id: Option<u32>,
    pending_card_id: Option<u32>,
    next_temporary_card_id: u32,
    next_temporary_entry_id: u32,
}

impl BoardView {
    pub(crate) fn view(window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::new(window, cx))
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let new_list_input = cx.new(|cx| InputState::new(window, cx).placeholder("List name..."));

        let dialog_title_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Give your title"));

        let dialog_description_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Give your description")
                .multi_line(true)
                .auto_grow(3, 24)
                .soft_wrap(true)
                .searchable(true)
        });

        let card_edit_input = cx.new(|cx| InputState::new(window, cx).placeholder("Edit title..."));

        cx.subscribe(
            &new_list_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let name = text.trim();
                    if let Some(board_id) = this.board_id
                        && !name.is_empty()
                    {
                        let card_id = this.next_card_id();
                        this.is_adding_list = false;
                        this.add_card(
                            cx,
                            CardDTO {
                                id: card_id,
                                title: SharedString::from(name),
                                board_id,
                                position: this.cards.len() as i32,
                                entries: vec![],
                            },
                            card_id,
                        );
                    } else {
                        this.is_adding_list = false;
                        cx.notify();
                    }
                }
                InputEvent::Blur => {
                    this.is_adding_list = false;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        cx.subscribe(
            &card_edit_input,
            |this: &mut Self, input, event: &InputEvent, cx| match event {
                InputEvent::PressEnter { .. } => {
                    let text = input.read(cx).text().to_string();
                    let name = text.trim();
                    if !name.is_empty() {
                        this.rename_card(cx, name);
                    } else {
                        this.renaming_card_id = None;
                        cx.notify();
                    }
                }
                InputEvent::Blur => {
                    this.renaming_card_id = None;
                    cx.notify();
                }
                _ => {}
            },
        )
        .detach();

        Self {
            board_id: None,
            cards: vec![],
            is_adding_list: false,
            new_list_input,
            dialog_title_input,
            dialog_description_input,
            rename_card_input: card_edit_input,
            renaming_card_id: None,
            pending_card_id: None,
            next_temporary_card_id: 0,
            next_temporary_entry_id: 0,
        }
    }
}
