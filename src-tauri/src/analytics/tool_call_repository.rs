use std::borrow::Borrow;
use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

use chrono::{DateTime, Datelike, Duration, LocalResult, TimeZone, Timelike, Utc};
use chrono_tz::Tz;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params_from_iter, Connection, OptionalExtension, Row};

use super::model::{
    MetricRate, SessionEventItem, ToolCallAnalysis, ToolCallAnalysisRequest, ToolCallComparison,
    ToolCallDetail, ToolCallFilterOption, ToolCallFilterOptions, ToolCallFilterOptionsRequest,
    ToolCallFilters, ToolCallListItem, ToolCallPage, ToolCallPageRequest, ToolCallSummary,
    ToolCallTrendPoint,
};
use super::repository::now_ms;
use crate::database::{Database, DatabaseError};

#[derive(Clone, Debug)]
struct CallRow {
    item: ToolCallListItem,
    project_key: String,
    namespace: Option<String>,
    repeat_index: i64,
}

#[derive(Clone, Debug)]
struct FilterOptionRow {
    provider: String,
    project_key: String,
    project_name: String,
    tool_name: String,
    mcp_server: String,
}

#[derive(Default)]
struct FilterSql {
    clauses: Vec<String>,
    params: Vec<SqlValue>,
}

impl FilterSql {
    fn new(filters: &ToolCallFilters) -> Self {
        Self::excluding(filters, None)
    }

    fn excluding(filters: &ToolCallFilters, excluded: Option<&str>) -> Self {
        let mut sql = Self::default();
        if let Some(value) = filters.start_at_ms {
            sql.clauses.push(
                "(tc.started_at_ms >= ? OR (tc.started_at_ms IS NULL AND tc.completed_at_ms >= ?))"
                    .to_owned(),
            );
            sql.params.push(SqlValue::Integer(value));
            sql.params.push(SqlValue::Integer(value));
        }
        if let Some(value) = filters.end_at_ms {
            sql.clauses.push(
                "(tc.started_at_ms < ? OR (tc.started_at_ms IS NULL AND tc.completed_at_ms < ?))"
                    .to_owned(),
            );
            sql.params.push(SqlValue::Integer(value));
            sql.params.push(SqlValue::Integer(value));
        }
        if excluded != Some("providers") {
            sql.add_in("tc.provider", &filters.providers);
        }
        if excluded != Some("project_keys") {
            sql.add_in("COALESCE(s.project_key, '')", &filters.project_keys);
        }
        if excluded != Some("tool_names") {
            sql.add_in("tc.tool_name", &filters.tool_names);
        }
        if excluded != Some("mcp_servers") {
            sql.add_in("COALESCE(tc.mcp_server, '')", &filters.mcp_servers);
        }
        sql
    }

    fn add_in(&mut self, column: &str, values: &[String]) {
        if values.is_empty() {
            return;
        }
        self.clauses.push(format!(
            "{column} IN ({})",
            std::iter::repeat_n("?", values.len())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        self.params
            .extend(values.iter().cloned().map(SqlValue::Text));
    }

    fn where_sql(&self) -> String {
        if self.clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.clauses.join(" AND "))
        }
    }
}

const CALL_COLUMNS: &str = "
    tc.id, tc.provider, tc.session_id, s.source_session_id, s.title,
    COALESCE(s.project_key, ''), s.project_name, s.agent_version,
    tc.tool_name, tc.namespace, tc.mcp_server, tc.tool_kind,
    tc.started_at_ms, tc.completed_at_ms, tc.duration_ms, tc.status, tc.has_result,
    tc.repeat_index, tc.call_event_id, tc.result_event_id
";

pub(super) fn analysis(
    database: &Database,
    request: ToolCallAnalysisRequest,
) -> Result<ToolCallAnalysis, DatabaseError> {
    validate_time_options(&request.filters)?;
    let connection = database.connect()?;
    let calls = query_calls(&connection, &request.filters)?;
    Ok(ToolCallAnalysis {
        time_trend: trend(&calls, &request.filters)?,
        status_distribution: comparisons(&calls, |row| {
            (row.item.status.as_str(), row.item.status.as_str())
        }),
        tool_ranking: ranked(comparisons(&calls, |row| {
            (row.item.tool_name.as_str(), row.item.tool_name.as_str())
        })),
        provider_comparison: comparisons(&calls, |row| {
            (row.item.provider.as_str(), row.item.provider.as_str())
        }),
        project_comparison: ranked(comparisons(&calls, |row| {
            (row.project_key.as_str(), row.item.project_name.as_str())
        })),
        mcp_server_comparison: ranked(comparisons(&calls, |row| {
            let value = row.item.mcp_server.as_deref().unwrap_or("unknown");
            (value, value)
        })),
        summary: summarize(&calls),
        generated_at_ms: now_ms(),
    })
}

