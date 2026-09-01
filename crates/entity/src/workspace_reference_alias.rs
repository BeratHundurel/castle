//! SeaORM entity for the historical names used by readable workspace references.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "workspace_reference_alias")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub alias: String,
    pub normalized_alias: String,
    pub project_id: Option<i64>,
    pub board_id: Option<i64>,
    pub list_id: Option<i64>,
    pub card_id: Option<i64>,
    pub saved_view_id: Option<i64>,
    pub created_at: i64,
}

impl ActiveModelBehavior for ActiveModel {}
