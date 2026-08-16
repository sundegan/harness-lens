use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsSnapshot {
    pub summary: OverallSummary,
    pub sessions: Vec<SessionSummary>,
    pub skills: Vec<SkillSummary>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillAnalysis {
    pub skills: Vec<SkillSummary>,
}

#[cfg(test)]
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverallSummary {
    pub session_count: i64,
    pub total_tokens: i64,
    pub skill_invocation_count: i64,
    pub succeeded_invocation_count: i64,
    pub failed_invocation_count: i64,
    pub cancelled_invocation_count: i64,
    pub active_invocation_count: i64,
    pub unknown_invocation_count: i64,
}

#[cfg(test)]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub project_name: String,
    pub created_at_ms: Option<i64>,
    pub updated_at_ms: Option<i64>,
    pub observed_duration_ms: i64,
    pub wall_duration_ms: Option<i64>,
    pub tokens_used: i64,
    pub invocation_count: i64,
    pub skill_invocation_count: i64,
    pub archived: bool,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    pub name: String,
    pub invocation_count: i64,
    pub succeeded_count: i64,
    pub failed_count: i64,
    pub cancelled_count: i64,
    pub unknown_count: i64,
    pub success_rate: Option<f64>,
    pub average_duration_ms: Option<f64>,
    pub max_duration_ms: Option<i64>,
    pub average_tokens: Option<f64>,
    pub max_tokens: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub provider: String,
    pub source_id: String,
    pub status: String,
    pub phase: String,
    pub total_files: i64,
    pub processed_files: i64,
    pub processed_lines: i64,
    pub estimated_total_lines: Option<i64>,
    pub current_file: Option<String>,
    pub current_line: i64,
    pub estimated_remaining_ms: Option<i64>,
    pub last_error: Option<String>,
    pub updated_at_ms: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPageRequest {
    pub page: u32,
    pub page_size: u32,
    pub query: Option<String>,
    pub archived: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPage {
    pub items: Vec<SessionListItem>,
    pub page: u32,
    pub page_size: u32,
    pub total: i64,
    pub generated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListItem {
    pub id: String,
    pub source_session_id: String,
    pub provider: String,
    pub title: String,
    pub project_name: String,
    pub cwd: Option<String>,
    pub created_at_ms: Option<i64>,
    pub updated_at_ms: Option<i64>,
    pub tokens_used: i64,
    pub invocation_count: i64,
    pub skill_invocation_count: i64,
    pub event_count: i64,
    pub archived: bool,
    pub status: String,
    pub model: Option<String>,
    pub model_provider: Option<String>,
    pub agent_version: Option<String>,
    pub git_branch: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session: SessionListItem,
    pub agent_name: Option<String>,
    pub agent_role: Option<String>,
    pub git_commit: Option<String>,
    pub git_remote_url: Option<String>,
    pub data_quality: String,
    pub events: Vec<SessionEventItem>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEventItem {
    pub id: String,
    pub invocation_id: Option<String>,
    pub timestamp_ms: Option<i64>,
    pub event_type: String,
    pub event: Value,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallFilters {
    pub start_at_ms: Option<i64>,
    pub end_at_ms: Option<i64>,
    #[serde(default)]
    pub providers: Vec<String>,
    #[serde(default)]
    pub project_keys: Vec<String>,
    #[serde(default)]
    pub tool_names: Vec<String>,
    #[serde(default)]
    pub mcp_servers: Vec<String>,
    pub timezone: Option<String>,
    pub bucket: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallAnalysisRequest {
    #[serde(flatten)]
    pub filters: ToolCallFilters,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallFilterOptionsRequest {
    #[serde(flatten)]
    pub filters: ToolCallFilters,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricRate {
    pub numerator: i64,
    pub denominator: i64,
    pub rate: Option<f64>,
    pub unknown_count: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallSummary {
    pub call_count: i64,
    pub tool_count: i64,
    pub session_count: i64,
    pub project_count: i64,
    pub average_duration_ms: Option<f64>,
    pub success_rate: MetricRate,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallTrendPoint {
    pub bucket_start_ms: i64,
    pub label: String,
    pub call_count: i64,
    pub failed_count: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallComparison {
    pub key: String,
    pub label: String,
    pub call_count: i64,
    pub session_count: i64,
    pub project_count: i64,
    pub failed_count: i64,
    pub declined_count: i64,
    pub cancelled_count: i64,
    pub average_duration_ms: Option<f64>,
    pub exact_repeat_count: i64,
    pub success_rate: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallAnalysis {
    pub summary: ToolCallSummary,
    pub time_trend: Vec<ToolCallTrendPoint>,
    pub status_distribution: Vec<ToolCallComparison>,
    pub tool_ranking: Vec<ToolCallComparison>,
    pub provider_comparison: Vec<ToolCallComparison>,
    pub project_comparison: Vec<ToolCallComparison>,
    pub mcp_server_comparison: Vec<ToolCallComparison>,
    pub generated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallFilterOption {
    pub value: String,
    pub label: String,
    pub count: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallFilterOptions {
    pub providers: Vec<ToolCallFilterOption>,
    pub projects: Vec<ToolCallFilterOption>,
    pub tool_names: Vec<ToolCallFilterOption>,
    pub mcp_servers: Vec<ToolCallFilterOption>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallPageRequest {
    #[serde(flatten)]
    pub filters: ToolCallFilters,
    pub page: u32,
    pub page_size: u32,
    pub query: Option<String>,
    pub sort_by: Option<String>,
    pub sort_direction: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallListItem {
    pub id: String,
    pub provider: String,
    pub session_id: String,
    pub source_session_id: String,
    pub session_title: String,
    pub project_name: String,
    pub agent_version: Option<String>,
    pub tool_name: String,
    pub mcp_server: Option<String>,
    pub tool_kind: String,
    pub started_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub duration_ms: Option<i64>,
    pub status: String,
    pub has_result: bool,
    pub call_event_id: Option<String>,
    pub result_event_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallPage {
    pub items: Vec<ToolCallListItem>,
    pub page: u32,
    pub page_size: u32,
    pub total: i64,
    pub generated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallDetail {
    pub call: ToolCallListItem,
    pub call_event: Option<SessionEventItem>,
    pub result_event: Option<SessionEventItem>,
    pub session_event_id: Option<String>,
}
