use super::*;

impl BoardView {
    pub(in crate::board) fn create_checklist_item(
        &mut self,
        title: String,
        cx: &mut Context<Self>,
    ) {
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

    pub(in crate::board) fn set_checklist_item_checked(
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

    pub(in crate::board) fn delete_checklist_item(&mut self, item_id: u32, cx: &mut Context<Self>) {
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

    pub(in crate::board) fn move_checklist_item(
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

    pub(in crate::board) fn rename_checklist_item(
        &mut self,
        title: String,
        cx: &mut Context<Self>,
    ) {
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
}
