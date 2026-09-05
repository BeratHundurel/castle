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
        let entries = Entry::find()
            .filter(entry::Column::DeletedAt.is_null())
            .filter(
                entry::Column::CardId
                    .in_subquery(active_search_card_ids(input.project_id, input.board_id)),
            )
            .filter(
                Condition::any()
                    .add(entry::Column::Title.contains(query))
                    .add(entry::Column::Description.contains(query)),
            )
            .order_by_asc(entry::Column::Id)
            .limit(limit)
            .all(self.db.as_ref())
            .await?;

        self.search_entry_details(entries, &projects).await
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

    async fn search_entry_details(
        &self,
        entries: Vec<entry::Model>,
        projects: &HashMap<i64, String>,
    ) -> Result<Vec<EntryDetail>> {
        if entries.is_empty() {
            return Ok(Vec::new());
        }
        let entry_ids = entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
        let lists = Card::find()
            .filter(
                card::Column::Id.is_in(
                    entries
                        .iter()
                        .map(|entry| entry.card_id)
                        .collect::<Vec<_>>(),
                ),
            )
            .filter(card::Column::DeletedAt.is_null())
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .map(|list| (list.id, list))
            .collect::<HashMap<_, _>>();
        let boards = Board::find()
            .filter(
                board::Column::Id
                    .is_in(lists.values().map(|list| list.board_id).collect::<Vec<_>>()),
            )
            .filter(board::Column::DeletedAt.is_null())
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .map(|board| (board.id, board))
            .collect::<HashMap<_, _>>();

        let mut checklist_by_entry = HashMap::<i64, Vec<ChecklistItemDetail>>::new();
        for item in EntryChecklistItem::find()
            .filter(entry_checklist_item::Column::EntryId.is_in(entry_ids.clone()))
            .order_by_asc(entry_checklist_item::Column::Position)
            .order_by_asc(entry_checklist_item::Column::Id)
            .all(self.db.as_ref())
            .await?
        {
            checklist_by_entry
                .entry(item.entry_id)
                .or_default()
                .push(ChecklistItemDetail {
                    id: item.id,
                    title: item.title,
                    checked: item.checked,
                    position: item.position,
                });
        }

        let associations = EntryLabel::find()
            .filter(entry_label::Column::EntryId.is_in(entry_ids.clone()))
            .all(self.db.as_ref())
            .await?;
        let labels_by_id = if associations.is_empty() {
            HashMap::new()
        } else {
            BoardLabel::find()
                .filter(
                    board_label::Column::Id.is_in(
                        associations
                            .iter()
                            .map(|association| association.board_label_id)
                            .collect::<HashSet<_>>()
                            .into_iter()
                            .collect::<Vec<_>>(),
                    ),
                )
                .order_by_asc(board_label::Column::Id)
                .all(self.db.as_ref())
                .await?
                .into_iter()
                .map(|label| (label.id, label))
                .collect::<HashMap<_, _>>()
        };
        let mut label_ids_by_entry = HashMap::<i64, Vec<i64>>::new();
        for association in associations {
            label_ids_by_entry
                .entry(association.entry_id)
                .or_default()
                .push(association.board_label_id);
        }

        let mut attachments_by_entry = HashMap::<i64, Vec<AttachmentDetail>>::new();
        for attachment in EntryAttachment::find()
            .filter(entry_attachment::Column::EntryId.is_in(entry_ids.clone()))
            .order_by_asc(entry_attachment::Column::Id)
            .all(self.db.as_ref())
            .await?
        {
            attachments_by_entry
                .entry(attachment.entry_id)
                .or_default()
                .push(AttachmentDetail {
                    id: attachment.id,
                    file_name: attachment.file_name,
                });
        }

        let mut related_by_entry = self
            .search_related_notes_by_entry(&entry_ids, projects)
            .await?;

        let mut details = Vec::with_capacity(entries.len());
        for entry in entries {
            let list = lists
                .get(&entry.card_id)
                .with_context(|| format!("active list {} was not found", entry.card_id))?;
            let board = boards
                .get(&list.board_id)
                .with_context(|| format!("active board {} was not found", list.board_id))?;
            let project_name = match board.project_id {
                Some(project_id) => Some(
                    projects
                        .get(&project_id)
                        .cloned()
                        .with_context(|| format!("active project {project_id} was not found"))?,
                ),
                None => None,
            };
            let mut label_ids = label_ids_by_entry.remove(&entry.id).unwrap_or_default();
            label_ids.sort_unstable();
            let labels = label_ids
                .into_iter()
                .filter_map(|label_id| labels_by_id.get(&label_id))
                .map(|label| label_detail(label.clone()))
                .collect();
            let mut related = related_by_entry.remove(&entry.id).unwrap_or_default();
            related.sort_by(|left, right| {
                (
                    left.project_id != board.project_id,
                    left.title.to_lowercase(),
                    left.note_id,
                )
                    .cmp(&(
                        right.project_id != board.project_id,
                        right.title.to_lowercase(),
                        right.note_id,
                    ))
            });
            let related_items = related.into_iter().map(SearchRelatedNote::detail).collect();
            details.push(EntryDetail {
                id: entry.id,
                title: entry.title,
                description: entry.description,
                due_on: entry.due_on,
                reminder_enabled: entry.reminder_enabled,
                position: entry.position,
                list_id: list.id,
                list_title: list.title.clone(),
                board_id: board.id,
                board_title: board.title.clone(),
                project_id: board.project_id,
                project_name,
                labels,
                checklist_items: checklist_by_entry.remove(&entry.id).unwrap_or_default(),
                attachments: attachments_by_entry.remove(&entry.id).unwrap_or_default(),
                related_items,
            });
        }
        Ok(details)
    }

    async fn search_related_notes_by_entry(
        &self,
        entry_ids: &[i64],
        projects: &HashMap<i64, String>,
    ) -> Result<HashMap<i64, Vec<SearchRelatedNote>>> {
        let mut grouped = HashMap::<i64, Vec<SearchRelatedNote>>::new();
        if entry_ids.is_empty() {
            return Ok(grouped);
        }
        let requested = entry_ids.iter().copied().collect::<HashSet<_>>();
        let links = WorkspaceLink::find()
            .filter(
                Condition::any()
                    .add(workspace_link::Column::TargetEntryId.is_in(entry_ids.to_vec()))
                    .add(
                        Condition::all()
                            .add(workspace_link::Column::SourceEntryId.is_in(entry_ids.to_vec()))
                            .add(workspace_link::Column::TargetNoteId.is_not_null()),
                    ),
            )
            .all(self.db.as_ref())
            .await?;
        let mut origins_by_item_note =
            HashMap::<(i64, i64), Vec<crate::workspace::links::WorkspaceLinkOrigin>>::new();
        let mut note_ids = HashSet::new();
        for link in &links {
            let item_id = match (link.target_entry_id, link.source_entry_id) {
                (Some(item_id), _) if requested.contains(&item_id) => item_id,
                (_, Some(item_id))
                    if link.target_note_id.is_some() && requested.contains(&item_id) =>
                {
                    item_id
                }
                _ => continue,
            };
            let Some(note_id) = link.source_note_id.or(link.target_note_id) else {
                continue;
            };
            note_ids.insert(note_id);
            let origins = origins_by_item_note.entry((item_id, note_id)).or_default();
            let origin = search_link_origin(&link.origin);
            if !origins.contains(&origin) {
                origins.push(origin);
            }
        }
        if note_ids.is_empty() {
            return Ok(grouped);
        }
        let notes_by_id = Note::find()
            .filter(note::Column::Id.is_in(note_ids.into_iter().collect::<Vec<_>>()))
            .filter(note::Column::DeletedAt.is_null())
            .all(self.db.as_ref())
            .await?
            .into_iter()
            .map(|note| (note.id, note))
            .collect::<HashMap<_, _>>();
        for ((item_id, note_id), origins) in origins_by_item_note {
            let Some(note) = notes_by_id.get(&note_id) else {
                continue;
            };
            if note
                .project_id
                .is_some_and(|project_id| !projects.contains_key(&project_id))
            {
                continue;
            }
            grouped.entry(item_id).or_default().push(SearchRelatedNote {
                note_id,
                title: note.title.clone(),
                project_id: note.project_id,
                project_name: note
                    .project_id
                    .and_then(|project_id| projects.get(&project_id).cloned()),
                origins,
            });
        }
        Ok(grouped)
    }
}

