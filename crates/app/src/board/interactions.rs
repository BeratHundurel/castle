use anyhow::Result;
use entity::{
    board_label, board_label::Entity as BoardLabel, card, card::Entity as Card, entry,
    entry::Entity as Entry, entry_checklist_item,
    entry_checklist_item::Entity as EntryChecklistItem, entry_label,
    entry_label::Entity as EntryLabel,
};
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
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    ExprTrait, QueryFilter, TransactionTrait, sea_query::Expr,
};

use crate::DB;

use super::{BoardView, drag::*, dto::*};

impl BoardView {
    pub(super) fn duplicate_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };
        let Some(source) = self
            .cards
            .iter()
            .flat_map(|list| list.entries.iter())
            .find(|entry| entry.id == entry_id)
            .cloned()
        else {
            return;
        };
        self.duplicate_entry(source, cx);
    }

    fn duplicate_entry(&mut self, source: EntryDTO, cx: &mut Context<Self>) {
        let db = cx.global::<DB>().conn.clone();
        let board_id = self.board_id;
        let runtime = tokio::runtime::Handle::current();
        cx.spawn(async move |this, cx| -> Result<()> {
            runtime
                .spawn(async move {
                    let txn = db.begin().await?;
                    Entry::update_many()
                        .col_expr(
                            entry::Column::Position,
                            Expr::col(entry::Column::Position).add(1),
                        )
                        .filter(entry::Column::CardId.eq(source.card_id as i64))
                        .filter(entry::Column::Position.gte(source.position + 1))
                        .exec(&txn)
                        .await?;
                    let inserted = entry::ActiveModel {
                        title: Set(format!("Copy of {}", source.title)),
                        description: Set(source.description.to_string()),
                        card_id: Set(source.card_id as i64),
                        position: Set(source.position + 1),
                        due_on: Set(source.due_on.map(|value| value.to_string())),
                        ..Default::default()
                    }
                    .insert(&txn)
                    .await?;
                    for label in source.labels {
                        entry_label::ActiveModel {
                            entry_id: Set(inserted.id),
                            board_label_id: Set(label.id as i64),
                            ..Default::default()
                        }
                        .insert(&txn)
                        .await?;
                    }
                    for item in source.checklist_items {
                        entry_checklist_item::ActiveModel {
                            entry_id: Set(inserted.id),
                            title: Set(item.title.to_string()),
                            checked: Set(item.checked),
                            position: Set(item.position),
                            ..Default::default()
                        }
                        .insert(&txn)
                        .await?;
                    }
                    txn.commit().await
                })
                .await??;
            this.update(cx, |this, cx| {
                if let Some(board_id) = board_id {
                    this.enrich_board_async(cx, board_id);
                }
            })
            .ok();
            Ok(())
        })
        .detach();
    }

    pub(super) fn duplicate_card(&mut self, card_id: u32, cx: &mut Context<Self>) {
        let Some(source) = self.cards.iter().find(|card| card.id == card_id).cloned() else {
            return;
        };
        let db = cx.global::<DB>().conn.clone();
        let board_id = self.board_id;
        let runtime = tokio::runtime::Handle::current();
        cx.spawn(async move |this, cx| -> Result<()> {
            runtime
                .spawn(async move {
                    let txn = db.begin().await?;
                    Card::update_many()
                        .col_expr(
                            card::Column::Position,
                            Expr::col(card::Column::Position).add(1),
                        )
                        .filter(card::Column::BoardId.eq(source.board_id as i64))
                        .filter(card::Column::Position.gte(source.position + 1))
                        .exec(&txn)
                        .await?;
                    let inserted_list = card::ActiveModel {
                        title: Set(format!("Copy of {}", source.title)),
                        board_id: Set(source.board_id as i64),
                        position: Set(source.position + 1),
                        ..Default::default()
                    }
                    .insert(&txn)
                    .await?;
                    for entry in source.entries {
                        let inserted = entry::ActiveModel {
                            title: Set(entry.title.to_string()),
                            description: Set(entry.description.to_string()),
                            card_id: Set(inserted_list.id),
                            position: Set(entry.position),
                            due_on: Set(entry.due_on.map(|value| value.to_string())),
                            ..Default::default()
                        }
                        .insert(&txn)
                        .await?;
                        for label in entry.labels {
                            entry_label::ActiveModel {
                                entry_id: Set(inserted.id),
                                board_label_id: Set(label.id as i64),
                                ..Default::default()
                            }
                            .insert(&txn)
                            .await?;
                        }
                        for item in entry.checklist_items {
                            entry_checklist_item::ActiveModel {
                                entry_id: Set(inserted.id),
                                title: Set(item.title.to_string()),
                                checked: Set(item.checked),
                                position: Set(item.position),
                                ..Default::default()
                            }
                            .insert(&txn)
                            .await?;
                        }
                    }
                    txn.commit().await
                })
                .await??;
            this.update(cx, |this, cx| {
                if let Some(board_id) = board_id {
                    this.enrich_board_async(cx, board_id);
                }
            })
            .ok();
            Ok(())
        })
        .detach();
    }
    pub(super) fn entry_values(
        &self,
        entry_id: u32,
    ) -> Option<(SharedString, SharedString, Option<SharedString>)> {
        self.cards
            .iter()
            .flat_map(|card| card.entries.iter())
            .find(|entry| entry.id == entry_id)
            .map(|entry| {
                (
                    entry.title.clone(),
                    entry.description.clone(),
                    entry.due_on.clone(),
                )
            })
    }

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
        let runtime = tokio::runtime::Handle::current();
        let card_id = entry.card_id;

        if let Some(card) = self.cards.iter_mut().find(|card| card.id == entry.card_id) {
            card.entries.push(entry.clone());
            cx.notify();
        }

        cx.spawn(async move |this, cx| -> Result<()> {
            let inserted = runtime
                .spawn(async move {
                    entry::ActiveModel {
                        title: Set(entry.title.to_string()),
                        description: Set(entry.description.to_string()),
                        card_id: Set(entry.card_id as i64),
                        position: Set(entry.position),
                        due_on: Set(None),
                        ..Default::default()
                    }
                    .insert(&*db)
                    .await
                })
                .await??;
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

    pub(super) fn add_card(&mut self, cx: &mut Context<Self>, card: CardDTO, temp_id: u32) {
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        let board_id = card.board_id;

        self.cards.push(card.clone());
        cx.notify();

        cx.spawn(async move |this, cx| -> Result<()> {
            let inserted = runtime
                .spawn(async move {
                    card::ActiveModel {
                        title: Set(card.title.to_string()),
                        board_id: Set(card.board_id as i64),
                        position: Set(card.position),
                        ..Default::default()
                    }
                    .insert(&*db)
                    .await
                })
                .await??;
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

        let _task = tokio::runtime::Handle::current().spawn(async move {
            let model = card::ActiveModel {
                id: Set(card_id as i64),
                title: Set(title),
                ..Default::default()
            };
            model.update(&*db).await?;
            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(super) fn show_add_entry_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let board_view = cx.entity();
        let dialog_title_input = self.dialog_title_input.clone();
        let dialog_description_input = self.dialog_description_input.clone();

        window.open_dialog(cx, move |dialog, _window, _cx| {
            dialog
                .on_ok({
                    let board_view = board_view.clone();
                    move |_, window, cx| {
                        board_view.update(cx, |this, cx| {
                            let Some(card_id) = this.pending_card_id else {
                                return;
                            };

                            let entry_id = this.next_entry_id();
                            let entry = EntryDTO {
                                id: entry_id,
                                title: this.dialog_title_input.read(cx).value(),
                                description: this.dialog_description_input.read(cx).value(),
                                card_id,
                                position: this
                                    .cards
                                    .iter()
                                    .find(|card| card.id == card_id)
                                    .map(|card| card.entries.len() as i32)
                                    .unwrap_or_default(),
                                due_on: None,
                                labels: vec![],
                                checklist_items: vec![],
                            };

                            this.dialog_title_input.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                            });
                            this.dialog_description_input.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                            });

                            this.pending_card_id = None;
                            this.add_entry(cx, entry, entry_id);
                        });

                        true
                    }
                })
                .child(
                    DialogHeader::new()
                        .mb_2()
                        .child(DialogTitle::new().child("Add a card"))
                        .child(
                            DialogDescription::new()
                                .child("Add a title and an optional description."),
                        ),
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
                                .child(Button::new("confirm").primary().label("Add card")),
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

        if info.source_board_id != board_id
            || !self.cards.iter().any(|card| {
                card.id == info.source_card_id
                    && card.entries.iter().any(|entry| entry.id == info.entry_id)
            })
        {
            return;
        }

        self.move_entry_to_list_end(info.entry_id, target_card_id, cx);
    }

    pub(super) fn move_entry_to_list_end(
        &mut self,
        entry_id: u32,
        target_card_id: u32,
        cx: &mut Context<Self>,
    ) {
        if move_entry_to_list_end_in_memory(&mut self.cards, entry_id, target_card_id) {
            self.persist_entry_positions(cx);
        }
    }

    pub(super) fn move_entry_before(
        &mut self,
        info: &DragInfo,
        target_card_id: u32,
        target_entry_id: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(board_id) = self.board_id else {
            return;
        };

        if info.source_board_id != board_id || info.entry_id == target_entry_id {
            return;
        }

        let source_index = self
            .cards
            .iter()
            .find(|card| card.id == info.source_card_id)
            .and_then(|card| {
                card.entries
                    .iter()
                    .position(|entry| entry.id == info.entry_id)
            });

        let target_index = self
            .cards
            .iter()
            .find(|card| card.id == target_card_id)
            .and_then(|card| {
                card.entries
                    .iter()
                    .position(|entry| entry.id == target_entry_id)
            });

        let moving_down_in_same_card = info.source_card_id == target_card_id
            && matches!(
                (source_index, target_index),
                (Some(source_index), Some(target_index)) if source_index < target_index
            );

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
            let Some(mut target_index) = target_card
                .entries
                .iter()
                .position(|entry| entry.id == target_entry_id)
            else {
                return;
            };

            dto.card_id = target_card_id;
            if moving_down_in_same_card {
                target_index = target_index.saturating_add(1);
            }
            target_card.entries.insert(target_index, dto);
            self.persist_entry_positions(cx);
        }
    }

    fn persist_entry_positions(&mut self, cx: &mut Context<Self>) {
        let positions = normalize_entry_positions(&mut self.cards);

        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current()
            .spawn(async move { persist_entry_positions_in_db(db.as_ref(), positions).await });
    }

    pub(super) fn update_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };

        let title = self.entry_title_input.read(cx).value();
        let description = self.entry_description_input.read(cx).value();
        let trimmed_title = title.trim();

        if trimmed_title.is_empty() {
            return;
        }

        let Some(entry) = self
            .cards
            .iter_mut()
            .flat_map(|card| card.entries.iter_mut())
            .find(|entry| entry.id == entry_id)
        else {
            return;
        };

        entry.title = SharedString::from(trimmed_title);
        entry.description = description.clone();
        self.entry_dialog.editing = false;
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let title = trimmed_title.to_string();
        let description = description.to_string();

        let _task = tokio::runtime::Handle::current().spawn(async move {
            let model = entry::ActiveModel {
                id: Set(entry_id as i64),
                title: Set(title),
                description: Set(description),
                ..Default::default()
            };

            model.update(&*db).await?;
            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(super) fn update_selected_entry_due_on(
        &mut self,
        due_on: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };
        let Some(entry) = self
            .cards
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .find(|card| card.id == entry_id)
        else {
            return;
        };

        entry.due_on = due_on.as_deref().map(SharedString::from);
        cx.notify();

        self.next_due_date_update_revision = self.next_due_date_update_revision.saturating_add(1);
        let revision = self.next_due_date_update_revision;
        let persisted_revisions = self.persisted_due_date_revisions.clone();
        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current().spawn(async move {
            let mut persisted_revisions = persisted_revisions.lock().await;
            if persisted_revisions
                .get(&entry_id)
                .is_some_and(|persisted_revision| *persisted_revision >= revision)
            {
                return Ok::<(), sea_orm::DbErr>(());
            }
            entry::ActiveModel {
                id: Set(entry_id as i64),
                due_on: Set(due_on),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            persisted_revisions.insert(entry_id, revision);
            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(super) fn create_checklist_item(&mut self, title: String, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };
        let Some(next_position) = self
            .cards
            .iter()
            .flat_map(|list| list.entries.iter())
            .find(|card| card.id == entry_id)
            .map(|entry| {
                entry
                    .checklist_items
                    .iter()
                    .map(|item| item.position)
                    .max()
                    .unwrap_or(-1)
                    .saturating_add(1)
            })
        else {
            return;
        };

        let position = std::cmp::max(self.next_checklist_item_position, next_position);
        self.next_checklist_item_position = position.saturating_add(1);

        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();
        cx.spawn(async move |this, cx| -> Result<()> {
            let inserted = runtime
                .spawn(async move {
                    entry_checklist_item::ActiveModel {
                        entry_id: Set(entry_id as i64),
                        title: Set(title),
                        checked: Set(false),
                        position: Set(position),
                        ..Default::default()
                    }
                    .insert(&*db)
                    .await
                })
                .await??;
            this.update(cx, |this, cx| {
                if let Some(entry) = this
                    .cards
                    .iter_mut()
                    .flat_map(|list| list.entries.iter_mut())
                    .find(|card| card.id == entry_id)
                {
                    entry.checklist_items.push(ChecklistItemDTO::from(inserted));
                    cx.notify();
                }
            })
            .ok();
            Ok(())
        })
        .detach();
    }

    pub(super) fn set_checklist_item_checked(
        &mut self,
        item_id: u32,
        checked: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self
            .cards
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .flat_map(|card| card.checklist_items.iter_mut())
            .find(|item| item.id == item_id)
        else {
            return;
        };
        item.checked = checked;
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current().spawn(async move {
            entry_checklist_item::ActiveModel {
                id: Set(item_id as i64),
                checked: Set(checked),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(super) fn delete_checklist_item(&mut self, item_id: u32, cx: &mut Context<Self>) {
        for card in self
            .cards
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
        {
            card.checklist_items.retain(|item| item.id != item_id);
        }
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current().spawn(async move {
            EntryChecklistItem::delete_by_id(item_id as i64)
                .exec(&*db)
                .await?;
            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(super) fn move_checklist_item(
        &mut self,
        item_id: u32,
        direction: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(items) = self
            .cards
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .find_map(|card| {
                card.checklist_items
                    .iter()
                    .any(|item| item.id == item_id)
                    .then_some(&mut card.checklist_items)
            })
        else {
            return;
        };
        let Some(index) = items.iter().position(|item| item.id == item_id) else {
            return;
        };
        let Some(target) = index.checked_add_signed(direction) else {
            return;
        };
        if target >= items.len() {
            return;
        }
        items.swap(index, target);
        let positions = items
            .iter_mut()
            .enumerate()
            .map(|(position, item)| {
                item.position = position as i32;
                (item.id, item.position)
            })
            .collect::<Vec<_>>();
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current().spawn(async move {
            for (item_id, position) in positions {
                entry_checklist_item::ActiveModel {
                    id: Set(item_id as i64),
                    position: Set(position),
                    ..Default::default()
                }
                .update(&*db)
                .await?;
            }
            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(super) fn rename_checklist_item(&mut self, title: String, cx: &mut Context<Self>) {
        let Some(item_id) = self.renaming_checklist_item_id else {
            return;
        };
        let Some(item) = self
            .cards
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .flat_map(|card| card.checklist_items.iter_mut())
            .find(|item| item.id == item_id)
        else {
            return;
        };
        item.title = SharedString::from(title.as_str());
        self.renaming_checklist_item_id = None;
        cx.notify();
        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current().spawn(async move {
            entry_checklist_item::ActiveModel {
                id: Set(item_id as i64),
                title: Set(title),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(super) fn create_board_label(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(board_id) = self.board_id else {
            return;
        };

        let color = self.selected_label_color.to_string();
        let db = cx.global::<DB>().conn.clone();
        let runtime = tokio::runtime::Handle::current();

        cx.spawn(async move |this, cx| -> Result<()> {
            let inserted = runtime
                .spawn(async move {
                    board_label::ActiveModel {
                        board_id: Set(board_id as i64),
                        name: Set(name),
                        color: Set(color),
                        ..Default::default()
                    }
                    .insert(&*db)
                    .await
                })
                .await??;

            this.update(cx, |this, cx| {
                if this.board_id == Some(board_id) {
                    this.board_labels.push(BoardLabelDTO::from(inserted));
                    cx.notify();
                }
            })
            .ok();

            Ok(())
        })
        .detach();
    }

    pub(super) fn rename_board_label(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(label_id) = self.renaming_label_id else {
            return;
        };
        let Some(label) = self
            .board_labels
            .iter_mut()
            .find(|label| label.id == label_id)
        else {
            return;
        };

        label.name = SharedString::from(name.as_str());
        self.renaming_label_id = None;
        self.cards
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .for_each(|card| {
                if let Some(label) = card.labels.iter_mut().find(|label| label.id == label_id) {
                    label.name = SharedString::from(name.as_str());
                }
            });
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current().spawn(async move {
            board_label::ActiveModel {
                id: Set(label_id as i64),
                name: Set(name),
                ..Default::default()
            }
            .update(&*db)
            .await?;
            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(super) fn set_entry_label_assignment(
        &mut self,
        entry_id: u32,
        label_id: u32,
        assigned: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(label) = self
            .board_labels
            .iter()
            .find(|label| label.id == label_id)
            .cloned()
        else {
            return;
        };
        let Some(entry) = self
            .cards
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .find(|card| card.id == entry_id)
        else {
            return;
        };

        if assigned {
            if entry
                .labels
                .iter()
                .any(|entry_label| entry_label.id == label_id)
            {
                return;
            }
            entry.labels.push(label);
        } else {
            entry
                .labels
                .retain(|entry_label| entry_label.id != label_id);
        }
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current().spawn(async move {
            if assigned {
                entry_label::ActiveModel {
                    entry_id: Set(entry_id as i64),
                    board_label_id: Set(label_id as i64),
                    ..Default::default()
                }
                .insert(&*db)
                .await?;
            } else {
                EntryLabel::delete_many()
                    .filter(entry_label::Column::EntryId.eq(entry_id as i64))
                    .filter(entry_label::Column::BoardLabelId.eq(label_id as i64))
                    .exec(&*db)
                    .await?;
            }

            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(super) fn delete_board_label(&mut self, label_id: u32, cx: &mut Context<Self>) {
        self.board_labels.retain(|label| label.id != label_id);
        self.filters.label_ids.remove(&label_id);
        self.cards
            .iter_mut()
            .flat_map(|list| list.entries.iter_mut())
            .for_each(|card| card.labels.retain(|label| label.id != label_id));
        self.renaming_label_id = None;
        cx.notify();

        let db = cx.global::<DB>().conn.clone();
        let _task = tokio::runtime::Handle::current().spawn(async move {
            BoardLabel::delete_by_id(label_id as i64).exec(&*db).await?;
            Ok::<(), sea_orm::DbErr>(())
        });
    }

    pub(super) fn delete_selected_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry_id) = self.entry_dialog.entry_id else {
            return;
        };

        for card in &mut self.cards {
            card.entries.retain(|entry| entry.id != entry_id);
        }

        self.is_entry_open = false;
        self.entry_dialog.open = false;
        self.entry_dialog.entry_id = None;
        self.entry_dialog.editing = false;
        cx.notify();

        let db = cx.global::<DB>().conn.clone();

        let _task = tokio::runtime::Handle::current().spawn(async move {
            crate::trash::move_to_trash(
                db.as_ref(),
                crate::trash::MoveToTrash {
                    kind: crate::trash::TrashItemKind::Entry,
                    id: entry_id,
                },
                crate::document_editor::now_ts(),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        });
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
        let _task = tokio::runtime::Handle::current().spawn(async move {
            for (card_id, position) in positions {
                let model = card::ActiveModel {
                    id: Set(card_id as i64),
                    position: Set(position),
                    ..Default::default()
                };
                model.update(&*db).await?;
            }

            Ok::<(), sea_orm::DbErr>(())
        });
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

        let _task = tokio::runtime::Handle::current().spawn(async move {
            crate::trash::move_to_trash(
                db.as_ref(),
                crate::trash::MoveToTrash {
                    kind: crate::trash::TrashItemKind::List,
                    id: card_id,
                },
                crate::document_editor::now_ts(),
            )
            .await?;
            Ok::<(), anyhow::Error>(())
        });
    }
}

fn move_entry_to_list_end_in_memory(
    cards: &mut [CardDTO],
    entry_id: u32,
    target_card_id: u32,
) -> bool {
    let Some(target_index) = cards.iter().position(|card| card.id == target_card_id) else {
        return false;
    };
    let Some((source_index, entry_index)) =
        cards.iter().enumerate().find_map(|(card_index, card)| {
            card.entries
                .iter()
                .position(|entry| entry.id == entry_id)
                .map(|entry_index| (card_index, entry_index))
        })
    else {
        return false;
    };

    if source_index == target_index {
        return false;
    }

    let mut entry = cards[source_index].entries.remove(entry_index);
    entry.card_id = target_card_id;
    cards[target_index].entries.push(entry);
    normalize_entry_positions(cards);
    true
}

fn normalize_entry_positions(cards: &mut [CardDTO]) -> Vec<(u32, u32, i32)> {
    cards
        .iter_mut()
        .flat_map(|card| {
            card.entries
                .iter_mut()
                .enumerate()
                .map(|(index, entry)| {
                    entry.card_id = card.id;
                    entry.position = index as i32;
                    (entry.id, entry.card_id, entry.position)
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

async fn persist_entry_positions_in_db(
    db: &DatabaseConnection,
    positions: Vec<(u32, u32, i32)>,
) -> Result<(), DbErr> {
    let txn = db.begin().await?;
    for (entry_id, card_id, position) in positions {
        entry::ActiveModel {
            id: Set(entry_id as i64),
            card_id: Set(card_id as i64),
            position: Set(position),
            ..Default::default()
        }
        .update(&txn)
        .await?;
    }
    txn.commit().await
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use entity::{board, card, entry};
    use gpui::SharedString;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};

    use super::{
        BoardLabelDTO, CardDTO, ChecklistItemDTO, EntryDTO, move_entry_to_list_end_in_memory,
        normalize_entry_positions, persist_entry_positions_in_db,
    };
    use crate::board::persistence::load_board_data;

    fn test_entry(id: u32, card_id: u32, position: i32, title: &str) -> EntryDTO {
        EntryDTO {
            id,
            title: SharedString::from(title),
            description: SharedString::from(format!("{title} description")),
            card_id,
            position,
            due_on: Some(SharedString::from("2026-07-31")),
            labels: vec![],
            checklist_items: vec![],
        }
    }

    fn test_card(id: u32, title: &str, entries: Vec<EntryDTO>) -> CardDTO {
        CardDTO {
            id,
            title: SharedString::from(title),
            board_id: 1,
            position: id as i32,
            entries,
        }
    }

    #[test]
    fn moves_entry_to_destination_end_and_normalizes_positions() {
        let mut moved_entry = test_entry(10, 1, 4, "First");
        moved_entry.labels.push(BoardLabelDTO {
            id: 40,
            board_id: 1,
            name: SharedString::from("Urgent"),
            color: SharedString::from("red"),
        });
        moved_entry.checklist_items.push(ChecklistItemDTO {
            id: 50,
            entry_id: 10,
            title: SharedString::from("Verify"),
            checked: true,
            position: 0,
        });
        let mut cards = vec![
            test_card(1, "Todo", vec![moved_entry, test_entry(11, 1, 8, "Second")]),
            test_card(2, "Doing", vec![test_entry(20, 2, 5, "Existing")]),
            test_card(3, "Done", vec![test_entry(30, 3, 7, "Unrelated")]),
        ];

        assert!(move_entry_to_list_end_in_memory(&mut cards, 10, 2));
        assert_eq!(
            cards[0]
                .entries
                .iter()
                .map(|entry| (entry.id, entry.card_id, entry.position))
                .collect::<Vec<_>>(),
            vec![(11, 1, 0)]
        );
        assert_eq!(
            cards[1]
                .entries
                .iter()
                .map(|entry| (entry.id, entry.card_id, entry.position))
                .collect::<Vec<_>>(),
            vec![(20, 2, 0), (10, 2, 1)]
        );
        assert_eq!(
            cards[2]
                .entries
                .iter()
                .map(|entry| (entry.id, entry.card_id, entry.position))
                .collect::<Vec<_>>(),
            vec![(30, 3, 0)]
        );
        assert_eq!(cards[1].entries[1].title.as_ref(), "First");
        assert_eq!(
            cards[1].entries[1].description.as_ref(),
            "First description"
        );
        assert_eq!(cards[1].entries[1].due_on.as_deref(), Some("2026-07-31"));
        assert_eq!(cards[1].entries[1].labels[0].name.as_ref(), "Urgent");
        assert_eq!(
            cards[1].entries[1].checklist_items[0].title.as_ref(),
            "Verify"
        );
        assert!(cards[1].entries[1].checklist_items[0].checked);
    }

    #[test]
    fn invalid_and_same_list_moves_leave_board_unchanged() {
        let cards = vec![
            test_card(1, "Todo", vec![test_entry(10, 1, 0, "First")]),
            test_card(2, "Doing", vec![]),
        ];

        for (entry_id, target_card_id) in [(10, 1), (99, 2), (10, 99)] {
            let mut candidate = cards.clone();
            assert!(!move_entry_to_list_end_in_memory(
                &mut candidate,
                entry_id,
                target_card_id,
            ));
            assert_eq!(candidate, cards);
        }
    }

    #[tokio::test]
    async fn moved_entry_positions_persist() -> Result<()> {
        let db = Database::connect("sqlite::memory:").await?;
        Migrator::up(&db, None).await?;

        let board = board::ActiveModel {
            title: Set("Kanban".to_string()),
            project_id: Set(None),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let source = card::ActiveModel {
            title: Set("Todo".to_string()),
            board_id: Set(board.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let target = card::ActiveModel {
            title: Set("Doing".to_string()),
            board_id: Set(board.id),
            position: Set(1),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let moved = entry::ActiveModel {
            title: Set("Move me".to_string()),
            description: Set(String::new()),
            card_id: Set(source.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;
        let existing = entry::ActiveModel {
            title: Set("Already there".to_string()),
            description: Set(String::new()),
            card_id: Set(target.id),
            position: Set(0),
            ..Default::default()
        }
        .insert(&db)
        .await?;

        let mut cards = vec![
            test_card(
                source.id as u32,
                "Todo",
                vec![test_entry(moved.id as u32, source.id as u32, 0, "Move me")],
            ),
            test_card(
                target.id as u32,
                "Doing",
                vec![test_entry(
                    existing.id as u32,
                    target.id as u32,
                    0,
                    "Already there",
                )],
            ),
        ];
        assert!(move_entry_to_list_end_in_memory(
            &mut cards,
            moved.id as u32,
            target.id as u32,
        ));

        let positions = normalize_entry_positions(&mut cards);
        persist_entry_positions_in_db(&db, positions).await?;

        let (reloaded_cards, _) = load_board_data(&db, board.id as u32).await?;
        assert!(reloaded_cards[0].entries.is_empty());
        assert_eq!(
            reloaded_cards[1]
                .entries
                .iter()
                .map(|entry| (entry.id, entry.card_id, entry.position))
                .collect::<Vec<_>>(),
            vec![
                (existing.id as u32, target.id as u32, 0),
                (moved.id as u32, target.id as u32, 1),
            ]
        );

        Ok(())
    }
}
