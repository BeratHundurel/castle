use anyhow::Result;
use entity::{card, card::Entity as Card, entry, entry::Entity as Entry};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Icon, IconName, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dialog::{
        DialogAction, DialogClose, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
    },
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::DropdownMenu as _,
    scroll::ScrollableElement,
    v_flex,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, QueryFilter, QueryOrder};
use sea_orm::{ActiveValue::Set, EntityTrait};
use serde::Deserialize;
use std::rc::Rc;

use crate::DB;

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
struct DeleteCardAction(u32);

#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = sidebar, no_json)]
struct EditCardAction(u32);

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

#[derive(Clone, PartialEq, Eq)]
struct DragInfo {
    entry_id: u32,
    source_board_id: u32,
    source_card_id: u32,
    position: Point<Pixels>,
    title: SharedString,
}

#[derive(Clone, PartialEq, Eq)]
struct CardDragInfo {
    card_id: u32,
    source_board_id: u32,
    position: Point<Pixels>,
    title: SharedString,
}

impl DragInfo {
    fn new(entry_id: u32, source_board_id: u32, source_card_id: u32, title: SharedString) -> Self {
        Self {
            entry_id,
            source_board_id,
            source_card_id,
            position: Point::default(),
            title,
        }
    }

    fn position(mut self, pos: Point<Pixels>) -> Self {
        self.position = pos;
        self
    }
}

impl Render for DragInfo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let size = gpui::size(px(200.), px(40.));

        div()
            .pl(self.position.x - size.width.half())
            .pt(self.position.y - size.height.half())
            .child(
                div()
                    .flex()
                    .justify_start()
                    .items_center()
                    .w(size.width)
                    .h(size.height)
                    .p_2()
                    .bg(cx.theme().primary.opacity(0.7))
                    .text_color(cx.theme().primary_foreground)
                    .rounded(cx.theme().radius)
                    .text_sm()
                    .shadow_md()
                    .child(self.title.clone()),
            )
    }
}

impl CardDragInfo {
    fn new(card_id: u32, source_board_id: u32, title: SharedString) -> Self {
        Self {
            card_id,
            source_board_id,
            position: Point::default(),
            title,
        }
    }

    fn position(mut self, pos: Point<Pixels>) -> Self {
        self.position = pos;
        self
    }
}

impl Render for CardDragInfo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let size = gpui::size(px(320.), px(56.));

        div()
            .pl(self.position.x - size.width.half())
            .pt(self.position.y - size.height.half())
            .child(
                h_flex()
                    .w(size.width)
                    .h(size.height)
                    .gap_2()
                    .items_center()
                    .p_3()
                    .bg(cx.theme().secondary.opacity(0.92))
                    .text_color(cx.theme().secondary_foreground)
                    .border_1()
                    .border_color(cx.theme().primary)
                    .rounded(cx.theme().radius)
                    .shadow_lg()
                    .child("⋮⋮")
                    .child(self.title.clone()),
            )
    }
}

