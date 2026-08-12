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
