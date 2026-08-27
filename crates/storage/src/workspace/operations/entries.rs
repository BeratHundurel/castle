use super::*;

impl<C> Store<C>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    pub async fn get_entry(&self, entry_id: i64) -> Result<EntryDetail> {
        let entry = Entry::find_by_id(entry_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active board entry {entry_id} was not found"))?;

        self.entry_detail(entry).await
    }

    pub async fn search_entries(&self, input: SearchEntriesInput) -> Result<Vec<EntryDetail>> {
        let query = input.query.trim();
        if query.is_empty() {
            bail!("query must not be empty");
        }
        if let Some(project_id) = input.project_id {
            self.active_project(project_id).await?;
        }
        if let Some(board_id) = input.board_id {
            self.active_board(board_id).await?;
        }
        let limit = input.limit.unwrap_or(25).clamp(1, 100);
        let projects = self.active_project_map().await?;
        let board_ids = Board::find()
            .filter(board::Column::DeletedAt.is_null())
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .filter(|board| {
                board
                    .project_id
                    .is_none_or(|project_id| projects.contains_key(&project_id))
                    && input
                        .project_id
                        .is_none_or(|project_id| board.project_id == Some(project_id))
                    && input.board_id.is_none_or(|board_id| board.id == board_id)
            })
            .map(|board| board.id)
            .collect::<HashSet<_>>();
        if board_ids.is_empty() {
            return Ok(Vec::new());
        }
        let list_ids = Card::find()
            .filter(card::Column::DeletedAt.is_null())
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .filter(|list| board_ids.contains(&list.board_id))
            .map(|list| list.id)
            .collect::<Vec<_>>();
        if list_ids.is_empty() {
            return Ok(Vec::new());
        }
        let entries = Entry::find()
            .filter(entry::Column::DeletedAt.is_null())
            .filter(entry::Column::CardId.is_in(list_ids))
            .filter(
                Condition::any()
                    .add(entry::Column::Title.contains(query))
                    .add(entry::Column::Description.contains(query)),
            )
            .order_by_asc(entry::Column::Id)
            .limit(limit)
            .all(self.db.as_ref())
            .await?;

        let mut details = Vec::with_capacity(entries.len());
        for entry in entries {
            details.push(self.entry_detail(entry).await?);
        }
        Ok(details)
    }

    pub async fn create_list(&self, input: CreateListInput) -> Result<ListDetail> {
        let title = required_text(input.title, "list title")?;
        self.active_board(input.board_id).await?;
        let position = Card::find()
            .filter(card::Column::BoardId.eq(input.board_id))
            .filter(card::Column::DeletedAt.is_null())
            .count(self.db.as_ref())
            .await? as i32;
        let list = crate::board::commands::create_board_list(
            self,
            crate::board::commands::BoardListDraft {
                title,
                board_id: u32::try_from(input.board_id).context("board ID is out of range")?,
                position,
                cards: Vec::new(),
            },
        )
        .await?;
        Ok(ListDetail {
            id: i64::from(list.id),
            title: list.title,
            position: list.position,
            entries: Vec::new(),
            related_items: Vec::new(),
        })
    }

    pub async fn rename_list(&self, input: RenameListInput) -> Result<ListDetail> {
        let list = self.active_list(input.list_id).await?;
        crate::board::commands::rename_board_list(
            self,
            u32::try_from(list.id).context("list ID is out of range")?,
            required_text(input.title, "list title")?,
        )
        .await?;
        self.get_board(list.board_id)
            .await?
            .lists
            .into_iter()
            .find(|candidate| candidate.id == list.id)
            .with_context(|| format!("renamed list {} was not found", list.id))
    }

    pub async fn create_entry(&self, input: CreateEntryInput) -> Result<EntryDetail> {
        let title = required_text(input.title, "entry title")?;
        validate_due_on(input.due_on.as_deref())?;
        self.active_list(input.list_id).await?;
        let position = Entry::find()
            .filter(entry::Column::CardId.eq(input.list_id))
            .filter(entry::Column::DeletedAt.is_null())
            .count(self.db.as_ref())
            .await? as i32;
        let entry = crate::board::commands::create_board_card(
            self,
            crate::board::commands::BoardCardDraft {
                title,
                description: input.description,
                list_id: u32::try_from(input.list_id).context("list ID is out of range")?,
                position,
                due_on: input.due_on,
                label_ids: Vec::new(),
                checklist_items: Vec::new(),
            },
            now_ts(),
        )
        .await?;
        self.get_entry(i64::from(entry.id)).await
    }