#[derive(Clone, PartialEq, Eq)]
struct CardDTO {
    id: u32,
    title: SharedString,
    board_id: u32,
    position: i32,
    entries: Vec<EntryDTO>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EntryDTO {
    id: u32,
    title: SharedString,
    description: SharedString,
    card_id: u32,
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
                            board_id,
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

    pub(crate) fn load_board(&mut self, board_id: u32, cx: &mut Context<Self>) {
        if self.board_id == Some(board_id) {
            return;
        }

        self.board_id = Some(board_id);
        self.cards.clear();
        self.is_adding_list = false;
        Self::enrich_board_async(cx, board_id);
    }

    fn enrich_board_async(cx: &mut Context<Self>, board_id: u32) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| -> Result<()> {
            let result = Card::load()
                .filter(card::Column::BoardId.eq(board_id as i32))
                .order_by_asc(card::Column::Position)
                .order_by_asc(card::Column::Id)
                .with(Entry)
                .all(&*db)
                .await?;

            let cards: Vec<CardDTO> = result
                .into_iter()
                .map(|c| CardDTO {
                    id: c.id as u32,
                    board_id: c.board_id as u32,
                    title: SharedString::from(c.title),
                    position: c.position,
                    entries: c
                        .entries
                        .into_iter()
                        .map(|e| EntryDTO {
                            id: e.id as u32,
                            title: SharedString::from(e.title),
                            description: SharedString::from(e.description),
                            card_id: e.card_id as u32,
                        })
                        .collect(),
                })
                .collect();

            this.update(cx, |this, cx| {
                if this.board_id == Some(board_id) {
                    this.cards = cards;
                    cx.notify();
                }
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn next_card_id(&mut self) -> u32 {
        self.next_temporary_card_id = self.next_temporary_card_id.saturating_add(1);
        u32::MAX.saturating_sub(self.next_temporary_card_id)
    }

    fn next_entry_id(&mut self) -> u32 {
        self.next_temporary_entry_id = self.next_temporary_entry_id.saturating_add(1);
        u32::MAX.saturating_sub(self.next_temporary_entry_id)
    }

    fn add_entry(&mut self, cx: &mut Context<Self>, entry: EntryDTO, card_id: u32, temp_id: u32) {
        let db = cx.global::<DB>().conn.clone();

        if let Some(card) = self.cards.iter_mut().find(|card| card.id == card_id) {
            card.entries.push(entry.clone());
            cx.notify();
        }

        cx.spawn(async move |this, cx| -> Result<()> {
            let model = entry::ActiveModel {
                title: Set(entry.title.to_string()),
                description: Set(entry.description.to_string()),
                card_id: Set(entry.card_id as i64),
                ..Default::default()
            };
            let inserted = model.insert(&*db).await?;
            let real_id = inserted.id as u32;

            this.update(cx, |this, _cx| {
                if let Some(entry) = this
                    .cards
                    .iter_mut()
                    .find(|card| card.id == card_id)
                    .and_then(|card| card.entries.iter_mut().find(|entry| entry.id == temp_id))
                {
                    entry.id = real_id;
                }
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn add_card(&mut self, cx: &mut Context<Self>, card: CardDTO, board_id: u32, temp_id: u32) {
        let db = cx.global::<DB>().conn.clone();

        self.cards.push(card.clone());
        cx.notify();

        cx.spawn(async move |this, cx| -> Result<()> {
            let model = card::ActiveModel {
                title: Set(card.title.to_string()),
                board_id: Set(board_id as i64),
                position: Set(card.position),
                ..Default::default()
            };
            let inserted = model.insert(&*db).await?;
            let real_id = inserted.id as u32;

            this.update(cx, |this, _cx| {
                if this.board_id == Some(board_id)
                    && let Some(card) = this.cards.iter_mut().find(|card| card.id == temp_id)
                {
                    card.id = real_id;
                }
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    fn rename_card(&mut self, cx: &mut Context<Self>, new_title: &str) {
        let Some(card_id) = self.renaming_card_id else {
            return;
        };

        let title = new_title.to_string();
        let db = cx.global::<DB>().conn.clone();

        let Some(card) = self.cards.iter_mut().find(|card| card.id == card_id) else {
            return;
        };

        card.title = SharedString::from(title.clone());
        self.renaming_card_id = None;
        cx.notify();

        cx.spawn(async move |_, _| -> Result<()> {
            let model = card::ActiveModel {
                id: Set(card_id as i64),
                title: Set(title),
                ..Default::default()
            };
            model.update(&*db).await?;
            Ok(())
        })
        .detach();
    }

    fn show_add_entry_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog_title_input = self.dialog_title_input.clone();
        let dialog_description_input = self.dialog_description_input.clone();

        let confirm_handler = Rc::new(cx.listener(move |this, _, _, cx| {
            let Some(card_id) = this.pending_card_id else {
                return;
            };

            let title = this.dialog_title_input.read(cx).text().to_string();
            let description = this.dialog_description_input.read(cx).text().to_string();
            let entry_id = this.next_entry_id();
            let entry = EntryDTO {
                id: entry_id,
                title: SharedString::from(title),
                description: SharedString::from(description),
                card_id,
            };
            this.pending_card_id = None;
            this.add_entry(cx, entry, card_id, entry_id);
        }));

        window.open_dialog(cx, move |dialog, _window, _cx| {
            let confirm_handler = confirm_handler.clone();
            dialog
                .on_ok(move |e, window, cx| {
                    (confirm_handler)(e, window, cx);
                    true
                })
                .child(
                    DialogHeader::new()
                        .mb_2()
                        .child(DialogTitle::new().child("Add a new entry"))
                        .child(DialogDescription::new().child("Enter the information needed")),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .mb_3()
                        .child(Input::new(&dialog_title_input))
                        .child(Input::new(&dialog_description_input)),
                )
                .child(
                    DialogFooter::new()
                        .justify_between()
                        .child(DialogClose::new().child(
                            Button::new("cancel").label("Cancel").outline().on_click({
                                move |_, window, cx| {
                                    window.close_dialog(cx);
                                }
                            }),
                        ))
                        .child(
                            DialogAction::new()
                                .child(Button::new("confirm").primary().label("Confirm")),
                        ),
                )
        });
    }

    fn move_entry(&mut self, info: &DragInfo, target_card_id: u32, cx: &mut Context<Self>) {
        let Some(board_id) = self.board_id else {
            return;
        };

        if info.source_board_id == board_id && info.source_card_id == target_card_id {
            return;
        }

        if !self.cards.iter().any(|card| card.id == target_card_id) {
            return;
        }

        let mut moving_entry = None;
        if let Some(source_card) = self
            .cards
            .iter_mut()
            .find(|card| card.id == info.source_card_id)
            && let Some(index) = source_card
                .entries
                .iter()
                .position(|entry| entry.id == info.entry_id)
        {
            moving_entry = Some(source_card.entries.remove(index));
        }

        if let Some(mut dto) = moving_entry
            && let Some(target_card) = self.cards.iter_mut().find(|card| card.id == target_card_id)
        {
            dto.card_id = target_card_id;
            target_card.entries.push(dto);
            cx.notify();

            let db = cx.global::<DB>().conn.clone();
            let entry_id = info.entry_id;
            cx.spawn(async move |_, _| -> Result<()> {
                let model = entry::ActiveModel {
                    id: Set(entry_id as i64),
                    card_id: Set(target_card_id as i64),
                    ..Default::default()
                };

                model.update(&*db).await?;

                Ok(())
            })
            .detach();
        }
    }

    fn persist_card_positions(&mut self, cx: &mut Context<Self>) {
        let positions: Vec<(u32, i32)> = self
            .cards
            .iter_mut()
            .enumerate()
            .map(|(index, card)| {
                card.position = index as i32;
                (card.id, card.position)
            })
            .collect();

        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        cx.spawn(async move |_, _| -> Result<()> {
            for (card_id, position) in positions {
                let model = card::ActiveModel {
                    id: Set(card_id as i64),
                    position: Set(position),
                    ..Default::default()
                };
                model.update(&*db).await?;
            }

            Ok(())
        })
        .detach();
    }

    fn move_card(&mut self, info: &CardDragInfo, target_card_id: u32, cx: &mut Context<Self>) {
        let Some(board_id) = self.board_id else {
            return;
        };

        if info.source_board_id != board_id || info.card_id == target_card_id {
            return;
        }

        let Some(from_index) = self.cards.iter().position(|card| card.id == info.card_id) else {
            return;
        };
        let Some(to_index) = self.cards.iter().position(|card| card.id == target_card_id) else {
            return;
        };

        let moved_card = self.cards.remove(from_index);
        self.cards.insert(to_index, moved_card);
        self.persist_card_positions(cx);
    }

    fn move_card_to_end(&mut self, info: &CardDragInfo, cx: &mut Context<Self>) {
        let Some(board_id) = self.board_id else {
            return;
        };

        if info.source_board_id != board_id {
            return;
        }

        let Some(from_index) = self.cards.iter().position(|card| card.id == info.card_id) else {
            return;
        };

        if from_index + 1 == self.cards.len() {
            return;
        }

        let moved_card = self.cards.remove(from_index);
        self.cards.push(moved_card);
        self.persist_card_positions(cx);
    }

    fn delete_card(&mut self, cx: &mut Context<Self>, card_id: u32) {
        self.cards.retain(|card| card.id != card_id);
        cx.notify();

        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |_, _| -> Result<()> {
            Card::delete_by_id(card_id as i64).exec(&*db).await?;
            Ok(())
        })
        .detach();
    }

    fn on_delete_card_action(
        &mut self,
        action: &DeleteCardAction,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delete_card(cx, action.0)
    }

    fn start_renaming_card(
        &mut self,
        action: &EditCardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(card) = self.cards.iter().find(|card| card.id == action.0) else {
            return;
        };

        self.renaming_card_id = Some(card.id);
        self.rename_card_input.update(cx, |input, cx| {
            input.set_value(card.title.clone(), window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn on_edit_card_action(
        &mut self,
        action: &EditCardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.start_renaming_card(action, window, cx)
    }
}

impl Render for BoardView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let Some(board_id_for_render) = self.board_id else {
            return h_flex()
                .id("scrollable-container")
                .size_full()
                .overflow_x_scrollbar()
                .gap_4()
                .p_4()
                .items_start();
        };

        h_flex()
            .id("scrollable-container")
            .on_action(cx.listener(Self::on_delete_card_action))
            .on_action(cx.listener(Self::on_edit_card_action))
            .size_full()
            .overflow_x_scrollbar()
            .gap_4()
            .p_4()
            .items_start()
            .children({
                self.cards.iter().map(|card| {
                    let card_id = card.id;
                    let board_id = board_id_for_render;
                    let card_drag_info =
                        CardDragInfo::new(card_id, board_id, card.title.clone().into());

                    v_flex()
                        .id(card.id as usize)
                        .w_80()
                        .min_w_auto()
                        .max_h_3_4()
                        .h_auto()
                        .gap_2()
                        .p_2()
                        .bg(cx.theme().secondary)
                        .text_color(cx.theme().secondary_foreground)
                        .rounded(cx.theme().radius)
                        .drag_over::<DragInfo>(|this, _, _, cx| {
                            this.border_1().border_color(cx.theme().primary)
                        })
                        .on_drop(cx.listener(move |this, info: &DragInfo, _, cx| {
                            this.move_entry(info, card_id, cx);
                        }))
                        .drag_over::<CardDragInfo>(|this, _, _, cx| {
                            this.border_1()
                                .border_color(cx.theme().primary)
                                .bg(cx.theme().secondary_hover)
                                .shadow_lg()
                        })
                        .on_drop(cx.listener(move |this, info: &CardDragInfo, _, cx| {
                            this.move_card(info, card_id, cx);
                        }))
                        .child(
                            h_flex()
                                .id("card-list-title")
                                .p_1()
                                .justify_between()
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_move()
                                .hover(|this| this.text_color(theme.foreground))
                                .on_drag(card_drag_info, |info: &CardDragInfo, position, _, cx| {
                                    cx.new(|_| info.clone().position(position))
                                })
                                .when_else(
                                    self.renaming_card_id == Some(card_id),
                                    |this| {
                                        this.child(
                                            Input::new(&self.rename_card_input)
                                                .bg(theme.secondary)
                                                .focus_bordered(false)
                                                .rounded_none()
                                                .border_0()
                                                .border_b_1()
                                                .border_color(theme.foreground),
                                        )
                                    },
                                    |this| this.child(card.title.clone()),
                                )
                                .child(
                                    Button::new(("card-menu", card_id as usize))
                                        .icon(IconName::Ellipsis)
                                        .ghost()
                                        .compact()
                                        .tooltip("Card actions")
                                        .dropdown_menu_with_anchor(Anchor::LeftCenter, {
                                            move |menu, _, cx| {
                                                let muted = cx.theme().muted_foreground;
                                                menu.menu_element(
                                                    Box::new(EditCardAction(card_id)),
                                                    move |_window, _cx| {
                                                        h_flex()
                                                            .w_full()
                                                            .gap_2()
                                                            .items_center()
                                                            .justify_between()
                                                            .child("Edit")
                                                            .child(
                                                                Icon::new(IconName::Replace)
                                                                    .xsmall()
                                                                    .text_color(muted),
                                                            )
                                                    },
                                                )
                                                .menu_element(
                                                    Box::new(DeleteCardAction(card_id)),
                                                    move |_window, _cx| {
                                                        h_flex()
                                                            .w_full()
                                                            .gap_2()
                                                            .items_center()
                                                            .justify_between()
                                                            .child("Delete")
                                                            .child(
                                                                Icon::new(IconName::Delete)
                                                                    .xsmall()
                                                                    .text_color(muted),
                                                            )
                                                    },
                                                )
                                            }
                                        }),
                                ),
                        )
                        .children(card.entries.iter().map(|entry| {
                            let drag_info = DragInfo::new(
                                entry.id,
                                board_id,
                                card_id,
                                entry.title.clone().into(),
                            );

                            div()
                                .id(entry.id as usize)
                                .p_2()
                                .bg(cx.theme().primary)
                                .text_color(cx.theme().primary_foreground)
                                .rounded(cx.theme().radius)
                                .hover(|this| {
                                    this.bg(cx.theme().primary_hover)
                                        .cursor(CursorStyle::PointingHand)
                                        .border_1()
                                        .border_color(cx.theme().primary_foreground)
                                })
                                .cursor_move()
                                .text_sm()
                                .w_full()
                                .child(entry.title.clone())
                                .on_drag(drag_info, |info: &DragInfo, position, _, cx| {
                                    cx.new(|_| info.clone().position(position))
                                })
                        }))
                        .child(
                            h_flex()
                                .id(("add-item", card_id as usize))
                                .w_full()
                                .rounded(cx.theme().radius)
                                .gap_2()
                                .p_1()
                                .text_color(cx.theme().secondary_foreground)
                                .text_sm()
                                .hover(|this| {
                                    this.bg(cx.theme().secondary_hover)
                                        .text_color(cx.theme().accent_foreground)
                                        .cursor(CursorStyle::PointingHand)
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, window, cx| {
                                        this.pending_card_id = Some(card_id);
                                        this.show_add_entry_dialog(window, cx);
                                    }),
                                )
                                .font_weight(FontWeight::MEDIUM)
                                .child(IconName::Plus)
                                .child("Add a card"),
                        )
                })
            })
            .child({
                if self.is_adding_list {
                    Input::new(&self.new_list_input)
                        .w_80()
                        .h_10()
                        .rounded_none()
                        .focus_bordered(false)
                        .border_0()
                        .border_b_1()
                        .border_color(theme.foreground)
                        .into_any_element()
                } else {
                    h_flex()
                        .id("add-list-button")
                        .gap_2()
                        .w_80()
                        .p_2()
                        .bg(theme.info.opacity(0.12))
                        .text_color(theme.info)
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .border_1()
                        .border_color(theme.info.opacity(0.24))
                        .rounded(theme.radius)
                        .cursor_pointer()
                        .hover(|this| this.bg(theme.info.opacity(0.18)))
                        .drag_over::<CardDragInfo>(|this, _, _, cx| {
                            this.bg(cx.theme().secondary_hover)
                                .border_color(cx.theme().primary)
                                .text_color(cx.theme().primary)
                        })
                        .on_drop(cx.listener(|this, info: &CardDragInfo, _, cx| {
                            this.move_card_to_end(info, cx);
                        }))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.is_adding_list = true;
                            this.new_list_input.update(cx, |input, cx| {
                                input.focus(window, cx);
                            });
                            cx.notify();
                        }))
                        .child(IconName::Plus)
                        .child("Add another list")
                        .into_any_element()
                }
            })
    }
}
