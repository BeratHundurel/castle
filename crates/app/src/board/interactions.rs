use crate::AppServices;
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

use super::{BoardView, BoardViewEvent, drag::*, dto::*};

mod checklist;
mod create;
mod entry_detail;
mod labels;
mod move_card;
mod move_entry;

fn move_entry_to_list_end_in_memory(
    cards: &mut [BoardListDTO],
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

fn normalize_entry_positions(cards: &mut [BoardListDTO]) -> Vec<(u32, u32, i32)> {
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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use entity::{board, card, entry};
    use gpui::SharedString;
    use migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};
    use storage::board_positions::{BoardLayoutPersistence, BoardLayoutSnapshot};

    use super::{
        BoardCardDTO, BoardLabelDTO, BoardListDTO, ChecklistItemDTO,
        move_entry_to_list_end_in_memory, normalize_entry_positions,
    };
    use crate::board::persistence::load_board_data;

    fn test_entry(id: u32, card_id: u32, position: i32, title: &str) -> BoardCardDTO {
        BoardCardDTO {
            id,
            title: SharedString::from(title),
            description: SharedString::from(format!("{title} description")),
            card_id,
            position,
            due_on: Some(SharedString::from("2026-07-31")),
            reminder_enabled: false,
            labels: vec![],
            checklist_items: vec![],
            attachments: vec![],
            related_notes: vec![],
        }
    }

    fn test_card(id: u32, title: &str, entries: Vec<BoardCardDTO>) -> BoardListDTO {
        BoardListDTO {
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
        let persistence = BoardLayoutPersistence::default();
        let revision = persistence.submit(
            board.id as u32,
            std::sync::Arc::new(db.clone()),
            BoardLayoutSnapshot {
                lists: cards
                    .iter()
                    .enumerate()
                    .map(|(position, card)| (card.id, position as i32))
                    .collect(),
                entries: positions,
            },
        )?;
        persistence
            .wait_for_revision(board.id as u32, revision)
            .await?;

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
