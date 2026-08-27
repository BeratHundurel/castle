use super::*;

impl<C> Store<C>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    pub async fn list_notes(
        &self,
        project_id: Option<i64>,
        limit: Option<u64>,
    ) -> Result<Vec<NoteSummary>> {
        if let Some(project_id) = project_id {
            self.active_project(project_id).await?;
        }

        let projects = self.active_project_map().await?;
        let mut query = Note::find().filter(note::Column::DeletedAt.is_null());

        if let Some(project_id) = project_id {
            query = query.filter(note::Column::ProjectId.eq(project_id));
        }

        let notes = query
            .order_by_desc(note::Column::IsPinned)
            .order_by_desc(note::Column::UpdatedAt)
            .order_by_asc(note::Column::Id)
            .limit(limit.unwrap_or(50).clamp(1, 100))
            .all(self.db.as_ref())
            .await?;

        Ok(notes
            .into_iter()
            .filter(|note| {
                note.project_id
                    .is_none_or(|project_id| projects.contains_key(&project_id))
            })
            .map(|note| note_summary(note, &projects))
            .collect())
    }

    pub async fn get_note(&self, note_id: i64) -> Result<NoteDetail> {
        let note = self.active_note(note_id).await?;
        self.note_detail(note).await
    }

    pub async fn get_note_links(&self, note_id: i64) -> Result<NoteLinksDetail> {
        let links = crate::note::links::load_note_links(self.db.as_ref(), note_id).await?;
        Ok(NoteLinksDetail {
            inbound: links.inbound.into_iter().map(note_link_detail).collect(),
            outbound: links.outbound.into_iter().map(note_link_detail).collect(),
            unresolved: links
                .unresolved
                .into_iter()
                .map(unresolved_link_detail)
                .collect(),
        })
    }

    pub async fn search_notes(&self, input: SearchNotesInput) -> Result<Vec<NoteSummary>> {
        let query_text = input.query.trim();
        if query_text.is_empty() {
            bail!("query must not be empty");
        }
        if let Some(project_id) = input.project_id {
            self.active_project(project_id).await?;
        }
        let projects = self.active_project_map().await?;
        let mut query = Note::find()
            .filter(note::Column::DeletedAt.is_null())
            .filter(
                Condition::any()
                    .add(note::Column::Title.contains(query_text))
                    .add(note::Column::CachedContent.contains(query_text)),
            );

        if let Some(project_id) = input.project_id {
            query = query.filter(note::Column::ProjectId.eq(project_id));
        }

        let notes = query
            .order_by_desc(note::Column::UpdatedAt)
            .limit(input.limit.unwrap_or(25).clamp(1, 100))
            .all(self.db.as_ref())
            .await?;

        Ok(notes
            .into_iter()
            .filter(|note| {
                note.project_id
                    .is_none_or(|project_id| projects.contains_key(&project_id))
            })
            .map(|note| note_summary(note, &projects))
            .collect())
    }

    pub async fn create_note(&self, input: CreateNoteInput) -> Result<NoteDetail> {
        let title = required_text(input.title, "note title")?;
        let project_name = match input.project_id {
            Some(project_id) => Some(self.active_project(project_id).await?.name),
            None => None,
        };
        let now = now_ts();
        let txn = self.db.as_ref().begin().await?;
        let note = note::ActiveModel {
            title: Set(title),
            project_id: Set(input.project_id),
            file_path: Set(None),
            file_managed_by_app: Set(false),
            cached_content: Set(input.content.clone()),
            file_missing_since: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&txn)
        .await?;
        crate::note::links::index_note_links_in_connection(
            &txn,
            note.id,
            &input.content,
            note.updated_at,
        )
        .await?;
        txn.commit().await?;
        let related_items = self.related_items_for_note(note.id).await?;
        Ok(NoteDetail {
            id: note.id,
            title: note.title,
            content: input.content,
            project_id: note.project_id,
            project_name,
            file_path: note.file_path,
            file_managed_by_app: note.file_managed_by_app,
            file_missing: false,
            is_pinned: note.is_pinned,
            created_at: note.created_at,
            updated_at: note.updated_at,
            related_items,
        })
    }

    pub async fn update_note(&self, input: UpdateNoteInput) -> Result<NoteDetail> {
        if input.title.is_none() && input.content.is_none() && input.is_pinned.is_none() {
            bail!("provide title, content, or is_pinned to update the note");
        }
        let mut note = self.active_note(input.note_id).await?;
        if let Some(expected) = input.expected_updated_at
            && expected != note.updated_at
        {
            bail!(
                "note {} changed since it was read; expected updated_at {}, current value is {}",
                note.id,
                expected,
                note.updated_at
            );
        }

        if let Some(content) = input.content.as_ref()
            && let Some(file_path) = note.file_path.as_ref()
        {
            let path = PathBuf::from(file_path);
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::write(path, content).await?;
        }

        let new_title = input
            .title
            .map(|title| required_text(title, "note title"))
            .transpose()?;
        if let Some(new_title) = new_title.as_deref().filter(|title| *title != note.title) {
            let note_id = u32::try_from(note.id)
                .with_context(|| format!("note id {} cannot be renamed", note.id))?;
            crate::workspace::persist_workspace_title(
                self.db.as_ref(),
                crate::workspace::WorkspaceTitleTarget::Note(note_id),
                new_title.to_string(),
            )
            .await?;
            note = self.active_note(input.note_id).await?;
        }
        let content_for_index = input.content.clone();
        let current_updated_at = note.updated_at;
        let mut active: note::ActiveModel = note.into();
        if let Some(content) = input.content {
            active.cached_content = Set(content);
            active.file_missing_since = Set(None);
        }
        if let Some(is_pinned) = input.is_pinned {
            active.is_pinned = Set(is_pinned);
        }
        active.updated_at = Set(next_updated_at(current_updated_at));
        let txn = self.db.as_ref().begin().await?;
        let note = active.update(&txn).await?;
        if let Some(content) = content_for_index {
            crate::note::links::index_note_links_in_connection(
                &txn,
                note.id,
                &content,
                note.updated_at,
            )
            .await?;
        }
        txn.commit().await?;
        self.note_detail(note).await
    }

    pub async fn move_note(&self, input: MoveNoteInput) -> Result<NoteDetail> {
        if let Some(project_id) = input.project_id {
            self.active_project(project_id).await?;
        }
        let note = self.active_note(input.note_id).await?;
        let note = note::ActiveModel {
            id: Set(note.id),
            project_id: Set(input.project_id),
            updated_at: Set(next_updated_at(note.updated_at)),
            ..Default::default()
        }
        .update(self.db.as_ref())
        .await?;
        self.note_detail(note).await
    }
}
