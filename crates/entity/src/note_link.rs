use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "note_link")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub source_note_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub ordinal: i32,
    pub target_note_id: Option<i64>,
    pub raw_target: String,
    pub display_text: Option<String>,
    pub start_byte: i64,
    pub end_byte: i64,
    pub line_number: i32,
}

impl ActiveModelBehavior for ActiveModel {}