pub(super) fn filter_options(
    database: &Database,
    request: ToolCallFilterOptionsRequest,
) -> Result<ToolCallFilterOptions, DatabaseError> {
    validate_time_options(&request.filters)?;
    let connection = database.connect()?;
    let rows = query_filter_option_rows(&connection, &request.filters)?;
    Ok(ToolCallFilterOptions {
        providers: facet_options(&rows, &request.filters, "providers", |row| {
            Some((row.provider.as_str(), row.provider.as_str()))
        }),
        projects: facet_options(&rows, &request.filters, "project_keys", |row| {
            Some((
                row.project_key.as_str(),
                label_or_unknown(&row.project_name),
            ))
        }),
        tool_names: facet_options(&rows, &request.filters, "tool_names", |row| {
            Some((row.tool_name.as_str(), row.tool_name.as_str()))
        }),
        mcp_servers: facet_options(&rows, &request.filters, "mcp_servers", |row| {
            Some((row.mcp_server.as_str(), label_or_unknown(&row.mcp_server)))
        }),
    })
}

pub(super) fn page(
    database: &Database,
    mut request: ToolCallPageRequest,
) -> Result<ToolCallPage, DatabaseError> {
    validate_time_options(&request.filters)?;
    request.page = request.page.max(1);
    request.page_size = request.page_size.clamp(1, 200);
    let connection = database.connect()?;
    let mut filter = FilterSql::new(&request.filters);
    if let Some(query) = request
        .query
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        filter.clauses.push(
            "(tc.tool_name LIKE ? ESCAPE '\\' OR COALESCE(tc.namespace, '') LIKE ? ESCAPE '\\' OR COALESCE(tc.mcp_server, '') LIKE ? ESCAPE '\\' OR s.title LIKE ? ESCAPE '\\' OR s.project_name LIKE ? ESCAPE '\\')".to_owned(),
        );
        let pattern = format!("%{}%", escape_like(query));
        filter
            .params
            .extend(std::iter::repeat_n(SqlValue::Text(pattern), 5));
    }
    let from = " FROM mcp_tool_call tc JOIN agent_sessions s ON s.id = tc.session_id";
    let where_sql = filter.where_sql();
    let total = connection
        .query_row(
            &format!("SELECT COUNT(*){from}{where_sql}"),
            params_from_iter(filter.params.iter()),
            |row| row.get(0),
        )
        .map_err(|source| DatabaseError::sqlite("count filtered tool calls", source))?;

    let sort = match request.sort_by.as_deref() {
        Some("toolName") => "tc.tool_name",
        Some("status") => "tc.status",
        Some("durationMs") => "tc.duration_ms",
        Some("provider") => "tc.provider",
        _ => "COALESCE(tc.started_at_ms, tc.completed_at_ms)",
    };
    let direction = if request.sort_direction.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };
    let mut params = filter.params;
    params.push(SqlValue::Integer(i64::from(request.page_size)));
    params.push(SqlValue::Integer(i64::from(
        (request.page - 1).saturating_mul(request.page_size),
    )));
    let sql = format!(
        "SELECT {CALL_COLUMNS}{from}{where_sql} ORDER BY {sort} {direction}, tc.id {direction} LIMIT ? OFFSET ?"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|source| DatabaseError::sqlite("prepare tool-call page", source))?;
    let items = statement
        .query_map(params_from_iter(params.iter()), map_call_row)
        .map_err(|source| DatabaseError::sqlite("query tool-call page", source))?
        .map(|row| row.map(|value| value.item))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| DatabaseError::sqlite("read tool-call page", source))?;
    Ok(ToolCallPage {
        items,
        page: request.page,
        page_size: request.page_size,
        total,
        generated_at_ms: now_ms(),
    })
}

pub(super) fn detail(
    database: &Database,
    id: &str,
) -> Result<Option<ToolCallDetail>, DatabaseError> {
    let connection = database.connect()?;
    let call = query_call_by(&connection, "tc.id = ?1", id)?;
    let Some(call) = call else {
        return Ok(None);
    };
    let call_event = event_by_id(&connection, call.item.call_event_id.as_deref())?;
    let result_event = event_by_id(&connection, call.item.result_event_id.as_deref())?;
    let session_event_id = call
        .item
        .call_event_id
        .clone()
        .or_else(|| call.item.result_event_id.clone());
    Ok(Some(ToolCallDetail {
        call: call.item,
        call_event,
        result_event,
        session_event_id,
    }))
}

