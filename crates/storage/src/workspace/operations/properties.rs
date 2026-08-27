use crate::workspace::api::{
    BoardPropertiesDetail, BoardPropertyDefinitionDetail, BoardPropertyKindInput,
    BoardPropertyOptionDetail, ClearEntryPropertyInput, CreateBoardPropertyInput,
    CreateBoardPropertyOptionInput, EntryPropertyValueDetail, SetEntryPropertyInput,
};
use anyhow::Result;
use sea_orm::{ConnectionTrait, TransactionTrait};

use crate::{
    store::Store,
    workspace::operations::{
        property_definition_detail, property_option_detail, property_value_detail,
        storage_property_value,
    },
};

impl<C> Store<C>
where
    C: ConnectionTrait + TransactionTrait + Send + Sync + 'static,
{
    pub async fn board_properties(&self, board_id: i64) -> Result<BoardPropertiesDetail> {
        let properties =
            crate::board::properties::load_board_properties(self.db.as_ref(), board_id).await?;
        Ok(BoardPropertiesDetail {
            definitions: properties
                .definitions
                .into_iter()
                .map(property_definition_detail)
                .collect(),
            values: properties
                .values
                .into_iter()
                .map(|value| EntryPropertyValueDetail {
                    entry_id: value.entry_id,
                    property_id: value.property_id,
                    value: property_value_detail(value.value),
                })
                .collect(),
        })
    }

    pub async fn create_board_property(
        &self,
        input: CreateBoardPropertyInput,
    ) -> Result<BoardPropertyDefinitionDetail> {
        let kind = match input.kind {
            BoardPropertyKindInput::Text => crate::board::properties::PropertyKind::Text,
            BoardPropertyKindInput::Number => crate::board::properties::PropertyKind::Number,
            BoardPropertyKindInput::Checkbox => crate::board::properties::PropertyKind::Checkbox,
            BoardPropertyKindInput::Date => crate::board::properties::PropertyKind::Date,
            BoardPropertyKindInput::Select => crate::board::properties::PropertyKind::Select,
            BoardPropertyKindInput::Url => crate::board::properties::PropertyKind::Url,
        };
        crate::board::properties::create_property(
            self.db.as_ref(),
            input.board_id,
            input.name,
            kind,
        )
        .await
        .map(property_definition_detail)
    }

    pub async fn create_board_property_option(
        &self,
        input: CreateBoardPropertyOptionInput,
    ) -> Result<BoardPropertyOptionDetail> {
        crate::board::properties::create_property_option(
            self.db.as_ref(),
            input.property_id,
            input.name,
            input.color,
        )
        .await
        .map(property_option_detail)
    }

    pub async fn set_entry_property(
        &self,
        input: SetEntryPropertyInput,
    ) -> Result<EntryPropertyValueDetail> {
        let value = storage_property_value(input.value);
        crate::board::properties::set_entry_property(
            self.db.as_ref(),
            input.entry_id,
            input.property_id,
            value,
        )
        .await
        .map(|value| EntryPropertyValueDetail {
            entry_id: value.entry_id,
            property_id: value.property_id,
            value: property_value_detail(value.value),
        })
    }

    pub async fn clear_entry_property(&self, input: ClearEntryPropertyInput) -> Result<()> {
        crate::board::properties::clear_entry_property(
            self.db.as_ref(),
            input.entry_id,
            input.property_id,
        )
        .await
    }
}
