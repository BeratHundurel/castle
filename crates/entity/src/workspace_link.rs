use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "workspace_link")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub source_note_id: Option<i64>,
    pub source_entry_id: Option<i64>,
    pub target_note_id: Option<i64>,
    pub target_board_id: Option<i64>,
    pub target_card_id: Option<i64>,
    pub target_entry_id: Option<i64>,
    pub target_saved_view_id: Option<i64>,
    pub origin: String,
    pub ordinal: i32,
    pub raw_target: Option<String>,
    pub display_text: Option<String>,
    pub start_byte: Option<i64>,
    pub end_byte: Option<i64>,
    pub line_number: Option<i32>,
    pub created_at: i64,
}

impl ActiveModelBehavior for ActiveModel {}
