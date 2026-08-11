mod paths;
mod server;
mod store;
mod types;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use storage::StoreOptions;

use crate::{paths::database_url, server::CastleServer, store::CastleStore};

#[tokio::main]
async fn main() -> Result<()> {
    let database_url = database_url(std::env::args().skip(1))?;
    paths::prepare_database_file(&database_url)?;

    let store = CastleStore::connect(StoreOptions::new(database_url)).await?;
    let service = CastleServer::new(store).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
