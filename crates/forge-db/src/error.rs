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
    /// The SSH host of a tunnel presented an unknown or changed key.
    #[error("SSH tunnel: the authenticity of host {} cannot be established ({} key fingerprint is {})", .0.host, .0.key_type, .0.fingerprint)]
    SshHostKey(Box<forge_remote::session::HostKeyInfo>),
    /// A production data source: the user must confirm the script first.
    #[error("{0}")]
    NeedsConfirmation(String),
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

impl From<tiberius::error::Error> for DbError {
    fn from(e: tiberius::error::Error) -> Self {
        match e {
            tiberius::error::Error::Server(t) => {
                DbError::Sql(format!("[{}] {}", t.code(), t.message()))
            }
            other => DbError::Sql(other.to_string()),
        }
    }
}

impl From<redis::RedisError> for DbError {
    fn from(e: redis::RedisError) -> Self {
        DbError::Sql(e.to_string())
    }
}

impl From<mongodb::error::Error> for DbError {
    fn from(e: mongodb::error::Error) -> Self {
        DbError::Sql(e.to_string())
    }
}

impl From<reqwest::Error> for DbError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_connect() || e.is_timeout() {
            DbError::Sql(format!("cannot connect (is the server running?): {e}"))
        } else {
            DbError::Sql(e.to_string())
        }
    }
}

impl From<DbError> for ForgeError {
    fn from(e: DbError) -> Self {
        ForgeError::Other(e.to_string())
    }
}

pub type DbResult<T> = std::result::Result<T, DbError>;
