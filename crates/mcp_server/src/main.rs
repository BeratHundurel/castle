mod paths;
mod server;
mod transport;

use anyhow::Result;
use rmcp::{ServiceExt, transport::stdio};
use storage::{Store, StoreOptions};

use crate::{paths::database_url, server::CastleServer};

#[tokio::main]
async fn main() -> Result<()> {
    let database_url = database_url(std::env::args().skip(1))?;
    paths::prepare_database_file(&database_url)?;

    let store = Store::connect(StoreOptions::new(database_url)).await?;
    let service = CastleServer::new(store).serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
