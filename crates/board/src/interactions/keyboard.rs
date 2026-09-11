use gpui_kit::component::WindowExt as _;
use gpui_kit::{Context, Window};

use super::{BoardSelection, BoardView};
use crate::action::{
    AddBoardCardAction, AddBoardListAction, ClearBoardSelectionAction,
    DeleteSelectedBoardItemAction, OpenSelectedBoardItemAction, SelectBoardDownAction,
    SelectBoardLeftAction, SelectBoardRightAction, SelectBoardUpAction,
};

impl BoardView {
    pub(crate) fn on_add_board_card_action(
        &mut self,
        _: &AddBoardCardAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window)
            || window.has_active_dialog(cx)
            || self.entry_editing.dialog.open
            || self.entry_editing.adding_list
        {
            return;
        }

        let Some(list_id) = self
            .selected_list_id()
            .or_else(|| self.data.lists.first().map(|list| list.id))
        else {
            self.start_adding_list(window, cx);
            return;
        };

        self.entry_editing.pending_list_id = Some(list_id);
        self.show_add_entry_dialog(window, cx);
        cx.stop_propagation();
    }

    pub(crate) fn on_add_board_list_action(
        &mut self,
        _: &AddBoardListAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window)
            || window.has_active_dialog(cx)
            || self.entry_editing.dialog.open
        {
            return;
        }

        self.start_adding_list(window, cx);
        cx.stop_propagation();
    }

    pub(crate) fn on_select_board_up_action(
        &mut self,
        _: &SelectBoardUpAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) {
            return;
        }
        self.move_selection_vertically(-1, cx);
        cx.stop_propagation();
    }

    pub(crate) fn on_select_board_down_action(
        &mut self,
        _: &SelectBoardDownAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) {
            return;
        }
        self.move_selection_vertically(1, cx);
        cx.stop_propagation();
    }

    pub(crate) fn on_select_board_left_action(
        &mut self,
        _: &SelectBoardLeftAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) {
            return;
        }
        self.move_selection_horizontally(-1, cx);
        cx.stop_propagation();
    }

    pub(crate) fn on_select_board_right_action(
        &mut self,
        _: &SelectBoardRightAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) {
            return;
        }
        self.move_selection_horizontally(1, cx);
        cx.stop_propagation();
    }

    pub(crate) fn on_open_selected_board_item_action(
        &mut self,
        _: &OpenSelectedBoardItemAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window)
            || window.has_active_dialog(cx)
            || self.entry_editing.dialog.open
        {
            return;
        }

        match self.selection.or_else(|| {
            self.data
                .lists
                .first()
                .map(|list| BoardSelection::List(list.id))
        }) {
            Some(BoardSelection::Entry { entry_id, .. }) => {
                self.open_entry_dialog(entry_id, window, cx);
            }
            Some(BoardSelection::List(list_id)) => {
                self.entry_editing.pending_list_id = Some(list_id);
                self.show_add_entry_dialog(window, cx);
            }
            None => self.start_adding_list(window, cx),
        }
        cx.stop_propagation();
    }

    pub(crate) fn on_clear_board_selection_action(
        &mut self,
        _: &ClearBoardSelectionAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }

        if self.entry_editing.open {
            self.close_entry_dialog(cx);
            cx.stop_propagation();
            return;
        }

        if !self.focus_handle.is_focused(window) {
            return;
        }

        if self.selection.is_some() {
            self.update_selection(None, cx);
            cx.stop_propagation();
        }
    }

    pub(crate) fn on_delete_selected_board_item_action(
        &mut self,
        _: &DeleteSelectedBoardItemAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx)
            || !self.focus_handle.is_focused(window)
            || self.entry_editing.open
            || self.entry_editing.adding_list
            || self.entry_editing.renaming_list_id.is_some()
        {
            return;
        }

        match self.selection {
            Some(BoardSelection::List(list_id)) => {
                self.on_delete_card_action(&crate::action::DeleteCardAction(list_id), window, cx);
            }
            Some(BoardSelection::Entry { entry_id, .. }) => {
                self.confirm_delete_entry(entry_id, window, cx);
            }
            None => {}
        }
        if self.selection.is_some() {
            cx.stop_propagation();
        }
    }

    pub(crate) fn select_list(&mut self, list_id: u32, cx: &mut Context<Self>) {
        if self.data.lists.iter().any(|list| list.id == list_id) {
            self.update_selection(Some(BoardSelection::List(list_id)), cx);
        }
    }

    pub(crate) fn select_entry(&mut self, list_id: u32, entry_id: u32, cx: &mut Context<Self>) {
        if self
            .data
            .lists
            .iter()
            .find(|list| list.id == list_id)
            .is_some_and(|list| list.entries.iter().any(|entry| entry.id == entry_id))
        {
            self.update_selection(Some(BoardSelection::Entry { list_id, entry_id }), cx);
        }
    }

    fn update_selection(&mut self, selection: Option<BoardSelection>, cx: &mut Context<Self>) {
        if self.selection == selection {
            return;
        }

        self.selection = selection;
        cx.notify();
    }

    fn selected_list_id(&self) -> Option<u32> {
        match self.selection {
            Some(BoardSelection::List(list_id)) | Some(BoardSelection::Entry { list_id, .. }) => {
                Some(list_id)
            }
            None => None,
        }
    }

    fn selection_items(&self) -> Vec<BoardSelection> {
        let mut items = Vec::new();
        for list in &self.data.lists {
            items.push(BoardSelection::List(list.id));
            let mut entries = list
                .entries
                .iter()
                .filter(|entry| self.entry_matches_filters(entry))
                .collect::<Vec<_>>();
            if self.properties.active_view_config.sort.is_some() {
                entries.sort_by(|left, right| self.compare_entries_for_active_sort(left, right));
            }
            items.extend(entries.into_iter().map(|entry| BoardSelection::Entry {
                list_id: list.id,
                entry_id: entry.id,
            }));
        }
        items
    }

    fn move_selection_vertically(&mut self, delta: isize, cx: &mut Context<Self>) {
        let items = self.selection_items();
        let Some(next) = next_selection(&items, self.selection, delta) else {
            return;
        };
        self.update_selection(Some(next), cx);
    }

    fn move_selection_horizontally(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(selection) = self.selection else {
            let first_list = self
                .data
                .lists
                .first()
                .map(|list| BoardSelection::List(list.id));
            self.update_selection(first_list, cx);
            return;
        };
        let Some(current_list_index) = self
            .data
            .lists
            .iter()
            .position(|list| Some(list.id) == self.selected_list_id())
        else {
            return;
        };
        let target_list_index = match delta.cmp(&0) {
            std::cmp::Ordering::Less => current_list_index.saturating_sub(1),
            std::cmp::Ordering::Equal => current_list_index,
            std::cmp::Ordering::Greater => current_list_index
                .saturating_add(1)
                .min(self.data.lists.len().saturating_sub(1)),
        };
        if target_list_index == current_list_index {
            return;
        }

        let target_list = &self.data.lists[target_list_index];
        let next_selection = match selection {
            BoardSelection::List(_) => Some(BoardSelection::List(target_list.id)),
            BoardSelection::Entry { entry_id, .. } => {
                let source_index = self.data.lists.get(current_list_index).and_then(|list| {
                    visible_entry_ids(self, list)
                        .iter()
                        .position(|id| *id == entry_id)
                });
                source_index
                    .and_then(|index| visible_entry_ids(self, target_list).get(index).copied())
                    .map(|entry_id| BoardSelection::Entry {
                        list_id: target_list.id,
                        entry_id,
                    })
                    .or(Some(BoardSelection::List(target_list.id)))
            }
        };
        self.update_selection(next_selection, cx);
    }
}