fn query_calls(
    connection: &Connection,
    filters: &ToolCallFilters,
) -> Result<Vec<CallRow>, DatabaseError> {
    let filter = FilterSql::new(filters);
    let sql = format!(
        "SELECT {CALL_COLUMNS} FROM mcp_tool_call tc JOIN agent_sessions s ON s.id = tc.session_id{}",
        filter.where_sql()
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|source| DatabaseError::sqlite("prepare tool-call analysis", source))?;
    let rows = statement
        .query_map(params_from_iter(filter.params.iter()), map_call_row)
        .map_err(|source| DatabaseError::sqlite("query tool-call analysis", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| DatabaseError::sqlite("read tool-call analysis", source))?;
    Ok(rows)
}

fn query_call_by(
    connection: &Connection,
    predicate: &str,
    value: &str,
) -> Result<Option<CallRow>, DatabaseError> {
    connection
        .query_row(
            &format!(
                "SELECT {CALL_COLUMNS} FROM mcp_tool_call tc JOIN agent_sessions s ON s.id = tc.session_id WHERE {predicate}"
            ),
            [value],
            map_call_row,
        )
        .optional()
        .map_err(|source| DatabaseError::sqlite("load tool-call detail", source))
}

fn map_call_row(row: &Row<'_>) -> rusqlite::Result<CallRow> {
    Ok(CallRow {
        item: ToolCallListItem {
            id: row.get(0)?,
            provider: row.get(1)?,
            session_id: row.get(2)?,
            source_session_id: row.get(3)?,
            session_title: row.get(4)?,
            project_name: row.get(6)?,
            agent_version: row.get(7)?,
            tool_name: row.get(8)?,
            mcp_server: row.get(10)?,
            tool_kind: row.get(11)?,
            started_at_ms: row.get(12)?,
            completed_at_ms: row.get(13)?,
            duration_ms: row.get(14)?,
            status: row.get(15)?,
            has_result: row.get::<_, i64>(16)? != 0,
            call_event_id: row.get(18)?,
            result_event_id: row.get(19)?,
        },
        project_key: row.get(5)?,
        namespace: row.get(9)?,
        repeat_index: row.get(17)?,
    })
}

fn summarize<T>(calls: &[T]) -> ToolCallSummary
where
    T: Borrow<CallRow>,
{
    let call_count = calls.len() as i64;
    let completed_count = count(calls, |row| row.item.status == "completed");
    let failed_count = count(calls, |row| row.item.status == "failed");
    let durations = calls
        .iter()
        .filter_map(|row| row.borrow().item.duration_ms)
        .collect::<Vec<_>>();
    let terminal = completed_count + failed_count;
    ToolCallSummary {
        call_count,
        tool_count: unique_tools(calls),
        session_count: unique(calls, |row| row.item.session_id.as_str()),
        project_count: unique_known_projects(calls),
        average_duration_ms: average(&durations),
        success_rate: rate(completed_count, terminal, call_count - terminal),
    }
}

fn comparisons<'a, I, F>(calls: I, key: F) -> Vec<ToolCallComparison>
where
    I: IntoIterator<Item = &'a CallRow>,
    F: Fn(&'a CallRow) -> (&'a str, &'a str),
{
    let mut groups = BTreeMap::<(&str, &str), Vec<&CallRow>>::new();
    for call in calls {
        groups.entry(key(call)).or_default().push(call);
    }
    groups
        .into_iter()
        .map(|((key, label), rows)| {
            let summary = summarize(&rows);
            ToolCallComparison {
                key: key.to_owned(),
                label: label.to_owned(),
                call_count: summary.call_count,
                session_count: summary.session_count,
                project_count: unique_known_projects(&rows),
                failed_count: count(&rows, |row| row.item.status == "failed"),
                declined_count: count(&rows, |row| row.item.status == "declined"),
                cancelled_count: count(&rows, |row| row.item.status == "cancelled"),
                average_duration_ms: summary.average_duration_ms,
                exact_repeat_count: count(&rows, |row| row.repeat_index > 0),
                success_rate: summary.success_rate.rate,
            }
        })
        .collect()
}

fn ranked(mut values: Vec<ToolCallComparison>) -> Vec<ToolCallComparison> {
    values.sort_by(|left, right| {
        right
            .call_count
            .cmp(&left.call_count)
            .then_with(|| left.label.cmp(&right.label))
    });
    values
}

fn trend(
    calls: &[CallRow],
    filters: &ToolCallFilters,
) -> Result<Vec<ToolCallTrendPoint>, DatabaseError> {
    let timezone = filters.timezone.as_deref().unwrap_or("UTC");
    let tz = Tz::from_str(timezone).map_err(|_| invalid_request("invalid IANA timezone"))?;
    let bucket = filters.bucket.as_deref().unwrap_or("day");
    let mut groups = BTreeMap::<i64, Vec<&CallRow>>::new();
    for call in calls {
        let Some(timestamp) = call.item.started_at_ms.or(call.item.completed_at_ms) else {
            continue;
        };
        let local = Utc
            .timestamp_millis_opt(timestamp)
            .single()
            .ok_or_else(|| invalid_request("invalid tool-call timestamp"))?
            .with_timezone(&tz);
        let local_start = bucket_start(local, tz, bucket)?;
        groups
            .entry(local_start.with_timezone(&Utc).timestamp_millis())
            .or_default()
            .push(call);
    }
    let bounds = match (filters.start_at_ms, filters.end_at_ms) {
        (Some(start), Some(end)) => Some((start, end.saturating_sub(1))),
        (Some(start), None) => Some((start, now_ms())),
        (None, Some(end)) => Some((
            groups
                .keys()
                .next()
                .copied()
                .unwrap_or(end.saturating_sub(1)),
            end.saturating_sub(1),
        )),
        (None, None) => groups
            .keys()
            .next()
            .zip(groups.keys().next_back())
            .map(|(start, end)| (*start, *end)),
    };
    let Some((range_start, range_end)) = bounds else {
        return Ok(Vec::new());
    };
    let start_local = Utc
        .timestamp_millis_opt(range_start)
        .single()
        .ok_or_else(|| invalid_request("invalid trend start timestamp"))?
        .with_timezone(&tz);
    let end_local = Utc
        .timestamp_millis_opt(range_end)
        .single()
        .ok_or_else(|| invalid_request("invalid trend end timestamp"))?
        .with_timezone(&tz);
    let mut cursor = bucket_start(start_local, tz, bucket)?;
    let end = bucket_start(end_local, tz, bucket)?;
    let mut points = Vec::new();
    while cursor <= end {
        let bucket_start_ms = cursor.with_timezone(&Utc).timestamp_millis();
        let rows = groups.remove(&bucket_start_ms).unwrap_or_default();
        let summary = summarize(&rows);
        points.push(ToolCallTrendPoint {
            bucket_start_ms,
            label: cursor.format("%Y-%m-%d %H:%M").to_string(),
            call_count: summary.call_count,
            failed_count: count(&rows, |row| row.item.status == "failed"),
        });
        cursor = next_bucket(cursor, tz, bucket)?;
    }
    Ok(points)
}

fn bucket_start(local: DateTime<Tz>, tz: Tz, bucket: &str) -> Result<DateTime<Tz>, DatabaseError> {
    match bucket {
        "hour" => local
            .with_minute(0)
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0)),
        "day" => local_midnight(tz, local.year(), local.month(), local.day()),
        "week" => {
            let day = local.date_naive()
                - Duration::days(i64::from(local.weekday().num_days_from_monday()));
            local_midnight(tz, day.year(), day.month(), day.day())
        }
        "month" => local_midnight(tz, local.year(), local.month(), 1),
        _ => return Err(invalid_request("invalid trend bucket")),
    }
    .ok_or_else(|| invalid_request("time bucket cannot be represented"))
}

