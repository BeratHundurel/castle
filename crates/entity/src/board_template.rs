//! `SeaORM` entity for user-created board templates.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "board_template")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub name: String,
    pub description: String,
    pub definition_json: String,
    pub created_at: i64,
}

impl ActiveModelBehavior for ActiveModel {}
