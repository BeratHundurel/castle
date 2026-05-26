use anyhow::Result;
use entity::{card, card::Entity as Card, entry};
use gpui::{Context, ParentElement, SharedString, Styled, Window};
use gpui_component::{
    WindowExt,
    button::{Button, ButtonVariants},
    dialog::{
        DialogAction, DialogClose, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
    },
    input::Input,
    v_flex,
};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use std::rc::Rc;

use crate::DB;

use super::{BoardView, drag::*, dto::*};

impl BoardView {
    pub(super) fn next_card_id(&mut self) -> u32 {
        self.next_temporary_card_id = self.next_temporary_card_id.saturating_add(1);
        u32::MAX.saturating_sub(self.next_temporary_card_id)
    }

    pub(super) fn next_entry_id(&mut self) -> u32 {
        self.next_temporary_entry_id = self.next_temporary_entry_id.saturating_add(1);
        u32::MAX.saturating_sub(self.next_temporary_entry_id)
    }

    pub(super) fn add_entry(&mut self, cx: &mut Context<Self>, entry: EntryDTO, temp_id: u32) {
        let db = cx.global::<DB>().conn.clone();

        if let Some(card) = self.cards.iter_mut().find(|card| card.id == entry.card_id) {
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
                    .find(|card| card.id == entry.card_id)
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

    pub(super) fn add_card(&mut self, cx: &mut Context<Self>, card: CardDTO, temp_id: u32) {
        let db = cx.global::<DB>().conn.clone();

        self.cards.push(card.clone());
        cx.notify();

        cx.spawn(async move |this, cx| -> Result<()> {
            let model = card::ActiveModel {
                title: Set(card.title.to_string()),
                board_id: Set(card.board_id as i64),
                position: Set(card.position),
                ..Default::default()
            };

            let inserted = model.insert(&*db).await?;
            let real_id = inserted.id as u32;

            this.update(cx, |this, _cx| {
                if this.board_id == Some(card.board_id)
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

    pub(super) fn rename_card(&mut self, cx: &mut Context<Self>, new_title: &str) {
        let Some(card_id) = self.renaming_card_id else {
            return;
        };

        let title = new_title.to_string();
        let db = cx.global::<DB>().conn.clone();

        let Some(card) = self.cards.iter_mut().find(|card| card.id == card_id) else {
            return;
        };

        card.title = SharedString::from(new_title);
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

    pub(super) fn show_add_entry_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog_title_input = self.dialog_title_input.clone();
        let dialog_description_input = self.dialog_description_input.clone();

        let confirm_handler = Rc::new(cx.listener(move |this, _, _, cx| {
            let Some(card_id) = this.pending_card_id else {
                return;
            };

            let entry_id = this.next_entry_id();
            let entry = EntryDTO {
                id: entry_id,
                title: this.dialog_title_input.read(cx).value(),
                description: this.dialog_description_input.read(cx).value(),
                card_id,
            };
            this.pending_card_id = None;
            this.add_entry(cx, entry, entry_id);
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

    pub(super) fn move_entry(
        &mut self,
        info: &DragInfo,
        target_card_id: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.board_id else {
            return;
        };

        if info.source_board_id == board_id && info.source_card_id == target_card_id {
            return;
        }

        if !self.cards.iter().any(|card| card.id == target_card_id) {
            return;
        }

        let moving_entry = self
            .cards
            .iter_mut()
            .find(|card| card.id == info.source_card_id)
            .and_then(|card| {
                let index = card
                    .entries
                    .iter()
                    .position(|entry| entry.id == info.entry_id)?;

                Some(card.entries.remove(index))
            });

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

    pub(super) fn persist_card_positions(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn move_card(
        &mut self,
        info: &CardDragInfo,
        target_card_id: u32,
        cx: &mut Context<Self>,
    ) {
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

    pub(super) fn move_card_to_end(&mut self, info: &CardDragInfo, cx: &mut Context<Self>) {
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

    pub(super) fn delete_card(&mut self, cx: &mut Context<Self>, card_id: u32) {
        self.cards.retain(|card| card.id != card_id);
        cx.notify();

        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |_, _| -> Result<()> {
            Card::delete_by_id(card_id as i64).exec(&*db).await?;
            Ok(())
        })
        .detach();
    }
}
