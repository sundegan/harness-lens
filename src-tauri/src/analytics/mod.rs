#[cfg(any(not(feature = "e2e"), test))]
mod ingestion;
mod model;
mod repository;

#[cfg(not(feature = "e2e"))]
pub use ingestion::AgentDataMonitor;
pub use model::{SessionDetail, SessionPage, SessionPageRequest, SkillAnalysis};

use crate::database::Database;

fn join_error(error: impl std::fmt::Display) -> String {
    format!("analytics query task failed: {error}")
}

#[tauri::command]
pub async fn get_skill_analysis(
    database: tauri::State<'_, Database>,
) -> Result<SkillAnalysis, String> {
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || repository::skill_analysis(&database))
        .await
        .map_err(join_error)?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_session_page(
    database: tauri::State<'_, Database>,
    request: SessionPageRequest,
) -> Result<SessionPage, String> {
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || repository::session_page(&database, request))
        .await
        .map_err(join_error)?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_session_detail(
    database: tauri::State<'_, Database>,
    session_id: String,
) -> Result<Option<SessionDetail>, String> {
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || repository::session_detail(&database, &session_id))
        .await
        .map_err(join_error)?
        .map_err(|error| error.to_string())
}