fn active_search_project_ids() -> SelectStatement {
    Query::select()
        .column(project::Column::Id)
        .from(Project)
        .and_where(project::Column::Archived.eq(false))
        .and_where(project::Column::DeletedAt.is_null())
        .to_owned()
}

fn active_search_board_ids(project_id: Option<i64>, board_id: Option<i64>) -> SelectStatement {
    let mut boards = Query::select().to_owned();
    boards
        .column(board::Column::Id)
        .from(Board)
        .and_where(board::Column::DeletedAt.is_null())
        .cond_where(
            Condition::any()
                .add(board::Column::ProjectId.is_null())
                .add(board::Column::ProjectId.in_subquery(active_search_project_ids())),
        );
    if let Some(project_id) = project_id {
        boards.and_where(board::Column::ProjectId.eq(project_id));
    }
    if let Some(board_id) = board_id {
        boards.and_where(board::Column::Id.eq(board_id));
    }
    boards
}

fn active_search_card_ids(project_id: Option<i64>, board_id: Option<i64>) -> SelectStatement {
    Query::select()
        .column(card::Column::Id)
        .from(Card)
        .and_where(card::Column::DeletedAt.is_null())
        .and_where(card::Column::BoardId.in_subquery(active_search_board_ids(project_id, board_id)))
        .to_owned()
}

struct SearchRelatedNote {
    note_id: i64,
    title: String,
    project_id: Option<i64>,
    project_name: Option<String>,
    origins: Vec<crate::workspace::links::WorkspaceLinkOrigin>,
}

impl SearchRelatedNote {
    fn detail(self) -> RelatedItemDetail {
        let breadcrumb = match &self.project_name {
            Some(project) => format!("{project} / {}", self.title),
            None => self.title.clone(),
        };
        let mut segments = Vec::with_capacity(2);
        if let Some(project) = &self.project_name {
            segments.push(crate::workspace::links::escape_segment(project));
        }
        segments.push(crate::workspace::links::escape_segment(&self.title));
        RelatedItemDetail {
            kind: crate::workspace::links::WorkspaceItemKind::Note
                .as_str()
                .to_string(),
            id: self.note_id,
            title: self.title,
            breadcrumb,
            stable_link: format!("[[note:{}]]", segments.join(" / ")),
            origins: self
                .origins
                .into_iter()
                .map(workspace_origin_label)
                .map(str::to_string)
                .collect(),
        }
    }
}

fn search_link_origin(origin: &str) -> crate::workspace::links::WorkspaceLinkOrigin {
    match origin {
        "manual" => crate::workspace::links::WorkspaceLinkOrigin::Manual,
        "embed" => crate::workspace::links::WorkspaceLinkOrigin::Embed,
        _ => crate::workspace::links::WorkspaceLinkOrigin::Wikilink,
    }
}
