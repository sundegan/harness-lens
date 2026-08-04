use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::time::Duration;

use coding_agent_data::providers::claude_code::ClaudeCodeProvider;
use coding_agent_data::providers::codex::CodexProvider;
use coding_agent_data::{Change, ItemData, Provider, Record, RecordData, RecordId, Result};
use indicatif::{ProgressBar, ProgressStyle};

fn main() -> Result<()> {
    report_provider("codex", &CodexProvider::discover()?)?;
    report_provider("claude-code", &ClaudeCodeProvider::discover()?)?;
    println!("\nLocal data report complete.");
    Ok(())
}

fn report_provider(name: &str, provider: &impl Provider) -> Result<()> {
    let mut checkpoint = None;
    let mut records = HashMap::<RecordId, Record>::new();
    let mut diagnostics = BTreeMap::<String, usize>::new();
    let progress = scan_progress(name);
    let mut batch_count = 0_usize;
    let mut change_count = 0_usize;
    let mut diagnostic_count = 0_usize;

    loop {
        progress.set_message(format!(
            "{name}: scanning batch {} | {} records",
            batch_count + 1,
            records.len()
        ));

        let batch = match provider.scan(checkpoint.as_ref()) {
            Ok(batch) => batch,
            Err(error) => {
                progress.abandon_with_message(format!(
                    "{name}: scan failed after {batch_count} batches | {change_count} changes"
                ));
                progress.disable_steady_tick();
                return Err(error);
            }
        };
        batch_count += 1;
        change_count += batch.changes.len();
        diagnostic_count += batch.diagnostics.len();

        for change in &batch.changes {
            match change {
                Change::Upsert(record) => {
                    records.insert(record.id.clone(), (**record).clone());
                }
                Change::Delete(id) => {
                    records.remove(id);
                }
                Change::Reset(origin) | Change::Remove(origin) => {
                    records.retain(|_, record| {
                        record.origin.source != origin.source || record.origin.path != origin.path
                    });
                }
                _ => {}
            }
        }
        for diagnostic in &batch.diagnostics {
            *diagnostics.entry(diagnostic.code.clone()).or_default() += 1;
        }
        checkpoint = Some(batch.checkpoint);
        progress.set_message(format!(
            "{name}: scanned {batch_count} batches | {change_count} changes | {} records | {diagnostic_count} diagnostics",
            records.len()
        ));
        if !batch.has_more {
            break;
        }
    }
    progress.finish_with_message(format!(
        "{name}: scan complete | {batch_count} batches | {change_count} changes | {} records | {diagnostic_count} diagnostics",
        records.len()
    ));
    progress.disable_steady_tick();

    let mut record_types = BTreeMap::<String, usize>::new();
    let mut unknown_types = BTreeMap::<String, usize>::new();
    let mut additive_tokens = 0_i64;
    for record in records.values() {
        let kind = match &record.data {
            RecordData::Session(_) => "session",
            RecordData::Turn(_) => "turn",
            RecordData::Usage(usage) => {
                if let Some(delta) = &usage.delta {
                    additive_tokens = additive_tokens.saturating_add(delta.total);
                }
                "usage"
            }
            RecordData::RateLimit(_) => "rate_limit",
            RecordData::Unknown(unknown) => {
                *unknown_types
                    .entry(
                        unknown
                            .kind
                            .clone()
                            .unwrap_or_else(|| "<missing>".to_owned()),
                    )
                    .or_default() += 1;
                "unknown_record"
            }
            RecordData::Item(item) => match &item.data {
                ItemData::Message(_) => "message",
                ItemData::Reasoning(_) => "reasoning",
                ItemData::Plan(_) => "plan",
                ItemData::ToolCall(_) => "tool_call",
                ItemData::ToolResult(_) => "tool_result",
                ItemData::ApprovalRequest(_) => "approval_request",
                ItemData::ApprovalDecision(_) => "approval_decision",
                ItemData::ModelInvocation(_) => "model_invocation",
                ItemData::AgentInvocation(_) => "agent_invocation",
                ItemData::FileChange(_) => "file_change",
                ItemData::WorldState(_) => "world_state",
                ItemData::Goal(_) => "goal",
                ItemData::ForkTurnBoundary(_) => "fork_turn_boundary",
                ItemData::InputQueue(_) => "input_queue",
                ItemData::ContextCompaction(_) => "context_compaction",
                ItemData::ExecutionContext(_) => "execution_context",
                ItemData::ModeChange(_) => "mode_change",
                ItemData::Notice(_) => "notice",
                ItemData::HookResult(_) => "hook_result",
                ItemData::Retry(_) => "retry",
                ItemData::Rollback(_) => "rollback",
                ItemData::Unknown(unknown) => {
                    *unknown_types
                        .entry(
                            unknown
                                .kind
                                .clone()
                                .unwrap_or_else(|| "<missing>".to_owned()),
                        )
                        .or_default() += 1;
                    "unknown_item"
                }
                _ => "future_item",
            },
            _ => "future_record",
        };
        *record_types.entry(kind.to_owned()).or_default() += 1;
    }

    print_summary_table(
        name,
        batch_count,
        change_count,
        records.len(),
        diagnostic_count,
        additive_tokens,
    );
    print_count_table(
        &format!("{name} record types"),
        "Record type",
        &record_types,
    );
    print_count_table(
        &format!("{name} unknown types"),
        "Unknown type",
        &unknown_types,
    );
    print_count_table(
        &format!("{name} diagnostic codes"),
        "Diagnostic code",
        &diagnostics,
    );
    Ok(())
}

