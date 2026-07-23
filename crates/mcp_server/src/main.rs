mod paths;
mod server;
mod store;
mod types;

use anyhow::Result;
use migration::{Migrator, MigratorTrait};
use rmcp::{ServiceExt, transport::stdio};
use sea_orm::{ConnectOptions, Database};

use crate::{paths::database_url, server::CastleServer, store::CastleStore};

#[tokio::main]
async fn main() -> Result<()> {
    let database_url = database_url(std::env::args().skip(1))?;
    paths::prepare_database_file(&database_url)?;

    let mut options = ConnectOptions::new(database_url);
    options.max_connections(4).min_connections(1);
    let db = Database::connect(options).await?;
    Migrator::up(&db, None).await?;

    let service = CastleServer::new(CastleStore::new(db))
        .serve(stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}
