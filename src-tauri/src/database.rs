//! HarnessLens-owned SQLite infrastructure.
//!
//! Connection policy, schema migrations, backups, and restores are exposed
//! through this facade without depending on Tauri or coding-agent providers.

mod backup;
mod connection;
mod error;
mod migrations;

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use serde::Serialize;

#[cfg(test)]
mod tests;

pub use backup::{BackupInfo, RestoreInfo};
pub use connection::Database;
pub use error::DatabaseError;
pub use migrations::current_schema_version;

#[derive(Clone, Debug)]
pub struct DatabaseRuntime {
    state: Arc<DatabaseRuntimeState>,
}

#[derive(Debug)]
struct DatabaseRuntimeState {
    value: Mutex<DatabaseRuntimeValue>,
    ready: Condvar,
}

#[derive(Debug)]
enum DatabaseRuntimeValue {
    Pending,
    Ready(Database),
    Failed(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseRuntimeStatus {
    Pending,
    Ready,
    Failed,
}

impl DatabaseRuntime {
    pub fn pending() -> Self {
        Self {
            state: Arc::new(DatabaseRuntimeState {
                value: Mutex::new(DatabaseRuntimeValue::Pending),
                ready: Condvar::new(),
            }),
        }
    }

    pub fn ready(database: Database) -> Self {
        let runtime = Self::pending();
        runtime.set_result(Ok(database));
        runtime
    }

    pub fn set_result(&self, result: Result<Database, String>) {
        let value = match result {
            Ok(database) => DatabaseRuntimeValue::Ready(database),
            Err(error) => DatabaseRuntimeValue::Failed(error),
        };
        let mut current = match self.state.value.lock() {
            Ok(current) => current,
            Err(poisoned) => poisoned.into_inner(),
        };
        *current = value;
        self.state.ready.notify_all();
    }

    pub fn wait(&self) -> Result<Database, String> {
        let mut value = match self.state.value.lock() {
            Ok(value) => value,
            Err(poisoned) => poisoned.into_inner(),
        };
        loop {
            match &*value {
                DatabaseRuntimeValue::Pending => {
                    value = match self.state.ready.wait(value) {
                        Ok(value) => value,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                }
                DatabaseRuntimeValue::Ready(database) => return Ok(database.clone()),
                DatabaseRuntimeValue::Failed(error) => return Err(error.clone()),
            }
        }
    }

    pub fn wait_timeout(&self, timeout: Duration) -> Result<Option<Database>, String> {
        let mut value = match self.state.value.lock() {
            Ok(value) => value,
            Err(poisoned) => poisoned.into_inner(),
        };
        loop {
            match &*value {
                DatabaseRuntimeValue::Pending => {
                    let (next_value, result) = match self.state.ready.wait_timeout(value, timeout) {
                        Ok(result) => result,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    value = next_value;
                    if result.timed_out() {
                        return Ok(None);
                    }
                }
                DatabaseRuntimeValue::Ready(database) => return Ok(Some(database.clone())),
                DatabaseRuntimeValue::Failed(error) => return Err(error.clone()),
            }
        }
    }

    pub fn status(&self) -> DatabaseRuntimeStatus {
        let value = match self.state.value.lock() {
            Ok(value) => value,
            Err(poisoned) => poisoned.into_inner(),
        };
        match &*value {
            DatabaseRuntimeValue::Pending => DatabaseRuntimeStatus::Pending,
            DatabaseRuntimeValue::Ready(_) => DatabaseRuntimeStatus::Ready,
            DatabaseRuntimeValue::Failed(_) => DatabaseRuntimeStatus::Failed,
        }
    }
}