fn next_bucket(current: DateTime<Tz>, tz: Tz, bucket: &str) -> Result<DateTime<Tz>, DatabaseError> {
    match bucket {
        "hour" => Ok(current + Duration::hours(1)),
        "day" => {
            let next = current.date_naive() + Duration::days(1);
            local_midnight(tz, next.year(), next.month(), next.day())
                .ok_or_else(|| invalid_request("next day bucket cannot be represented"))
        }
        "week" => {
            let next = current.date_naive() + Duration::days(7);
            local_midnight(tz, next.year(), next.month(), next.day())
                .ok_or_else(|| invalid_request("next week bucket cannot be represented"))
        }
        "month" => {
            let (year, month) = if current.month() == 12 {
                (current.year() + 1, 1)
            } else {
                (current.year(), current.month() + 1)
            };
            local_midnight(tz, year, month, 1)
                .ok_or_else(|| invalid_request("next month bucket cannot be represented"))
        }
        _ => Err(invalid_request("invalid trend bucket")),
    }
}

fn local_midnight(tz: Tz, year: i32, month: u32, day: u32) -> Option<DateTime<Tz>> {
    match tz.with_ymd_and_hms(year, month, day, 0, 0, 0) {
        LocalResult::Single(value) => Some(value),
        LocalResult::Ambiguous(earliest, _) => Some(earliest),
        LocalResult::None => None,
    }
}

fn query_filter_option_rows(
    connection: &Connection,
    filters: &ToolCallFilters,
) -> Result<Vec<FilterOptionRow>, DatabaseError> {
    let time_filters = ToolCallFilters {
        start_at_ms: filters.start_at_ms,
        end_at_ms: filters.end_at_ms,
        ..ToolCallFilters::default()
    };
    let filter = FilterSql::new(&time_filters);
    let where_sql = filter.where_sql();
    let sql = format!(
        "SELECT
            tc.provider, COALESCE(s.project_key, ''), COALESCE(s.project_name, ''),
            tc.tool_name, COALESCE(tc.mcp_server, '')
         FROM mcp_tool_call tc
         JOIN agent_sessions s ON s.id = tc.session_id{where_sql}"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|source| DatabaseError::sqlite("prepare tool-call filter options", source))?;
    let rows = statement
        .query_map(params_from_iter(filter.params.iter()), |row| {
            Ok(FilterOptionRow {
                provider: row.get(0)?,
                project_key: row.get(1)?,
                project_name: row.get(2)?,
                tool_name: row.get(3)?,
                mcp_server: row.get(4)?,
            })
        })
        .map_err(|source| DatabaseError::sqlite("query tool-call filter options", source))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| DatabaseError::sqlite("read tool-call filter options", source))?;
    Ok(rows)
}

