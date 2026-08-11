use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "workspace_link_index_state")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub source_kind: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub source_id: i64,
    pub indexed_content: String,
}

impl ActiveModelBehavior for ActiveModel {}
