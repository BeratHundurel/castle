use std::collections::HashSet;

use gpui::{AppContext as _, Context, Entity, FocusHandle, SharedString, Window};
use gpui_component::input::InputState;

use crate::AppServices;

use super::{BoardView, BoardViewEvent};

pub(super) struct RelatedNotePickerState {
    pub(super) search_input: Entity<InputState>,
    pub(super) open: bool,
    pub(super) target: Option<storage::workspace_links::WorkspaceItemRef>,
    pub(super) active_row: usize,
    pub(super) pending: HashSet<(storage::workspace_links::WorkspaceItemRef, i64)>,
    return_focus: Option<FocusHandle>,
}

impl RelatedNotePickerState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<BoardView>) -> Self {
        Self {
            search_input: cx.new(|cx| InputState::new(window, cx).placeholder("Search notes")),
            open: false,
            target: None,
            active_row: 0,
            pending: HashSet::new(),
            return_focus: None,
        }
    }

    pub(super) fn set_open(
        &mut self,
        open: bool,
        target: Option<storage::workspace_links::WorkspaceItemRef>,
        window: &mut Window,
        cx: &mut Context<BoardView>,
    ) {
        self.open = open;
        self.target = open.then_some(target).flatten();
        self.active_row = 0;
        if open {
            self.return_focus = window.focused(cx);
            self.search_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
                input.focus(window, cx);
            });
        } else if let Some(return_focus) = self.return_focus.take() {
            return_focus.focus(window, cx);
        }
    }
}

impl BoardView {
    pub(in crate::board) fn related_note_candidates(
        &self,
        item: storage::workspace_links::WorkspaceItemRef,
        cx: &mut Context<Self>,
    ) -> Vec<storage::workspace_links::WorkspaceCatalogEntry> {
        let source_project_id = self
            .workspace_link_catalog
            .iter()
            .find(|entry| entry.item == item)
            .and_then(|entry| entry.project_id);
        let query = self
            .related_note_picker
            .search_input
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let mut candidates = self
            .workspace_link_catalog
            .iter()
            .filter(|entry| entry.item.kind == storage::workspace_links::WorkspaceItemKind::Note)
            .filter(|entry| {
                query.is_empty()
                    || entry.title.to_lowercase().contains(&query)
                    || entry
                        .project_name
                        .as_deref()
                        .is_some_and(|project| project.to_lowercase().contains(&query))
            })
            .cloned()
            .collect::<Vec<_>>();
        candidates.sort_by_key(|entry| {
            (
                entry.project_id != source_project_id,
                entry.title.to_lowercase(),
                entry.item.id,
            )
        });
        candidates.truncate(20);
        candidates
    }

    pub(in crate::board) fn activate_related_note_candidate(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.related_note_picker.target else {
            return;
        };
        let linked = self
            .related_notes_for_item(item)
            .into_iter()
            .map(|note| note.note_id)
            .collect::<HashSet<_>>();
        let candidates = self.related_note_candidates(item, cx);
        if candidates.is_empty() {
            return;
        }
        let index = self
            .related_note_picker
            .active_row
            .min(candidates.len() - 1);
        let note_id = candidates[index].item.id;
        if linked.contains(&note_id) || self.related_note_picker.pending.contains(&(item, note_id))
        {
            return;
        }
        self.link_note_to_item(item, note_id, cx);
    }

    pub(in crate::board) fn move_related_note_candidate(
        &mut self,
        direction: isize,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.related_note_picker.target else {
            return;
        };
        let count = self.related_note_candidates(item, cx).len();
        if count == 0 {
            self.related_note_picker.active_row = 0;
        } else {
            self.related_note_picker.active_row = self
                .related_note_picker
                .active_row
                .saturating_add_signed(direction)
                .min(count - 1);
        }
        cx.notify();
    }

    pub(in crate::board) fn selected_workspace_item(
        &self,
    ) -> Option<storage::workspace_links::WorkspaceItemRef> {
        Some(storage::workspace_links::WorkspaceItemRef {
            kind: storage::workspace_links::WorkspaceItemKind::Card,
            id: i64::from(self.entry_dialog.entry_id?),
        })
    }

    pub(in crate::board) fn related_notes_for_item(
        &self,
        item: storage::workspace_links::WorkspaceItemRef,
    ) -> Vec<storage::workspace_links::RelatedNote> {
        if item.kind == storage::workspace_links::WorkspaceItemKind::Card {
            return self
                .cards
                .iter()
                .flat_map(|list| list.entries.iter())
                .find(|card| i64::from(card.id) == item.id)
                .map(|card| card.related_notes.clone())
                .unwrap_or_default();
        }
        self.related_notes_by_item
            .get(&item)
            .cloned()
            .unwrap_or_default()
    }

    fn related_notes_for_item_mut(
        &mut self,
        item: storage::workspace_links::WorkspaceItemRef,
    ) -> Option<&mut Vec<storage::workspace_links::RelatedNote>> {
        if item.kind == storage::workspace_links::WorkspaceItemKind::Card {
            return self
                .cards
                .iter_mut()
                .flat_map(|list| list.entries.iter_mut())
                .find(|card| i64::from(card.id) == item.id)
                .map(|card| &mut card.related_notes);
        }
        Some(self.related_notes_by_item.entry(item).or_default())
    }

