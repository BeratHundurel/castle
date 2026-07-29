use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "note_alias")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    pub note_id: i64,
    pub alias: String,
    pub normalized_alias: String,
    pub created_at: i64,
    #[sea_orm(
        belongs_to,
        from = "note_id",
        to = "id",
        on_update = "NoAction",
        on_delete = "Cascade"
    )]
    pub note: HasOne<super::note::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
