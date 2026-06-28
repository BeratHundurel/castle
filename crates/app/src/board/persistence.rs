use entity::{card, card::Entity as Card, entry::Entity as Entry};
use gpui::{Context, SharedString};
use sea_orm::{ColumnTrait, QueryFilter, QueryOrder};

use crate::DB;

use super::{BoardView, dto::*};

impl BoardView {
    pub(crate) fn load_board(&mut self, board_id: u32, cx: &mut Context<Self>) {
        if self.board_id == Some(board_id) {
            return;
        }

        self.board_id = Some(board_id);
        self.cards.clear();
        self.load_error = None;
        self.is_adding_list = false;
        Self::enrich_board_async(cx, board_id);
    }

    pub(super) fn enrich_board_async(cx: &mut Context<Self>, board_id: u32) {
        let db = cx.global::<DB>().conn.clone();

        cx.spawn(async move |this, cx| {
            let result = Card::load()
                .filter(card::Column::BoardId.eq(board_id as i32))
                .order_by_asc(card::Column::Position)
                .order_by_asc(card::Column::Id)
                .with(Entry)
                .all(&*db)
                .await;

            this.update(cx, |this, cx| {
                if this.board_id == Some(board_id) {
                    match result {
                        Ok(result) => {
                            this.cards = result.into_iter().map(CardDTO::from).collect();
                            this.load_error = None;
                        }
                        Err(err) => {
                            let message = format!("Failed to load board {board_id}: {err}");
                            eprintln!("{message}");
                            this.cards.clear();
                            this.load_error = Some(SharedString::from(message));
                        }
                    }
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}