    pub async fn update_entry(&self, input: UpdateEntryInput) -> Result<EntryDetail> {
        if input.clear_due_on && input.due_on.is_some() {
            bail!("due_on and clear_due_on cannot be used together");
        }
        validate_due_on(input.due_on.as_deref())?;
        let entry = Entry::find_by_id(input.entry_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active board entry {} was not found", input.entry_id))?;
        let title = match input.title {
            Some(title) => required_text(title, "entry title")?,
            None => entry.title.clone(),
        };
        let description = input
            .description
            .unwrap_or_else(|| entry.description.clone());
        crate::board::commands::update_board_card(
            self,
            u32::try_from(entry.id).context("entry ID is out of range")?,
            title,
            description,
            now_ts(),
        )
        .await?;
        if input.clear_due_on || input.due_on.is_some() {
            crate::board::commands::set_board_card_due_on(
                self,
                u32::try_from(entry.id).context("entry ID is out of range")?,
                if input.clear_due_on {
                    None
                } else {
                    input.due_on
                },
            )
            .await?;
        }
        self.get_entry(entry.id).await
    }

    pub async fn set_entry_reminder(&self, input: SetEntryReminderInput) -> Result<EntryDetail> {
        let entry = Entry::find_by_id(input.entry_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active board entry {} was not found", input.entry_id))?;
        self.active_list(entry.card_id).await?;
        if input.enabled && entry.due_on.is_none() {
            bail!("a board entry needs a due date before its reminder can be enabled");
        }
        crate::board::commands::set_board_card_reminder(
            self,
            u32::try_from(entry.id).context("entry ID is out of range")?,
            input.enabled,
        )
        .await?;
        self.get_entry(entry.id).await
    }

    pub async fn add_checklist_item(
        &self,
        input: AddChecklistItemInput,
    ) -> Result<ChecklistItemDetail> {
        self.get_entry(input.entry_id).await?;
        let position = EntryChecklistItem::find()
            .filter(entry_checklist_item::Column::EntryId.eq(input.entry_id))
            .count(self.db.as_ref())
            .await? as i32;
        let item = crate::board::commands::create_checklist_item(
            self,
            u32::try_from(input.entry_id).context("entry ID is out of range")?,
            required_text(input.title, "checklist item title")?,
            position,
        )
        .await?;
        Ok(ChecklistItemDetail {
            id: i64::from(item.id),
            title: item.title,
            checked: item.checked,
            position: item.position,
        })
    }

    pub async fn update_checklist_item(
        &self,
        input: UpdateChecklistItemInput,
    ) -> Result<ChecklistItemDetail> {
        if input.title.is_none() && input.checked.is_none() {
            bail!("provide title or checked to update the checklist item");
        }
        let item = EntryChecklistItem::find_by_id(input.item_id)
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("checklist item {} was not found", input.item_id))?;
        self.get_entry(item.entry_id).await?;
        let title = input
            .title
            .map(|title| required_text(title, "checklist item title"))
            .transpose()?;
        crate::board::commands::update_checklist_item(
            self,
            u32::try_from(item.id).context("checklist item ID is out of range")?,
            title,
            input.checked,
        )
        .await?;
        let item = EntryChecklistItem::find_by_id(item.id)
            .one(self.db.as_ref())
            .await?
            .context("updated checklist item was not found")?;
        Ok(ChecklistItemDetail {
            id: item.id,
            title: item.title,
            checked: item.checked,
            position: item.position,
        })
    }

    pub async fn create_board_label(&self, input: CreateBoardLabelInput) -> Result<LabelDetail> {
        self.active_board(input.board_id).await?;
        let label = crate::board::commands::create_label(
            self,
            u32::try_from(input.board_id).context("board ID is out of range")?,
            required_text(input.name, "label name")?,
            required_text(input.color, "label color")?,
        )
        .await?;
        Ok(label_record_detail(label))
    }

    pub async fn set_entry_label(&self, input: SetEntryLabelInput) -> Result<EntryDetail> {
        let entry = self.get_entry(input.entry_id).await?;
        let label = BoardLabel::find_by_id(input.label_id)
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("board label {} was not found", input.label_id))?;
        if label.board_id != entry.board_id {
            bail!(
                "label {} belongs to board {}, but entry {} belongs to board {}",
                label.id,
                label.board_id,
                entry.id,
                entry.board_id
            );
        }
        crate::board::commands::set_label_assignment(
            self,
            u32::try_from(input.entry_id).context("entry ID is out of range")?,
            u32::try_from(input.label_id).context("label ID is out of range")?,
            input.assigned,
        )
        .await?;
        self.get_entry(input.entry_id).await
    }

    pub async fn move_entry(&self, input: MoveEntryInput) -> Result<EntryDetail> {
        self.active_list(input.list_id).await?;
        let entry = Entry::find_by_id(input.entry_id)
            .filter(entry::Column::DeletedAt.is_null())
            .one(self.db.as_ref())
            .await?
            .with_context(|| format!("active board entry {} was not found", input.entry_id))?;
        if entry.card_id == input.list_id {
            return self.entry_detail(entry).await;
        }

        let transaction = self.db.as_ref().begin().await?;
        Entry::update_many()
            .col_expr(
                entry::Column::Position,
                Expr::col(entry::Column::Position).sub(1),
            )
            .filter(entry::Column::CardId.eq(entry.card_id))
            .filter(entry::Column::Position.gt(entry.position))
            .exec(&transaction)
            .await?;
        let position = Entry::find()
            .filter(entry::Column::CardId.eq(input.list_id))
            .filter(entry::Column::DeletedAt.is_null())
            .count(&transaction)
            .await? as i32;
        let moved = entry::ActiveModel {
            id: Set(entry.id),
            card_id: Set(input.list_id),
            position: Set(position),
            ..Default::default()
        }
        .update(&transaction)
        .await?;
        transaction.commit().await?;
        self.entry_detail(moved).await
    }
}
