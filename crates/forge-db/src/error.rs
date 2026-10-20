use forge_core::ForgeError;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// An error reported by the database server or the driver.
    #[error("{0}")]
    Sql(String),
    #[error("unknown data source: {0}")]
    UnknownDataSource(String),
    #[error("unknown console session: {0}")]
    UnknownSession(String),
    #[error("data sources config: {0}")]
    Config(String),
    #[error("system keychain: {0}")]
    Keychain(String),
    #[error("query cancelled")]
    Cancelled,
    /// An operation Forge refuses to perform, e.g. editing a table without a primary key.
    #[error("{0}")]
    Refused(String),
    #[error("{0}")]
    Io(String),
}

impl From<sqlx::Error> for DbError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::Database(db) => match db.code() {
                Some(code) => DbError::Sql(format!("[{code}] {}", db.message())),
                None => DbError::Sql(db.message().to_string()),
            },
            sqlx::Error::PoolTimedOut => {
                DbError::Sql("connection timed out (is the server running?)".into())
            }
            other => DbError::Sql(other.to_string()),
        }
    }
}

impl From<DbError> for ForgeError {
    fn from(e: DbError) -> Self {
        ForgeError::Other(e.to_string())
    }
}

pub type DbResult<T> = std::result::Result<T, DbError>;