fn scan_progress(name: &str) -> ProgressBar {
    let progress = ProgressBar::new_spinner();
    progress.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg} [{elapsed_precise}]")
            .expect("valid progress template")
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    progress.enable_steady_tick(Duration::from_millis(120));
    progress.set_message(format!("{name}: starting scan"));
    progress
}

fn print_summary_table(
    name: &str,
    batch_count: usize,
    change_count: usize,
    record_count: usize,
    diagnostic_count: usize,
    additive_tokens: i64,
) {
    let rows = vec![
        ("Batches".to_owned(), batch_count.to_string()),
        ("Changes".to_owned(), change_count.to_string()),
        ("Current records".to_owned(), record_count.to_string()),
        ("Diagnostics".to_owned(), diagnostic_count.to_string()),
        ("Additive tokens".to_owned(), additive_tokens.to_string()),
    ];
    print_table(&format!("{name} summary"), "Metric", "Value", &rows);
}

fn print_count_table(title: &str, item_header: &str, counts: &BTreeMap<String, usize>) {
    let rows = if counts.is_empty() {
        vec![("<none>".to_owned(), "0".to_owned())]
    } else {
        counts
            .iter()
            .map(|(item, count)| (item.clone(), count.to_string()))
            .collect()
    };
    print_table(title, item_header, "Count", &rows);
}

fn print_table(title: &str, left_header: &str, right_header: &str, rows: &[(String, String)]) {
    print!("{}", format_table(title, left_header, right_header, rows));
}

fn format_table(
    title: &str,
    left_header: &str,
    right_header: &str,
    rows: &[(String, String)],
) -> String {
    let left_width = rows
        .iter()
        .map(|(left, _)| left.len())
        .chain(std::iter::once(left_header.len()))
        .max()
        .expect("table header provides a width");
    let right_width = rows
        .iter()
        .map(|(_, right)| right.len())
        .chain(std::iter::once(right_header.len()))
        .max()
        .expect("table header provides a width");
    let divider = format!(
        "+-{}-+-{}-+",
        "-".repeat(left_width),
        "-".repeat(right_width)
    );
    let mut table = String::new();

    writeln!(table, "\n{title}").expect("writing to a string cannot fail");
    writeln!(table, "{divider}").expect("writing to a string cannot fail");
    writeln!(
        table,
        "| {left_header:<left_width$} | {right_header:>right_width$} |"
    )
    .expect("writing to a string cannot fail");
    writeln!(table, "{divider}").expect("writing to a string cannot fail");
    for (left, right) in rows {
        writeln!(table, "| {left:<left_width$} | {right:>right_width$} |")
            .expect("writing to a string cannot fail");
    }
    writeln!(table, "{divider}").expect("writing to a string cannot fail");
    table
}

#[cfg(test)]
mod tests {
    use super::format_table;

    #[test]
    fn formats_aligned_count_table() {
        let rows = vec![
            ("message".to_owned(), "57267".to_owned()),
            ("tool_call".to_owned(), "121345".to_owned()),
        ];

        let table = format_table("codex record types", "Record type", "Count", &rows);

        assert_eq!(
            table,
            "\
\ncodex record types
+-------------+--------+
| Record type |  Count |
+-------------+--------+
| message     |  57267 |
| tool_call   | 121345 |
+-------------+--------+
"
        );
    }
}
