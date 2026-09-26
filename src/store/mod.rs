//! The daemon's data, in one SQLite database (`ferry.db` in the data
//! directory): typed configs that any part of the daemon declares keys for
//! ([`ConfigKey`]), and tables for records that are lists.
//!
//! Calls are synchronous and short, behind one lock: the TLS verifier
//! checks trust from a synchronous callback, and nothing stored is big.
//! The lock is a leaf: nothing else is locked while it's held.

mod config;
mod devices;

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{Connection, TransactionBehavior};
use thiserror::Error;
use tokio::sync::{broadcast, watch};

pub use config::{
    ConfigChange, ConfigKey, ConfigWatch, Entry, Global, IdScope, PerDevice, Scope, Scoped,
};
use config::{EntryId, RawValue};
#[cfg(test)]
pub(crate) use devices::testing;
pub use devices::{TrustedDevice, TrustedIdentity};

use crate::config::create_private_dir;

/// The database's file name in the data directory.
pub const FILE_NAME: &str = "ferry.db";

/// Bumped when the schema changes. Nothing is migrated before the first
/// release: a database of another version is refused.
const SCHEMA_VERSION: i32 = 1;

const SCHEMA: &str = "
CREATE TABLE configs (
  key        TEXT NOT NULL,
  scope      TEXT NOT NULL DEFAULT '',
  id         TEXT NOT NULL DEFAULT '',
  value      TEXT NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (key, scope, id)
) WITHOUT ROWID;

CREATE TABLE devices (
  device_id             TEXT PRIMARY KEY,
  certificate_der       BLOB NOT NULL,
  protocol_version      INTEGER NOT NULL,
  name                  TEXT,
  device_type           TEXT,
  incoming_capabilities TEXT,
  outgoing_capabilities TEXT,
  paired_at             INTEGER NOT NULL,
  updated_at            INTEGER NOT NULL
);
";

/// How many untyped changes [`Store::changes`] buffers for a slow listener.
const CHANGES_CAPACITY: usize = 256;

/// The database, and the watchers of its configs. Cheap to clone.
#[derive(Clone)]
pub struct Store {
    state: Arc<Mutex<State>>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Store").finish_non_exhaustive()
    }
}

/// What the store's one lock guards. Watchers are told of a commit before
/// it is released, so they hear about commits in order; telling them runs
/// none of their code.
struct State {
    connection: Connection,
    watchers: HashMap<EntryId, watch::Sender<RawValue>>,
    changes: broadcast::Sender<ConfigChange>,
}

impl Store {
    /// Open `ferry.db` in `data_dir`, creating the directory (private to
    /// the user) and the database if they don't exist.
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, StoreError> {
        let data_dir = data_dir.as_ref();
        create_private_dir(data_dir).map_err(StoreError::Io)?;
        let connection = Connection::open(data_dir.join(FILE_NAME))?;
        // WAL lets readers and a writer overlap; the timeout makes a CLI
        // daemon and the app on the same data directory wait for each
        // other instead of failing.
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.busy_timeout(Duration::from_secs(5))?;
        Self::with_connection(connection)
    }

    /// A database held in memory, gone when the last clone is dropped: for
    /// tests.
    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::with_connection(Connection::open_in_memory()?)
    }

    fn with_connection(mut connection: Connection) -> Result<Self, StoreError> {
        connection.pragma_update(None, "foreign_keys", true)?;
        let version: i32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => {
                let transaction = connection.transaction()?;
                transaction.execute_batch(SCHEMA)?;
                transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
                transaction.commit()?;
            }
            SCHEMA_VERSION => {}
            other => return Err(StoreError::UnsupportedVersion(other)),
        }
        Ok(Self {
            state: Arc::new(Mutex::new(State {
                connection,
                watchers: HashMap::new(),
                changes: broadcast::Sender::new(CHANGES_CAPACITY),
            })),
        })
    }

    /// Run `body` in one transaction: it commits if `body` returns `Ok`,
    /// and changes nothing otherwise. Watchers hear about what it changed
    /// once it has committed.
    ///
    /// It takes the database's write lock at the start, so another process
    /// on the same data directory waits (up to the busy timeout) rather
    /// than failing midway, after both have read the same state.
    ///
    /// `body` must use the [`Transaction`] it's given, never this `Store`:
    /// the store's lock is held throughout and isn't reentrant, so a call
    /// on the store from `body` deadlocks.
    pub fn transaction<R, E>(
        &self,
        body: impl FnOnce(&mut Transaction<'_>) -> Result<R, E>,
    ) -> Result<R, E>
    where
        E: From<StoreError>,
    {
        let mut state = self.lock();
        let State {
            connection,
            watchers,
            changes,
        } = &mut *state;
        let mut transaction = Transaction {
            inner: connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(StoreError::from)?,
            changed: BTreeMap::new(),
        };
        let result = body(&mut transaction)?;
        let changed = transaction.changed;
        transaction.inner.commit().map_err(StoreError::from)?;
        config::notify(watchers, changes, changed);
        Ok(result)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        // Every write is one transaction, so a panic mid-way leaves
        // nothing half-written behind.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A transaction in progress, from [`Store::transaction`].
pub struct Transaction<'a> {
    inner: rusqlite::Transaction<'a>,
    /// Each config entry written, with its value before the transaction
    /// and now.
    changed: BTreeMap<EntryId, config::Change>,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("the data directory could not be created")]
    Io(#[source] std::io::Error),
    #[error("database operation failed")]
    Database(#[from] rusqlite::Error),
    #[error("a value could not be encoded")]
    Encoding(#[source] serde_json::Error),
    #[error("the stored value of {key} doesn't decode")]
    Undecodable {
        key: &'static str,
        #[source]
        source: serde_json::Error,
    },
    #[error("peer device ID is invalid")]
    InvalidDeviceId,
    #[error("peer certificate is invalid")]
    InvalidCertificate,
    #[error("peer protocol version is unsupported")]
    UnsupportedProtocolVersion,
    #[error("a paired device's record is corrupt")]
    CorruptDevice,
    #[error("the database has schema version {0}, which this build can't read; delete {FILE_NAME}")]
    UnsupportedVersion(i32),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_database_is_created_once_and_reopened() {
        let directory = tempfile::tempdir().unwrap();
        let data_dir = directory.path().join("data");
        drop(Store::open(&data_dir).unwrap());
        assert!(data_dir.join(FILE_NAME).exists());
        Store::open(&data_dir).expect("the same version reopens");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&data_dir).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }

    #[test]
    fn another_schema_version_is_refused() {
        let directory = tempfile::tempdir().unwrap();
        let connection = Connection::open(directory.path().join(FILE_NAME)).unwrap();
        connection.pragma_update(None, "user_version", 7).unwrap();
        drop(connection);
        assert!(matches!(
            Store::open(directory.path()),
            Err(StoreError::UnsupportedVersion(7))
        ));
    }
}