fn visible_entry_ids(board: &BoardView, list: &crate::model::BoardListState) -> Vec<u32> {
    let mut entries = list
        .entries
        .iter()
        .filter(|entry| board.entry_matches_filters(entry))
        .collect::<Vec<_>>();
    if board.properties.active_view_config.sort.is_some() {
        entries.sort_by(|left, right| board.compare_entries_for_active_sort(left, right));
    }
    entries.into_iter().map(|entry| entry.id).collect()
}

fn next_selection(
    items: &[BoardSelection],
    current: Option<BoardSelection>,
    delta: isize,
) -> Option<BoardSelection> {
    if items.is_empty() {
        return None;
    }
    let current_index =
        current.and_then(|selection| items.iter().position(|candidate| *candidate == selection));
    let next_index = match (current_index, delta.cmp(&0)) {
        (None, std::cmp::Ordering::Less) => items.len().saturating_sub(1),
        (None, _) => 0,
        (Some(index), std::cmp::Ordering::Less) => index.saturating_sub(1),
        (Some(index), std::cmp::Ordering::Equal) => index,
        (Some(index), std::cmp::Ordering::Greater) => {
            index.saturating_add(1).min(items.len().saturating_sub(1))
        }
    };
    items.get(next_index).copied()
}

#[cfg(test)]
mod tests {
    use super::{BoardSelection, next_selection};

    #[test]
    fn vertical_selection_starts_at_the_first_or_last_item() {
        let items = [BoardSelection::List(1), BoardSelection::List(2)];
        assert_eq!(
            next_selection(&items, None, 1),
            Some(BoardSelection::List(1))
        );
        assert_eq!(
            next_selection(&items, None, -1),
            Some(BoardSelection::List(2))
        );
    }

    #[test]
    fn vertical_selection_stays_within_the_board_items() {
        let items = [BoardSelection::List(1), BoardSelection::List(2)];
        assert_eq!(
            next_selection(&items, Some(BoardSelection::List(1)), -1),
            Some(BoardSelection::List(1))
        );
        assert_eq!(
            next_selection(&items, Some(BoardSelection::List(2)), 1),
            Some(BoardSelection::List(2))
        );
    }
}
