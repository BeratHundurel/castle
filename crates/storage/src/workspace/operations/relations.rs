use super::*;

impl<C> Store<C>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    pub async fn list_workspace_relations(
        &self,
        input: WorkspaceRelationsInput,
    ) -> Result<Vec<RelatedItemDetail>> {
        match (input.note_id, input.kind, input.item_id) {
            (Some(note_id), None, None) => {
                self.active_note(note_id).await?;
                self.related_items_for_note(note_id).await
            }
            (None, Some(kind), Some(item_id)) => {
                let relation = NoteWorkspaceRelationInput {
                    note_id: 0,
                    kind,
                    item_id,
                    board_id: input.board_id,
                    list_id: input.list_id,
                };
                let item = self.validate_relation_target(&relation).await?;
                self.related_items_for_workspace_item(item).await
            }
            _ => {
                bail!("provide either note_id, or kind and item_id with the required hierarchy IDs")
            }
        }
    }

    pub async fn link_note_to_workspace_item(
        &self,
        input: NoteWorkspaceRelationInput,
    ) -> Result<Vec<RelatedItemDetail>> {
        self.active_note(input.note_id).await?;
        let item = self.validate_relation_target(&input).await?;
        Ok(crate::workspace::links::set_manual_note_link(
            self.db.as_ref(),
            input.note_id,
            item,
            true,
            now_ts(),
        )
        .await?
        .related_notes
        .into_iter()
        .map(related_note_detail)
        .collect())
    }

    pub async fn unlink_note_from_workspace_item(
        &self,
        input: NoteWorkspaceRelationInput,
    ) -> Result<Vec<RelatedItemDetail>> {
        self.active_note(input.note_id).await?;
        let item = self.validate_relation_target(&input).await?;
        Ok(crate::workspace::links::set_manual_note_link(
            self.db.as_ref(),
            input.note_id,
            item,
            false,
            now_ts(),
        )
        .await?
        .related_notes
        .into_iter()
        .map(related_note_detail)
        .collect())
    }

    pub(super) async fn validate_relation_target(
        &self,
        input: &NoteWorkspaceRelationInput,
    ) -> Result<crate::workspace::links::WorkspaceItemRef> {
        let kind = match input.kind {
            WorkspaceItemKindInput::Board => crate::workspace::links::WorkspaceItemKind::Board,
            WorkspaceItemKindInput::List => crate::workspace::links::WorkspaceItemKind::List,
            WorkspaceItemKindInput::Card => crate::workspace::links::WorkspaceItemKind::Card,
        };
        let catalog =
            crate::workspace::links::load_workspace_link_catalog(self.db.as_ref()).await?;
        let target = catalog
            .iter()
            .find(|entry| entry.item.kind == kind && entry.item.id == input.item_id)
            .with_context(|| format!("active {} {} was not found", kind.as_str(), input.item_id))?;
        match kind {
            crate::workspace::links::WorkspaceItemKind::Board => {
                if input
                    .board_id
                    .is_some_and(|board_id| board_id != input.item_id)
                    || input.list_id.is_some()
                {
                    bail!(
                        "board target hierarchy does not match item_id {}",
                        input.item_id
                    );
                }
            }
            crate::workspace::links::WorkspaceItemKind::List => {
                let board_id = input
                    .board_id
                    .context("board_id is required for a list target")?;
                if target.board_id != Some(board_id) || input.list_id.is_some() {
                    bail!(
                        "list {} does not belong to board {}",
                        input.item_id,
                        board_id
                    );
                }
            }
            crate::workspace::links::WorkspaceItemKind::Card => {
                let board_id = input
                    .board_id
                    .context("board_id is required for a card target")?;
                let list_id = input
                    .list_id
                    .context("list_id is required for a card target")?;
                if target.board_id != Some(board_id) || target.list_id != Some(list_id) {
                    bail!(
                        "card {} does not belong to board {} and list {}",
                        input.item_id,
                        board_id,
                        list_id
                    );
                }
            }
            crate::workspace::links::WorkspaceItemKind::Note => {
                bail!("note targets are not manual workspace relationships")
            }
        }
        Ok(target.item)
    }

    pub(super) async fn related_items_for_note(
        &self,
        note_id: i64,
    ) -> Result<Vec<RelatedItemDetail>> {
        let links =
            crate::workspace::links::load_note_workspace_links(self.db.as_ref(), note_id).await?;
        let mut grouped = HashMap::<
            crate::workspace::links::WorkspaceItemRef,
            (crate::workspace::links::WorkspaceCatalogEntry, Vec<String>),
        >::new();
        for reference in links.references {
            let origin = workspace_origin_label(reference.origin);
            let row = grouped
                .entry(reference.item.item)
                .or_insert_with(|| (reference.item.clone(), Vec::new()));
            if !row.1.iter().any(|existing| existing == origin) {
                row.1.push(origin.to_string());
            }
        }
        let mut details = grouped
            .into_values()
            .map(|(entry, origins)| related_item_detail(entry, origins))
            .collect::<Vec<_>>();
        details.sort_by_key(|detail| (detail.kind.clone(), detail.breadcrumb.to_lowercase()));
        Ok(details)
    }

    pub(super) async fn related_items_for_workspace_item(
        &self,
        item: crate::workspace::links::WorkspaceItemRef,
    ) -> Result<Vec<RelatedItemDetail>> {
        let related = crate::workspace::links::load_related_notes(self.db.as_ref(), item).await?;
        let catalog =
            crate::workspace::links::load_workspace_link_catalog(self.db.as_ref()).await?;
        Ok(related
            .into_iter()
            .filter_map(|note| {
                let entry = catalog.iter().find(|entry| {
                    entry.item.kind == crate::workspace::links::WorkspaceItemKind::Note
                        && entry.item.id == note.note_id
                })?;
                Some(related_item_detail(
                    entry.clone(),
                    note.origins
                        .into_iter()
                        .map(workspace_origin_label)
                        .map(str::to_string)
                        .collect(),
                ))
            })
            .collect())
    }
}
