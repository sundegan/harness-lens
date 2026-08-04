use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsSnapshot {
    pub sync: SyncStatus,
    pub summary: OverallSummary,
    pub sessions: Vec<SessionSummary>,
    pub skills: Vec<SkillSummary>,
    pub generated_at_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub status: String,
    pub phase: String,
    pub processed_records: i64,
    pub diagnostic_count: i64,
    pub last_error: Option<String>,
    pub updated_at_ms: Option<i64>,
}

impl Default for SyncStatus {
    fn default() -> Self {
        Self {
            status: "not_started".to_owned(),
            phase: "idle".to_owned(),
            processed_records: 0,
            diagnostic_count: 0,
            last_error: None,
            updated_at_ms: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverallSummary {
    pub session_count: i64,
    pub total_tokens: i64,
    pub skill_invocation_count: i64,
    pub succeeded_turn_count: i64,
    pub failed_turn_count: i64,
    pub cancelled_turn_count: i64,
    pub active_turn_count: i64,
    pub unknown_turn_count: i64,
}

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
    pub turn_count: i64,
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
