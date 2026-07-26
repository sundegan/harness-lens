//! HarnessLens-owned SQLite infrastructure.
//!
//! Connection policy, schema migrations, backups, and restores are exposed
//! through this facade without depending on Tauri or coding-agent providers.

mod backup;
mod connection;
mod error;
mod migrations;

#[cfg(test)]
mod tests;

pub use backup::{BackupInfo, RestoreInfo};
pub use connection::Database;
pub use error::DatabaseError;
pub use migrations::current_schema_version;
