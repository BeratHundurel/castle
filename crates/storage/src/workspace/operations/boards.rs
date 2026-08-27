use super::*;

impl<C> Store<C>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    pub async fn list_boards(&self, project_id: Option<i64>) -> Result<Vec<BoardSummary>> {
        if let Some(project_id) = project_id {
            self.active_project(project_id).await?;
        }
        let mut query = Board::find().filter(board::Column::DeletedAt.is_null());
        if let Some(project_id) = project_id {
            query = query.filter(board::Column::ProjectId.eq(project_id));
        }
        let boards = query
            .order_by_asc(board::Column::Id)
            .all(self.db.as_ref())
            .await?;
        let projects = self.active_project_map().await?;

        Ok(boards
            .into_iter()
            .filter(|board| {
                board
                    .project_id
                    .is_none_or(|project_id| projects.contains_key(&project_id))
            })
            .map(|board| BoardSummary {
                id: board.id,
                title: board.title,
                project_id: board.project_id,
                project_name: board
                    .project_id
                    .and_then(|project_id| projects.get(&project_id).cloned()),
            })
            .collect())
    }

    pub async fn get_board(&self, board_id: i64) -> Result<BoardDetail> {
        let board = self.active_board(board_id).await?;
        let project_name = match board.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        let board_id = u32::try_from(board.id)
            .with_context(|| format!("board id {} is outside the supported range", board.id))?;
        let snapshot = crate::board::load_board_snapshot(self.db.as_ref(), board_id).await?;
        let board_item = crate::workspace::links::WorkspaceItemRef {
            kind: crate::workspace::links::WorkspaceItemKind::Board,
            id: board.id,
        };
        let mut relation_items = Vec::with_capacity(snapshot.cards.len() + 1);
        relation_items.push(board_item);
        relation_items.extend(snapshot.cards.iter().map(|list| {
            crate::workspace::links::WorkspaceItemRef {
                kind: crate::workspace::links::WorkspaceItemKind::List,
                id: i64::from(list.id),
            }
        }));
        let mut related_notes = crate::workspace::links::load_related_notes_for_items(
            self.db.as_ref(),
            &relation_items,
        )
        .await?;

        let details = snapshot
            .cards
            .into_iter()
            .map(|list| {
                let list_item = crate::workspace::links::WorkspaceItemRef {
                    kind: crate::workspace::links::WorkspaceItemKind::List,
                    id: i64::from(list.id),
                };
                ListDetail {
                    id: i64::from(list.id),
                    title: list.title.clone(),
                    position: list.position,
                    entries: list
                        .entries
                        .into_iter()
                        .map(|entry| {
                            entry_record_detail(entry, &list.title, &board, project_name.clone())
                        })
                        .collect(),
                    related_items: related_notes
                        .remove(&list_item)
                        .unwrap_or_default()
                        .into_iter()
                        .map(related_note_detail)
                        .collect(),
                }
            })
            .collect();

        Ok(BoardDetail {
            id: board.id,
            title: board.title,
            project_id: board.project_id,
            project_name,
            labels: snapshot
                .labels
                .into_iter()
                .map(label_record_detail)
                .collect(),
            lists: details,
            related_items: related_notes
                .remove(&board_item)
                .unwrap_or_default()
                .into_iter()
                .map(related_note_detail)
                .collect(),
        })
    }

    pub async fn create_board(&self, input: CreateBoardInput) -> Result<BoardSummary> {
        let title = required_text(input.title, "board title")?;
        let project_name = match input.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        let board = crate::workspace::create_board(
            self,
            input
                .project_id
                .map(u32::try_from)
                .transpose()
                .context("project ID is out of range")?,
            title,
        )
        .await?;
        Ok(BoardSummary {
            id: i64::from(board.id),
            title: board.title,
            project_id: input.project_id,
            project_name,
        })
    }

    pub async fn rename_board(&self, input: RenameBoardInput) -> Result<BoardSummary> {
        let board = self.active_board(input.board_id).await?;
        let title = required_text(input.title, "board title")?;
        crate::workspace::persist_workspace_title(
            self,
            crate::workspace::WorkspaceTitleTarget::Board(
                u32::try_from(board.id).context("board ID is out of range")?,
            ),
            title.clone(),
        )
        .await?;
        let project_name = match board.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        Ok(BoardSummary {
            id: board.id,
            title,
            project_id: board.project_id,
            project_name,
        })
    }
}
