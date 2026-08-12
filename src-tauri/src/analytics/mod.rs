#[cfg(any(not(feature = "e2e"), test))]
mod ingestion;
mod model;
mod repository;

#[cfg(not(feature = "e2e"))]
pub use ingestion::AgentDataMonitor;
pub use model::{AnalyticsSnapshot, SessionDetail, SessionPage, SessionPageRequest};

use crate::database::Database;

#[tauri::command]
pub fn get_analytics_snapshot(
    database: tauri::State<'_, Database>,
) -> Result<AnalyticsSnapshot, String> {
    repository::analytics_snapshot(&database).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_session_page(
    database: tauri::State<'_, Database>,
    request: SessionPageRequest,
) -> Result<SessionPage, String> {
    repository::session_page(&database, request).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_session_detail(
    database: tauri::State<'_, Database>,
    session_id: String,
) -> Result<Option<SessionDetail>, String> {
    repository::session_detail(&database, &session_id).map_err(|error| error.to_string())
}
