use super::*;
use entity::{board, card, entry};
use migration::{Migrator, MigratorTrait};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, Database};

#[tokio::test]
async fn typed_properties_are_board_scoped_and_bulk_loaded() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let (board, entry) = board_with_entry(&db, "Collection").await?;
    let (other_board, other_entry) = board_with_entry(&db, "Other").await?;
    let rating = create_property(&db, board.id, "Rating".into(), PropertyKind::Number).await?;
    set_entry_property(&db, entry.id, rating.id, PropertyValue::Number(4.5)).await?;
    assert!(
        set_entry_property(&db, other_entry.id, rating.id, PropertyValue::Number(3.0))
            .await
            .is_err()
    );
    assert!(
        create_property(&db, other_board.id, "Rating".into(), PropertyKind::Number)
            .await
            .is_ok()
    );
    let loaded = load_board_properties(&db, board.id).await?;
    assert_eq!(loaded.definitions.len(), 1);
    assert_eq!(loaded.values[0].value, PropertyValue::Number(4.5));
    Ok(())
}

#[tokio::test]
async fn saved_view_rejects_foreign_property_references() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let (board, _) = board_with_entry(&db, "First").await?;
    let (other, _) = board_with_entry(&db, "Second").await?;
    let foreign = create_property(&db, other.id, "Stage".into(), PropertyKind::Select).await?;
    let config = BoardViewConfig {
        visible_properties: vec![PropertyKey::Custom(foreign.id)],
        ..Default::default()
    };
    assert!(
        create_board_view(&db, board.id, "Invalid".into(), config)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn deleting_options_and_properties_cleans_values_and_views() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let (board, entry) = board_with_entry(&db, "Collection").await?;
    let stage = create_property(&db, board.id, "Stage".into(), PropertyKind::Select).await?;
    let draft = create_property_option(&db, stage.id, "Draft".into(), "blue".into()).await?;
    set_entry_property(&db, entry.id, stage.id, PropertyValue::Select(draft.id)).await?;
    let config = BoardViewConfig {
        filters: vec![ViewFilter {
            property: PropertyKey::Custom(stage.id),
            operator: FilterOperator::IsAnyOf,
            operand: Some(FilterOperand::OptionIds(vec![draft.id])),
        }],
        sort: Some(ViewSort {
            property: PropertyKey::Custom(stage.id),
            direction: SortDirection::Ascending,
        }),
        visible_properties: vec![PropertyKey::Custom(stage.id)],
        compact_cards: false,
    };
    create_board_view(&db, board.id, "Drafts".into(), config).await?;

    assert_eq!(
        property_deletion_impact(&db, stage.id).await?,
        DeletionImpact {
            value_count: 1,
            view_count: 1,
        }
    );
    delete_property_option(&db, draft.id).await?;
    let loaded = load_board_properties(&db, board.id).await?;
    assert!(loaded.values.is_empty());
    assert!(loaded.definitions[0].options.is_empty());
    let views = load_board_views(&db, board.id).await?;
    assert!(views.views[0].config.filters.is_empty());

    delete_property(&db, stage.id).await?;
    let views = load_board_views(&db, board.id).await?;
    assert!(views.views[0].config.visible_properties.is_empty());
    assert!(views.views[0].config.sort.is_none());
    assert!(
        load_board_properties(&db, board.id)
            .await?
            .definitions
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn saved_view_crud_preserves_one_default() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let (board, _) = board_with_entry(&db, "Views").await?;
    let first = create_board_view(&db, board.id, "First".into(), Default::default()).await?;
    let second = create_board_view(&db, board.id, "Second".into(), Default::default()).await?;

    set_default_board_view(&db, first.id).await?;
    set_default_board_view(&db, second.id).await?;
    let views = load_board_views(&db, board.id).await?;
    assert_eq!(views.views.iter().filter(|view| view.is_default).count(), 1);
    assert!(
        views
            .views
            .iter()
            .find(|view| view.id == second.id)
            .is_some_and(|view| view.is_default)
    );

    let renamed = rename_board_view(&db, second.id, "Current".into()).await?;
    assert_eq!(renamed.name, "Current");
    let updated = update_board_view(
        &db,
        second.id,
        BoardViewConfig {
            compact_cards: true,
            ..Default::default()
        },
    )
    .await?;
    assert!(updated.config.compact_cards);
    delete_board_view(&db, second.id).await?;
    let views = load_board_views(&db, board.id).await?;
    assert_eq!(views.views.len(), 1);
    assert!(views.views[0].is_default);
    Ok(())
}

#[tokio::test]
async fn selected_board_view_is_restored_independently_from_default() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let (board, _) = board_with_entry(&db, "Views").await?;
    let first = create_board_view(&db, board.id, "First".into(), Default::default()).await?;
    let second = create_board_view(&db, board.id, "Second".into(), Default::default()).await?;

    set_default_board_view(&db, first.id).await?;
    set_selected_board_view(&db, board.id, Some(second.id)).await?;

    let views = load_board_views(&db, board.id).await?;
    assert_eq!(views.selected_view_id, Some(second.id));
    assert!(
        views
            .views
            .iter()
            .find(|view| view.id == first.id)
            .is_some_and(|view| view.is_default)
    );

    set_selected_board_view(&db, board.id, None).await?;
    assert_eq!(
        load_board_views(&db, board.id).await?.selected_view_id,
        None
    );

    set_selected_board_view(&db, board.id, Some(second.id)).await?;
    delete_board_view(&db, second.id).await?;
    assert_eq!(
        load_board_views(&db, board.id).await?.selected_view_id,
        None
    );

    let (other_board, _) = board_with_entry(&db, "Other").await?;
    let other_view =
        create_board_view(&db, other_board.id, "Other".into(), Default::default()).await?;
    assert!(
        set_selected_board_view(&db, board.id, Some(other_view.id))
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn version_one_views_are_migrated_without_blocking_board_load() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let (board, _) = board_with_entry(&db, "Legacy").await?;
    let rating = create_property(&db, board.id, "Rating".into(), PropertyKind::Number).await?;
    saved_board_view::ActiveModel {
        board_id: Set(board.id),
        name: Set("Legacy view".into()),
        position: Set(0),
        is_default: Set(true),
        config_version: Set(1),
        config_json: Set(serde_json::json!({
            "filters": [{
                "property": { "kind": "custom", "id": rating.id },
                "operator": "greater_than",
                "value": { "kind": "number", "value": 3.0 }
            }],
            "sort": null,
            "visible_properties": [{ "kind": "custom", "id": rating.id }],
            "compact_cards": false
        })
        .to_string()),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(&db)
    .await?;

    let views = load_board_views(&db, board.id).await?;
    assert!(views.warnings.is_empty());
    assert_eq!(
        views.views[0].config.filters[0].operand,
        Some(FilterOperand::Number(3.0))
    );
    Ok(())
}

#[tokio::test]
async fn malformed_property_values_are_reported_as_warnings() -> Result<()> {
    let db = Database::connect("sqlite::memory:").await?;
    Migrator::up(&db, None).await?;
    let (board, entry) = board_with_entry(&db, "Warnings").await?;
    let rating = create_property(&db, board.id, "Rating".into(), PropertyKind::Number).await?;
    entry_property_value::ActiveModel {
        entry_id: Set(entry.id),
        property_id: Set(rating.id),
        text_value: Set(None),
        number_value: Set(None),
        boolean_value: Set(None),
        date_value: Set(None),
        option_id: Set(None),
    }
    .insert(&db)
    .await?;

    let loaded = load_board_properties(&db, board.id).await?;
    assert!(loaded.values.is_empty());
    assert_eq!(loaded.warnings.len(), 1);
    Ok(())
}

async fn board_with_entry(
    db: &(impl ConnectionTrait + TransactionTrait),
    title: &str,
) -> Result<(board::Model, entry::Model)> {
    let board = board::ActiveModel {
        title: Set(title.to_string()),
        project_id: Set(None),
        ..Default::default()
    }
    .insert(db)
    .await?;
    let list = card::ActiveModel {
        title: Set("Any list".to_string()),
        board_id: Set(board.id),
        position: Set(0),
        ..Default::default()
    }
    .insert(db)
    .await?;
    let entry = entry::ActiveModel {
        title: Set("An entry".to_string()),
        description: Set(String::new()),
        card_id: Set(list.id),
        position: Set(0),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok((board, entry))
}
