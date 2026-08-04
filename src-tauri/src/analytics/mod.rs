#[cfg(any(not(feature = "e2e"), test))]
mod ingestion;
mod model;
mod repository;

#[cfg(not(feature = "e2e"))]
pub use ingestion::AgentDataMonitor;
pub use model::AnalyticsSnapshot;

use crate::database::Database;

#[tauri::command]
pub fn get_analytics_snapshot(
    database: tauri::State<'_, Database>,
) -> Result<AnalyticsSnapshot, String> {
    repository::analytics_snapshot(&database).map_err(|error| error.to_string())
}