fn facet_options<'a, F>(
    rows: &'a [FilterOptionRow],
    filters: &ToolCallFilters,
    excluded: &str,
    value: F,
) -> Vec<ToolCallFilterOption>
where
    F: Fn(&'a FilterOptionRow) -> Option<(&'a str, &'a str)>,
{
    let mut counts = BTreeMap::<(&str, &str), i64>::new();
    for row in rows
        .iter()
        .filter(|row| filter_option_row_matches(row, filters, Some(excluded)))
    {
        if let Some((value, label)) = value(row) {
            *counts.entry((value, label)).or_default() += 1;
        }
    }
    let mut options = counts
        .into_iter()
        .map(|((value, label), count)| ToolCallFilterOption {
            value: value.to_owned(),
            label: label.to_owned(),
            count,
        })
        .collect::<Vec<_>>();
    options.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    options
}

fn filter_option_row_matches(
    row: &FilterOptionRow,
    filters: &ToolCallFilters,
    excluded: Option<&str>,
) -> bool {
    (excluded == Some("providers") || selected(&filters.providers, &row.provider))
        && (excluded == Some("project_keys") || selected(&filters.project_keys, &row.project_key))
        && (excluded == Some("tool_names") || selected(&filters.tool_names, &row.tool_name))
        && (excluded == Some("mcp_servers") || selected(&filters.mcp_servers, &row.mcp_server))
}

fn selected(values: &[String], value: &str) -> bool {
    values.is_empty() || values.iter().any(|selected| selected == value)
}

fn label_or_unknown(value: &str) -> &str {
    if value.is_empty() {
        "unknown"
    } else {
        value
    }
}

fn event_by_id(
    connection: &Connection,
    id: Option<&str>,
) -> Result<Option<SessionEventItem>, DatabaseError> {
    let Some(id) = id else {
        return Ok(None);
    };
    connection
        .query_row(
            "SELECT id, invocation_id, timestamp_ms, event_type, event_json FROM session_events WHERE id = ?1",
            [id],
            |row| {
                let json: String = row.get(4)?;
                let event = serde_json::from_str(&json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(SessionEventItem {
                    id: row.get(0)?,
                    invocation_id: row.get(1)?,
                    timestamp_ms: row.get(2)?,
                    event_type: row.get(3)?,
                    event,
                })
            },
        )
        .optional()
        .map_err(|source| DatabaseError::sqlite("load tool-call evidence", source))
}

fn validate_time_options(filters: &ToolCallFilters) -> Result<(), DatabaseError> {
    if let (Some(start), Some(end)) = (filters.start_at_ms, filters.end_at_ms) {
        if start >= end {
            return Err(invalid_request("startAtMs must be before endAtMs"));
        }
    }
    let timezone = filters.timezone.as_deref().unwrap_or("UTC");
    Tz::from_str(timezone).map_err(|_| invalid_request("invalid IANA timezone"))?;
    if !matches!(
        filters.bucket.as_deref().unwrap_or("day"),
        "hour" | "day" | "week" | "month"
    ) {
        return Err(invalid_request("invalid trend bucket"));
    }
    Ok(())
}

fn invalid_request(message: &str) -> DatabaseError {
    DatabaseError::sqlite(
        "validate tool-call request",
        rusqlite::Error::InvalidParameterName(message.to_owned()),
    )
}

fn count<T, F>(calls: &[T], predicate: F) -> i64
where
    T: Borrow<CallRow>,
    F: Fn(&CallRow) -> bool,
{
    calls
        .iter()
        .filter(|row| predicate((*row).borrow()))
        .count() as i64
}

fn unique<'a, T, F>(calls: &'a [T], key: F) -> i64
where
    T: Borrow<CallRow>,
    F: Fn(&'a CallRow) -> &'a str,
{
    calls
        .iter()
        .map(|row| key(row.borrow()))
        .collect::<HashSet<_>>()
        .len() as i64
}

fn unique_tools<T>(calls: &[T]) -> i64
where
    T: Borrow<CallRow>,
{
    calls
        .iter()
        .map(|row| {
            let row = row.borrow();
            let item = &row.item;
            (
                item.provider.as_str(),
                row.namespace.as_deref(),
                item.mcp_server.as_deref(),
                item.tool_name.as_str(),
            )
        })
        .collect::<HashSet<_>>()
        .len() as i64
}

fn unique_known_projects<T>(calls: &[T]) -> i64
where
    T: Borrow<CallRow>,
{
    calls
        .iter()
        .map(|row| row.borrow())
        .filter(|row| !row.project_key.is_empty())
        .map(|row| row.project_key.as_str())
        .collect::<HashSet<_>>()
        .len() as i64
}

fn average(values: &[i64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<i64>() as f64 / values.len() as f64)
}

fn rate(numerator: i64, denominator: i64, unknown_count: i64) -> MetricRate {
    MetricRate {
        numerator,
        denominator,
        rate: (denominator > 0).then(|| numerator as f64 / denominator as f64),
        unknown_count,
    }
}

fn escape_like(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    fn repository_fixture() -> (Database, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "harness-lens-tool-call-repository-{}-{}",
            std::process::id(),
            NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let path = root.join("harness-lens.sqlite");
        let database = Database::initialize(&path).unwrap();
        let connection = database.connect().unwrap();

        connection
            .execute_batch(
                r#"
                INSERT INTO agent_sessions (
                    id, provider, source_id, source_session_id, title, project_name,
                    project_key, agent_version, metadata_present, data_quality
                ) VALUES
                    ('session-codex', 'codex', 'codex:test', 'codex-session',
                     'Codex fixture', 'Project Alpha', 'project-alpha', '1.0.0', 1, 'complete'),
                    ('session-claude', 'claude-code', 'claude:test', 'claude-session',
                     'Claude fixture', 'Project Beta', 'project-beta', '2.0.0', 1, 'complete');

                INSERT INTO agent_invocations (
                    id, source_id, session_id, source_path, started_at_ms, status
                ) VALUES
                    ('invocation-codex', 'codex:test', 'session-codex', '/fixture/codex.jsonl', 1690848000000, 'failed'),
                    ('invocation-claude', 'claude:test', 'session-claude', '/fixture/claude.jsonl', 1691020800000, 'cancelled');

                INSERT INTO session_events (
                    id, provider, source_id, session_id, invocation_id, source_path,
                    timestamp_ms, sequence_position, sequence_part, event_type, event_json
                ) VALUES
                    ('event-call-3', 'codex', 'codex:test', 'session-codex', 'invocation-codex',
                     '/fixture/codex.jsonl', 1691020800000, 5, 0, 'tool_call',
                     '{"type":"tool_call","value":{"call_id":"call-3","input":{"value":1}}}'),
                    ('event-result-3', 'codex', 'codex:test', 'session-codex', 'invocation-codex',
                     '/fixture/codex.jsonl', 1691020800200, 6, 0, 'tool_result',
                     '{"type":"tool_result","value":{"call_id":"call-3","output":{"ok":true}}}'),
                    ('event-retry-1', 'codex', 'codex:test', 'session-codex', 'invocation-codex',
                     '/fixture/codex.jsonl', 1691020799900, 4, 0, 'retry',
                     '{"type":"retry","value":{"attempt":2}}'),
                    ('event-retry-unattributed', 'codex', 'codex:test', 'session-codex', 'invocation-codex',
                     '/fixture/codex.jsonl', 1691020799950, 4, 1, 'retry',
                     '{"type":"retry","value":{"attempt":3}}');

                INSERT INTO mcp_tool_call (
                    id, provider, source_id, session_id, invocation_id, source_path, call_id,
                    tool_name, mcp_server, tool_kind, started_at_ms, completed_at_ms,
                    duration_ms, duration_source, status, has_result, has_error,
                    call_event_id, result_event_id, input_fingerprint, repeat_group_id,
                    repeat_of_id, repeat_index, retry_class, retry_of_id, evidence_quality
                ) VALUES
                    ('call-1', 'codex', 'codex:test', 'session-codex', 'invocation-codex',
                     '/fixture/codex.jsonl', 'native-1', 'query', 'database', 'fetch',
                     1690848000000, 1690848000100, 100, 'normalized', 'completed', 1, 0,
                     NULL, NULL, 'fingerprint-a', 'repeat-a', NULL, 0, 'none', NULL, 'observed'),
                    ('call-2', 'codex', 'codex:test', 'session-codex', 'invocation-codex',
                     '/fixture/codex.jsonl', 'native-2', 'query', 'database', 'fetch',
                     1690851600000, 1690851600100, NULL, 'unknown', 'failed', 1, 1,
                     NULL, NULL, 'fingerprint-a', 'repeat-a', 'call-1', 1, 'none', NULL, 'observed'),
                    ('call-3', 'codex', 'codex:test', 'session-codex', 'invocation-codex',
                     '/fixture/codex.jsonl', 'native-3', 'query', 'database', 'fetch',
                     1691020800000, 1691020800200, 200, 'event_delta', 'completed', 1, 0,
                     'event-call-3', 'event-result-3', 'fingerprint-a', 'repeat-a', 'call-2', 2,
                     'inferred', 'call-2', 'observed'),
                    ('call-4', 'claude-code', 'claude:test', 'session-claude', 'invocation-claude',
                     '/fixture/claude.jsonl', 'native-4', 'read_file', 'filesystem', 'read',
                     1691020800000, 1691020800050, 50, 'normalized', 'declined', 1, 0,
                     NULL, NULL, 'fingerprint-b', 'repeat-b', NULL, 0, 'none', NULL, 'observed'),
                    ('call-5', 'claude-code', 'claude:test', 'session-claude', 'invocation-claude',
                     '/fixture/claude.jsonl', 'native-5', 'search', 'web', 'search',
                     1691024400000, NULL, NULL, 'unknown', 'cancelled', 0, 0,
                     NULL, NULL, 'fingerprint-c', 'repeat-c', NULL, 0, 'none', NULL, 'observed'),
                    ('call-6', 'claude-code', 'claude:test', 'session-claude', 'invocation-claude',
                     '/fixture/claude.jsonl', 'native-6', 'query', 'database', 'fetch',
                     NULL, NULL, NULL, 'unknown', 'unknown', 0, 0,
                     NULL, NULL, 'fingerprint-d', 'repeat-d', NULL, 0, 'none', NULL, 'unknown');

                INSERT INTO mcp_tool_call_retry (
                    id, provider, source_id, session_id, invocation_id, source_path,
                    timestamp_ms, mcp_tool_call_id, attempt, evidence_quality
                ) VALUES
                    ('event-retry-1', 'codex', 'codex:test', 'session-codex', 'invocation-codex',
                     '/fixture/codex.jsonl', 1691020799900, 'call-3', 2, 'observed');
                "#,
            )
            .unwrap();
        drop(connection);
        (database, root)
    }

    #[test]
    fn validates_iana_timezone_and_range() {
        let valid = ToolCallFilters {
            timezone: Some("Asia/Shanghai".to_owned()),
            bucket: Some("day".to_owned()),
            start_at_ms: Some(1),
            end_at_ms: Some(2),
            ..ToolCallFilters::default()
        };
        assert!(validate_time_options(&valid).is_ok());

        let invalid_zone = ToolCallFilters {
            timezone: Some("Local/Unknown".to_owned()),
            ..ToolCallFilters::default()
        };
        assert!(validate_time_options(&invalid_zone).is_err());

        let invalid_range = ToolCallFilters {
            start_at_ms: Some(2),
            end_at_ms: Some(2),
            ..ToolCallFilters::default()
        };
        assert!(validate_time_options(&invalid_range).is_err());
    }

    #[test]
    fn filter_sql_uses_placeholders_for_multiselect_values() {
        let filters = ToolCallFilters {
            providers: vec!["codex' OR 1=1 --".to_owned(), "claude-code".to_owned()],
            ..ToolCallFilters::default()
        };
        let sql = FilterSql::new(&filters);
        let where_sql = sql.where_sql();

        assert!(where_sql.contains("tc.provider IN (?, ?)"));
        assert!(!where_sql.contains("OR 1=1"));
        assert_eq!(sql.params.len(), 2);
    }

    #[test]
    fn time_filter_keeps_started_at_index_eligible_and_excludes_unknown_timestamps() {
        let filters = ToolCallFilters {
            start_at_ms: Some(100),
            end_at_ms: Some(200),
            ..ToolCallFilters::default()
        };
        let sql = FilterSql::new(&filters);
        let where_sql = sql.where_sql();

        assert!(where_sql.contains("tc.started_at_ms >= ?"));
        assert!(where_sql.contains("tc.completed_at_ms >= ?"));
        assert!(where_sql.contains("tc.started_at_ms < ?"));
        assert!(where_sql.contains("tc.completed_at_ms < ?"));
        assert!(!where_sql.contains("tc.completed_at_ms IS NULL"));
        assert!(!where_sql.contains("COALESCE(tc.started_at_ms"));
        assert_eq!(sql.params.len(), 4);
    }

    #[test]
    fn dst_day_buckets_keep_distinct_local_dates() {
        let calls = vec![
            fixture_call(1_763_268_600_000),
            fixture_call(1_763_355_000_000),
        ];
        let filters = ToolCallFilters {
            timezone: Some("America/New_York".to_owned()),
            bucket: Some("day".to_owned()),
            ..ToolCallFilters::default()
        };
        let points = trend(&calls, &filters).unwrap();
        assert_eq!(points.len(), 2);
    }

    #[test]
    fn tool_count_uses_the_full_tool_identity() {
        let first = fixture_call(1);
        let mut second = fixture_call(2);
        second.namespace = Some("another-namespace".to_owned());

        assert_eq!(summarize(&[first, second]).tool_count, 2);
    }

    #[test]
    fn project_count_excludes_calls_without_a_project_identity() {
        let known = fixture_call(1);
        let mut unknown = fixture_call(2);
        unknown.project_key.clear();

        assert_eq!(summarize(&[known, unknown]).project_count, 1);
    }

    #[test]
    fn tool_ranking_keeps_every_group_for_pagination() {
        let calls = (0..25)
            .map(|index| {
                let mut call = fixture_call(index);
                call.item.tool_name = format!("tool-{index:02}");
                call
            })
            .collect::<Vec<_>>();
        let ranking = ranked(comparisons(&calls, |row| {
            (row.item.tool_name.as_str(), row.item.tool_name.as_str())
        }));

        assert_eq!(ranking.len(), 25);
        assert_eq!(ranking.first().unwrap().key, "tool-00");
        assert_eq!(ranking.last().unwrap().key, "tool-24");
    }

    #[test]
    fn repository_analysis_preserves_metric_denominators_and_comparison_dimensions() {
        let (database, root) = repository_fixture();
        let result = analysis(
            &database,
            ToolCallAnalysisRequest {
                filters: ToolCallFilters {
                    start_at_ms: Some(1_690_848_000_000),
                    end_at_ms: Some(1_691_107_200_000),
                    timezone: Some("UTC".to_owned()),
                    bucket: Some("day".to_owned()),
                    ..ToolCallFilters::default()
                },
            },
        )
        .unwrap();

        assert_eq!(result.summary.call_count, 5);
        assert_eq!(result.summary.project_count, 2);
        assert_eq!(result.summary.success_rate.numerator, 2);
        assert_eq!(result.summary.success_rate.denominator, 3);
        assert_eq!(result.summary.average_duration_ms, Some(350.0 / 3.0));
        assert_eq!(result.time_trend.len(), 3);
        assert_eq!(result.time_trend[0].call_count, 2);
        assert_eq!(result.time_trend[1].call_count, 0);
        assert_eq!(result.time_trend[2].call_count, 3);
        assert_eq!(result.provider_comparison.len(), 2);
        assert!(result
            .provider_comparison
            .iter()
            .all(|row| row.project_count == 1));
        assert_eq!(result.project_comparison.len(), 2);
        assert_eq!(result.mcp_server_comparison.len(), 3);
        assert!(result
            .mcp_server_comparison
            .iter()
            .all(|row| matches!(row.key.as_str(), "filesystem" | "database" | "web")));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repository_filters_pages_and_details_use_the_same_evidence() {
        let (database, root) = repository_fixture();
        let filters = ToolCallFilters {
            providers: vec!["codex".to_owned()],
            timezone: Some("UTC".to_owned()),
            bucket: Some("day".to_owned()),
            ..ToolCallFilters::default()
        };
        let options = filter_options(
            &database,
            ToolCallFilterOptionsRequest {
                filters: filters.clone(),
            },
        )
        .unwrap();
        assert_eq!(options.providers.len(), 2);
        assert_eq!(options.projects.len(), 1);
        assert_eq!(options.projects[0].value, "project-alpha");
        assert_eq!(options.providers.len(), 2);
        assert_eq!(options.providers[0].value, "claude-code");
        assert_eq!(options.providers[0].count, 3);
        assert_eq!(options.providers[1].value, "codex");
        assert_eq!(options.providers[1].count, 3);
        let mcp = page(
            &database,
            ToolCallPageRequest {
                filters: ToolCallFilters {
                    mcp_servers: vec!["filesystem".to_owned()],
                    timezone: Some("UTC".to_owned()),
                    bucket: Some("day".to_owned()),
                    ..ToolCallFilters::default()
                },
                page: 1,
                page_size: 1,
                query: Some("filesystem".to_owned()),
                sort_by: Some("toolName".to_owned()),
                sort_direction: Some("asc".to_owned()),
            },
        )
        .unwrap();
        assert_eq!(mcp.total, 1);
        assert_eq!(mcp.items[0].mcp_server.as_deref(), Some("filesystem"));

        let selected_detail = detail(&database, "call-3").unwrap().unwrap();
        assert_eq!(selected_detail.call.source_session_id, "codex-session");
        assert_eq!(
            selected_detail.call_event.as_ref().unwrap().id,
            "event-call-3"
        );
        assert_eq!(
            selected_detail.result_event.as_ref().unwrap().id,
            "event-result-3"
        );
        assert_eq!(
            selected_detail.session_event_id.as_deref(),
            Some("event-call-3")
        );

        fs::remove_dir_all(root).unwrap();
    }

    fn fixture_call(timestamp: i64) -> CallRow {
        CallRow {
            item: ToolCallListItem {
                id: timestamp.to_string(),
                provider: "codex".to_owned(),
                session_id: "session".to_owned(),
                source_session_id: "source-session".to_owned(),
                session_title: "Fixture".to_owned(),
                project_name: "Project".to_owned(),
                agent_version: Some("1".to_owned()),
                tool_name: "exec_command".to_owned(),
                mcp_server: Some("database".to_owned()),
                tool_kind: "execute".to_owned(),
                started_at_ms: Some(timestamp),
                completed_at_ms: Some(timestamp + 1),
                duration_ms: Some(1),
                status: "completed".to_owned(),
                has_result: true,
                call_event_id: None,
                result_event_id: None,
            },
            project_key: "project".to_owned(),
            namespace: None,
            repeat_index: 0,
        }
    }
}
