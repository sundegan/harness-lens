#[cfg(any(not(feature = "e2e"), test))]
mod ingestion;
mod model;
mod repository;
mod tool_call_repository;

#[cfg(not(feature = "e2e"))]
pub use ingestion::AgentDataMonitor;
pub use model::{
    SessionDetail, SessionPage, SessionPageRequest, SkillAnalysis, SyncStatus, ToolCallAnalysis,
    ToolCallAnalysisRequest, ToolCallDetail, ToolCallFilterOptions, ToolCallFilterOptionsRequest,
    ToolCallPage, ToolCallPageRequest,
};

use crate::database::{Database, DatabaseError, DatabaseRuntime};

fn join_error(error: impl std::fmt::Display) -> String {
    format!("analytics query task failed: {error}")
}

async fn run_database_query<T, F>(database: DatabaseRuntime, query: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&Database) -> Result<T, DatabaseError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let database = database.wait()?;
        query(&database).map_err(|error| error.to_string())
    })
    .await
    .map_err(join_error)?
}

#[tauri::command]
pub async fn get_sync_status(
    database: tauri::State<'_, DatabaseRuntime>,
) -> Result<Vec<SyncStatus>, String> {
    run_database_query(database.inner().clone(), repository::sync_status).await
}

#[tauri::command]
pub async fn get_tool_call_analysis(
    database: tauri::State<'_, DatabaseRuntime>,
    request: ToolCallAnalysisRequest,
) -> Result<ToolCallAnalysis, String> {
    run_database_query(database.inner().clone(), move |database| {
        tool_call_repository::analysis(database, request)
    })
    .await
}

#[tauri::command]
pub async fn get_tool_call_filter_options(
    database: tauri::State<'_, DatabaseRuntime>,
    request: ToolCallFilterOptionsRequest,
) -> Result<ToolCallFilterOptions, String> {
    run_database_query(database.inner().clone(), move |database| {
        tool_call_repository::filter_options(database, request)
    })
    .await
}

#[tauri::command]
pub async fn get_tool_call_page(
    database: tauri::State<'_, DatabaseRuntime>,
    request: ToolCallPageRequest,
) -> Result<ToolCallPage, String> {
    run_database_query(database.inner().clone(), move |database| {
        tool_call_repository::page(database, request)
    })
    .await
}

#[tauri::command]
pub async fn get_tool_call_detail(
    database: tauri::State<'_, DatabaseRuntime>,
    tool_call_id: String,
) -> Result<Option<ToolCallDetail>, String> {
    run_database_query(database.inner().clone(), move |database| {
        tool_call_repository::detail(database, &tool_call_id)
    })
    .await
}

#[tauri::command]
pub async fn get_skill_analysis(
    database: tauri::State<'_, DatabaseRuntime>,
) -> Result<SkillAnalysis, String> {
    run_database_query(database.inner().clone(), repository::skill_analysis).await
}

#[tauri::command]
pub async fn get_session_page(
    database: tauri::State<'_, DatabaseRuntime>,
    request: SessionPageRequest,
) -> Result<SessionPage, String> {
    run_database_query(database.inner().clone(), move |database| {
        repository::session_page(database, request)
    })
    .await
}

#[tauri::command]
pub async fn get_session_detail(
    database: tauri::State<'_, DatabaseRuntime>,
    session_id: String,
) -> Result<Option<SessionDetail>, String> {
    run_database_query(database.inner().clone(), move |database| {
        repository::session_detail(database, &session_id)
    })
    .await
}
