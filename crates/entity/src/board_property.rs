use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "board_property")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub kind: String,
    pub position: i32,
    pub deleted_at: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