    pub(in crate::board) fn link_note_to_item(
        &mut self,
        item: storage::workspace_links::WorkspaceItemRef,
        note_id: i64,
        cx: &mut Context<Self>,
    ) {
        if !self.related_note_picker.pending.insert((item, note_id)) {
            return;
        }
        let Some(note) = self
            .workspace_link_catalog
            .iter()
            .find(|entry| {
                entry.item.kind == storage::workspace_links::WorkspaceItemKind::Note
                    && entry.item.id == note_id
            })
            .cloned()
        else {
            self.related_note_picker.pending.remove(&(item, note_id));
            return;
        };

        let Some(related_notes) = self.related_notes_for_item_mut(item) else {
            self.related_note_picker.pending.remove(&(item, note_id));
            return;
        };

        if let Some(related) = related_notes
            .iter_mut()
            .find(|related| related.note_id == note_id)
        {
            if !related
                .origins
                .contains(&storage::workspace_links::WorkspaceLinkOrigin::Manual)
            {
                related
                    .origins
                    .push(storage::workspace_links::WorkspaceLinkOrigin::Manual);
            }
        } else {
            related_notes.push(storage::workspace_links::RelatedNote {
                note_id,
                title: note.title,
                project_id: note.project_id,
                project_name: note.project_name,
                origins: vec![storage::workspace_links::WorkspaceLinkOrigin::Manual],
            });
        }

        self.related_note_picker.open = false;
        self.related_note_picker.target = None;
        self.related_note_error = None;
        cx.notify();

        let db = cx.global::<AppServices>().store().connection();
        let board_id = self
            .workspace_link_catalog
            .iter()
            .find(|entry| entry.item == item)
            .and_then(|entry| entry.board_id)
            .and_then(|id| u32::try_from(id).ok());
        let runtime = cx.global::<AppServices>().runtime();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::workspace_links::set_manual_note_link(
                        db.as_ref(),
                        note_id,
                        item,
                        true,
                        crate::document_editor::now_ts(),
                    )
                    .await
                })
                .await;

            this.update(cx, |this, cx| {
                this.related_note_picker.pending.remove(&(item, note_id));
                let error = match result {
                    Ok(Ok(update)) => {
                        if let Some(related_notes) = this.related_notes_for_item_mut(item) {
                            *related_notes = update.related_notes;
                        }
                        if let Some(board_id) = board_id {
                            cx.emit(BoardViewEvent::DataCommitted {
                                board_id,
                                links_changed: true,
                            });
                        }
                        None
                    }
                    Ok(Err(error)) => Some(error.to_string()),
                    Err(error) => Some(error.to_string()),
                };
                if let Some(error) = error {
                    this.related_note_error =
                        Some(SharedString::from(format!("Could not link note: {error}")));
                    if let Some(board_id) = this.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(in crate::board) fn unlink_note_from_item(
        &mut self,
        item: storage::workspace_links::WorkspaceItemRef,
        note_id: i64,
        cx: &mut Context<Self>,
    ) {
        if !self.related_note_picker.pending.insert((item, note_id)) {
            return;
        }
        self.remove_manual_origin(item, note_id);
        self.related_note_error = None;
        cx.notify();

        let db = cx.global::<AppServices>().store().connection();
        let board_id = self
            .workspace_link_catalog
            .iter()
            .find(|entry| entry.item == item)
            .and_then(|entry| entry.board_id)
            .and_then(|id| u32::try_from(id).ok());
        let runtime = cx.global::<AppServices>().runtime();
        cx.spawn(async move |this, cx| {
            let result = runtime
                .spawn(async move {
                    storage::workspace_links::set_manual_note_link(
                        db.as_ref(),
                        note_id,
                        item,
                        false,
                        crate::document_editor::now_ts(),
                    )
                    .await
                })
                .await;
            this.update(cx, |this, cx| {
                this.related_note_picker.pending.remove(&(item, note_id));
                let error = match result {
                    Ok(Ok(update)) => {
                        if let Some(related_notes) = this.related_notes_for_item_mut(item) {
                            *related_notes = update.related_notes;
                        }
                        if let Some(board_id) = board_id {
                            cx.emit(BoardViewEvent::DataCommitted {
                                board_id,
                                links_changed: true,
                            });
                        }
                        None
                    }
                    Ok(Err(error)) => Some(error.to_string()),
                    Err(error) => Some(error.to_string()),
                };
                if let Some(error) = error {
                    this.related_note_error = Some(SharedString::from(format!(
                        "Could not unlink note: {error}"
                    )));
                    if let Some(board_id) = this.board_id {
                        this.enrich_board_async(cx, board_id);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn remove_manual_origin(
        &mut self,
        item: storage::workspace_links::WorkspaceItemRef,
        note_id: i64,
    ) {
        let Some(related_notes) = self.related_notes_for_item_mut(item) else {
            return;
        };
        if let Some(related) = related_notes
            .iter_mut()
            .find(|related| related.note_id == note_id)
        {
            related
                .origins
                .retain(|origin| *origin != storage::workspace_links::WorkspaceLinkOrigin::Manual);
        }
        related_notes.retain(|related| !related.origins.is_empty());
    }

    pub(in crate::board) fn open_related_note(&self, note_id: i64, cx: &mut Context<Self>) {
        if let Ok(note_id) = u32::try_from(note_id) {
            cx.emit(BoardViewEvent::OpenNote(note_id));
        }
    }

    pub(in crate::board) fn create_note_for_item(
        &self,
        item: storage::workspace_links::WorkspaceItemRef,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self
            .workspace_link_catalog
            .iter()
            .find(|entry| entry.item == item)
        else {
            return;
        };
        cx.emit(BoardViewEvent::CreateLinkedNote {
            item,
            project_id: source.project_id.and_then(|id| u32::try_from(id).ok()),
            title: source.title.clone(),
        });
    }
}
