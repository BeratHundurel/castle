use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "board_property_option")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    pub property_id: i64,
    pub name: String,
    pub color: String,
    pub position: i32,
    pub deleted_at: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
