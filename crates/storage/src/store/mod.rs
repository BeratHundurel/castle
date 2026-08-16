use std::{future::Future, pin::Pin, sync::Arc};

use anyhow::Result;
use sea_orm::{
    AccessMode, ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, DbErr,
    ExecResult, IsolationLevel, QueryResult, Statement, TransactionError, TransactionOptions,
    TransactionTrait,
};

use migration::{Migrator, MigratorTrait};

mod mutations;

pub use mutations::{MutationOrigin, Mutations};

#[derive(Clone)]
pub struct Store<C = DatabaseConnection> {
    pub(crate) db: Arc<C>,
}

impl From<&Store> for Store {
    fn from(store: &Store) -> Self {
        store.clone()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl From<DatabaseConnection> for Store {
    fn from(db: DatabaseConnection) -> Self {
        Self { db: Arc::new(db) }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl From<&DatabaseConnection> for Store {
    fn from(db: &DatabaseConnection) -> Self {
        Self {
            db: Arc::new(db.clone()),
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl From<Arc<DatabaseConnection>> for Store {
    fn from(db: Arc<DatabaseConnection>) -> Self {
        Self { db }
    }
}

#[derive(Clone, Debug)]
pub struct StoreOptions {
    database_url: String,
    min_connections: u32,
    max_connections: u32,
}

impl StoreOptions {
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            min_connections: 1,
            max_connections: 4,
        }
    }

    pub fn connection_pool(mut self, min_connections: u32, max_connections: u32) -> Self {
        self.min_connections = min_connections;
        self.max_connections = std::cmp::max(max_connections, min_connections);
        self
    }
}

impl Store {
    pub async fn connect(options: StoreOptions) -> Result<Self> {
        let mut connect_options = ConnectOptions::new(options.database_url);
        connect_options
            .min_connections(options.min_connections)
            .max_connections(options.max_connections);
        let db = Database::connect(connect_options).await?;
        Migrator::up(&db, None).await?;
        Ok(Self { db: Arc::new(db) })
    }

    #[cfg(test)]
    pub(crate) fn new(db: DatabaseConnection) -> Self {
        Self { db: Arc::new(db) }
    }
}

#[async_trait::async_trait]
impl<C> ConnectionTrait for Store<C>
where
    C: ConnectionTrait + Send + Sync,
{
    fn get_database_backend(&self) -> DbBackend {
        self.db.get_database_backend()
    }

    async fn execute_raw(&self, statement: Statement) -> Result<ExecResult, DbErr> {
        self.db.execute_raw(statement).await
    }

    async fn execute_unprepared(&self, sql: &str) -> Result<ExecResult, DbErr> {
        self.db.execute_unprepared(sql).await
    }

    async fn query_one_raw(&self, statement: Statement) -> Result<Option<QueryResult>, DbErr> {
        self.db.query_one_raw(statement).await
    }

    async fn query_all_raw(&self, statement: Statement) -> Result<Vec<QueryResult>, DbErr> {
        self.db.query_all_raw(statement).await
    }

    fn support_returning(&self) -> bool {
        self.db.support_returning()
    }

    fn is_mock_connection(&self) -> bool {
        self.db.is_mock_connection()
    }
}

#[async_trait::async_trait]
impl<C> TransactionTrait for Store<C>
where
    C: TransactionTrait + Send + Sync,
{
    type Transaction = C::Transaction;

    async fn begin(&self) -> Result<Self::Transaction, DbErr> {
        self.db.begin().await
    }

    async fn begin_with_config(
        &self,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> Result<Self::Transaction, DbErr> {
        self.db
            .begin_with_config(isolation_level, access_mode)
            .await
    }

    async fn begin_with_options(
        &self,
        options: TransactionOptions,
    ) -> Result<Self::Transaction, DbErr> {
        self.db.begin_with_options(options).await
    }

    async fn transaction<F, T, E>(&self, callback: F) -> Result<T, TransactionError<E>>
    where
        F: for<'a> FnOnce(
                &'a Self::Transaction,
            ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>
            + Send,
        T: Send,
        E: std::fmt::Display + std::fmt::Debug + Send,
    {
        self.db.transaction(callback).await
    }

    async fn transaction_with_config<F, T, E>(
        &self,
        callback: F,
        isolation_level: Option<IsolationLevel>,
        access_mode: Option<AccessMode>,
    ) -> Result<T, TransactionError<E>>
    where
        F: for<'a> FnOnce(
                &'a Self::Transaction,
            ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>
            + Send,
        T: Send,
        E: std::fmt::Display + std::fmt::Debug + Send,
    {
        self.db
            .transaction_with_config(callback, isolation_level, access_mode)
            .await
    }
}
