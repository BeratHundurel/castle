use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "entry_property_value")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub entry_id: i64,
    #[sea_orm(primary_key, auto_increment = false)]
    pub property_id: i64,
    pub text_value: Option<String>,
    pub number_value: Option<f64>,
    pub boolean_value: Option<bool>,
    pub date_value: Option<String>,
    pub option_id: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}
