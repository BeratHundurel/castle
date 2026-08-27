use std::sync::Arc;

use gpui::{App, Context, WeakEntity};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceNavigationTarget {
    Note {
        note_id: u32,
        source_offset: Option<usize>,
    },
    Board {
        board_id: u32,
        list_id: Option<u32>,
        card_id: Option<u32>,
    },
}

pub type WorkspaceNavigationHandler =
    Arc<dyn Fn(WorkspaceNavigationTarget, &mut App) + Send + Sync>;

pub fn weak_navigation_handler<T: 'static>(
    owner: WeakEntity<T>,
    handle: impl Fn(&mut T, WorkspaceNavigationTarget, &mut Context<T>) + Send + Sync + 'static,
) -> WorkspaceNavigationHandler {
    Arc::new(move |target, cx| {
        let _ = owner.update(cx, |owner, cx| handle(owner, target, cx));
    })
}

impl WorkspaceNavigationTarget {
    pub fn board(board_id: u32) -> Self {
        Self::Board {
            board_id,
            list_id: None,
            card_id: None,
        }
    }

    pub fn list(board_id: u32, list_id: u32) -> Self {
        Self::Board {
            board_id,
            list_id: Some(list_id),
            card_id: None,
        }
    }

    pub fn card(board_id: u32, card_id: u32) -> Self {
        Self::Board {
            board_id,
            list_id: None,
            card_id: Some(card_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext as _;

    #[derive(Default)]
    struct NavigationOwner {
        target: Option<WorkspaceNavigationTarget>,
    }

    #[gpui::test]
    fn weak_handler_routes_targets_without_retaining_its_owner(cx: &mut gpui::TestAppContext) {
        let owner = cx.new(|_| NavigationOwner::default());
        let weak_owner = owner.downgrade();
        let handler = weak_navigation_handler(weak_owner.clone(), |owner, target, _| {
            owner.target = Some(target);
        });
        let target = WorkspaceNavigationTarget::board(42);

        cx.update(|cx| handler(target, cx));
        assert_eq!(owner.read_with(cx, |owner, _| owner.target), Some(target));

        drop(owner);
        assert!(weak_owner.upgrade().is_none());
        cx.update(|cx| handler(WorkspaceNavigationTarget::board(7), cx));
    }
}
