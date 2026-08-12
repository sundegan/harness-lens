use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{TokenUsage, UsageReport};

const SEEN_CUMULATIVE_LIMIT: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(transparent)]
struct UsageSnapshotFingerprint(String);

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct UsageAccountingState {
    /// Additive usage accepted so far in this rollout, including any leading
    /// history that replay filtering will later remove from emitted deltas.
    #[serde(default)]
    counted: Option<TokenUsage>,
    /// Most recent provider-reported cumulative snapshot.
    #[serde(default)]
    raw_baseline: Option<TokenUsage>,
    /// Component-wise maximum cumulative snapshot observed in this rollout.
    #[serde(default)]
    watermark: Option<TokenUsage>,
    /// Once a cumulative component drops, subsequent observations are bounded
    /// by the watermark so alternating lineages cannot recount their gap.
    #[serde(default)]
    saw_cumulative_drop: bool,
    /// Whether accepted request deltas no longer equal the latest raw
    /// cumulative snapshot.
    #[serde(default)]
    saw_divergent_totals: bool,
    /// Recent raw cumulative snapshots used to suppress exact re-emissions.
    ///
    /// Containment remains the load-bearing overcount guard after older entries
    /// leave this bounded precision cache.
    #[serde(default)]
    seen_cumulative: Vec<UsageSnapshotFingerprint>,
}

pub(super) fn usage_report(
    payload: &Value,
    state: &mut UsageAccountingState,
) -> Option<UsageReport> {
    let info = payload.get("info")?;
    let cumulative = info.get("total_token_usage").and_then(normalize_usage);
    let reported_delta = info.get("last_token_usage").and_then(normalize_usage);
    let delta = account(cumulative.as_ref(), reported_delta.as_ref(), state);
    (cumulative.is_some() || reported_delta.is_some()).then_some(UsageReport {
        model_provider: None,
        model: None,
        service_tier: None,
        request_id: None,
        invocation_id: None,
        cumulative,
        delta,
        cost: None,
    })
}

fn normalize_usage(value: &Value) -> Option<TokenUsage> {
    let input = first_non_negative(value, &["input_tokens", "prompt_tokens", "input"]);
    let cache_creation_input = first_non_negative(
        value,
        &["cache_write_input_tokens", "cache_creation_input_tokens"],
    );
    let cached_input = first_non_negative(
        value,
        &[
            "cached_input_tokens",
            "cache_read_input_tokens",
            "cached_tokens",
        ],
    );
    let output = first_non_negative(value, &["output_tokens", "completion_tokens", "output"]);
    let reasoning_output =
        first_non_negative(value, &["reasoning_output_tokens", "reasoning_tokens"]);
    let recorded_total = first_non_negative(value, &["total_tokens"]);
    if recorded_total.is_none()
        && input.is_none()
        && cache_creation_input.is_none()
        && cached_input.is_none()
        && output.is_none()
        && reasoning_output.is_none()
    {
        return None;
    }
    // Codex reports reasoning as a subset of output. Historical records can
    // omit `total_tokens` or write zero even when components are present.
    let derived_total = input
        .unwrap_or_default()
        .saturating_add(output.unwrap_or_default());
    Some(TokenUsage {
        total: recorded_total
            .filter(|total| *total > 0)
            .unwrap_or(derived_total),
        input,
        cache_creation_input,
        cache_creation_ephemeral_5m_input: None,
        cache_creation_ephemeral_1h_input: None,
        cached_input,
        output,
        reasoning_output,
    })
}

fn first_non_negative(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_i64))
        .filter(|value| *value >= 0)
}

fn account(
    cumulative: Option<&TokenUsage>,
    reported_delta: Option<&TokenUsage>,
    state: &mut UsageAccountingState,
) -> Option<TokenUsage> {
    let Some(cumulative) = cumulative else {
        let delta = reported_delta.filter(|usage| has_additive_components(usage))?;
        let counted = add_usage(state.counted.as_ref(), delta);
        state.counted = Some(counted.clone());
        state.raw_baseline = Some(counted.clone());
        state.watermark = Some(max_usage(state.watermark.as_ref(), &counted));
        return Some(delta.clone());
    };

    let fingerprint = usage_snapshot_fingerprint(cumulative);
    if state
        .seen_cumulative
        .iter()
        .any(|seen| seen == &fingerprint)
    {
        return None;
    }

    let watermark = state
        .watermark
        .clone()
        .or_else(|| state.raw_baseline.clone());
    if watermark
        .as_ref()
        .is_some_and(|watermark| usage_dropped_below(cumulative, watermark))
    {
        state.saw_cumulative_drop = true;
    }

    // Codex can replace `last_token_usage` with an estimated context size
    // whose only non-zero field is `total_tokens` during rollback,
    // compaction, or "context full" handling. It is evidence, but it is not
    // request usage and must never become an additive delta.
    let additive_reported = reported_delta.filter(|usage| has_additive_components(usage));
    let estimated_or_empty_last = reported_delta.is_some() && additive_reported.is_none();

    let candidate = if estimated_or_empty_last {
        None
    } else if state.saw_cumulative_drop {
        let contained = contained_usage(watermark.as_ref(), state.counted.as_ref(), cumulative);
        additive_reported
            .map(|reported| min_usage(reported, &contained))
            .or(Some(contained))
    } else if let Some(reported) = additive_reported {
        Some(match state.raw_baseline.as_ref() {
            Some(raw_baseline) => {
                let cumulative_delta = subtract_usage(cumulative, Some(raw_baseline));
                if should_prefer_cumulative_delta(
                    raw_baseline,
                    cumulative,
                    &cumulative_delta,
                    reported,
                    state.saw_divergent_totals,
                ) {
                    cumulative_delta
                } else {
                    reported.clone()
                }
            }
            None => reported.clone(),
        })
    } else if state.saw_divergent_totals {
        Some(divergent_usage(
            watermark.as_ref(),
            state.counted.as_ref(),
            cumulative,
        ))
    } else {
        Some(subtract_usage(cumulative, state.raw_baseline.as_ref()))
    };

    state.raw_baseline = Some(cumulative.clone());
    state.watermark = Some(max_usage(state.watermark.as_ref(), cumulative));
    remember_cumulative(state, fingerprint);

    let delta = candidate.filter(|delta| {
        delta.total > 0
            && (has_additive_components(delta)
                || additive_reported.is_some()
                || has_additive_components(cumulative))
    });
    if let Some(delta) = delta {
        let counted = add_usage(state.counted.as_ref(), &delta);
        state.saw_divergent_totals |= !core_usage_equal(cumulative, Some(&counted));
        state.counted = Some(counted);
        Some(delta)
    } else {
        state.saw_divergent_totals |= !core_usage_equal(cumulative, state.counted.as_ref());
        None
    }
}

fn remember_cumulative(state: &mut UsageAccountingState, fingerprint: UsageSnapshotFingerprint) {
    state.seen_cumulative.push(fingerprint);
    let excess = state
        .seen_cumulative
        .len()
        .saturating_sub(SEEN_CUMULATIVE_LIMIT);
    if excess > 0 {
        state.seen_cumulative.drain(..excess);
    }
}

fn usage_snapshot_fingerprint(usage: &TokenUsage) -> UsageSnapshotFingerprint {
    let mut hasher = Sha256::new();
    hasher.update(b"codex-cumulative-usage");
    update_i64(&mut hasher, usage.total);
    update_i64(&mut hasher, usage.input.unwrap_or_default());
    update_i64(&mut hasher, usage.cache_creation_input.unwrap_or_default());
    update_i64(
        &mut hasher,
        usage.cache_creation_ephemeral_5m_input.unwrap_or_default(),
    );
    update_i64(
        &mut hasher,
        usage.cache_creation_ephemeral_1h_input.unwrap_or_default(),
    );
    update_i64(&mut hasher, usage.cached_input.unwrap_or_default());
    update_i64(&mut hasher, usage.output.unwrap_or_default());

    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(32);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in &digest[..16] {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    UsageSnapshotFingerprint(encoded)
}

fn update_i64(hasher: &mut Sha256, value: i64) {
    hasher.update(value.to_be_bytes());
}

fn has_additive_components(usage: &TokenUsage) -> bool {
    [
        usage.input,
        usage.cache_creation_input,
        usage.cache_creation_ephemeral_5m_input,
        usage.cache_creation_ephemeral_1h_input,
        usage.cached_input,
        usage.output,
        usage.reasoning_output,
    ]
    .into_iter()
    .flatten()
    .any(|value| value > 0)
}

fn usage_dropped_below(current: &TokenUsage, watermark: &TokenUsage) -> bool {
    current.total < watermark.total
        || optional_is_below(current.input, watermark.input)
        || optional_is_below(current.cache_creation_input, watermark.cache_creation_input)
        || optional_is_below(
            current.cache_creation_ephemeral_5m_input,
            watermark.cache_creation_ephemeral_5m_input,
        )
        || optional_is_below(
            current.cache_creation_ephemeral_1h_input,
            watermark.cache_creation_ephemeral_1h_input,
        )
        || optional_is_below(current.cached_input, watermark.cached_input)
        || optional_is_below(current.output, watermark.output)
        || optional_is_below(current.reasoning_output, watermark.reasoning_output)
}

fn should_prefer_cumulative_delta(
    raw_baseline: &TokenUsage,
    cumulative: &TokenUsage,
    cumulative_delta: &TokenUsage,
    reported_delta: &TokenUsage,
    saw_divergent_totals: bool,
) -> bool {
    !saw_divergent_totals
        && core_usage_at_least(cumulative, raw_baseline)
        && core_usage_at_most(cumulative_delta, reported_delta)
}

fn core_usage_equal(left: &TokenUsage, right: Option<&TokenUsage>) -> bool {
    core_usage_components(left) == right.map_or([0; 6], core_usage_components)
}

fn core_usage_at_least(left: &TokenUsage, right: &TokenUsage) -> bool {
    core_usage_components(left)
        .into_iter()
        .zip(core_usage_components(right))
        .all(|(left, right)| left >= right)
}

fn core_usage_at_most(left: &TokenUsage, right: &TokenUsage) -> bool {
    core_usage_components(left)
        .into_iter()
        .zip(core_usage_components(right))
        .all(|(left, right)| left <= right)
}

fn core_usage_components(usage: &TokenUsage) -> [i64; 6] {
    [
        usage.input.unwrap_or_default(),
        usage.cache_creation_input.unwrap_or_default(),
        usage.cache_creation_ephemeral_5m_input.unwrap_or_default(),
        usage.cache_creation_ephemeral_1h_input.unwrap_or_default(),
        usage.cached_input.unwrap_or_default(),
        usage.output.unwrap_or_default(),
    ]
}

fn optional_is_below(current: Option<i64>, watermark: Option<i64>) -> bool {
    matches!((current, watermark), (Some(current), Some(watermark)) if current < watermark)
}

fn divergent_usage(
    raw_baseline: Option<&TokenUsage>,
    counted: Option<&TokenUsage>,
    current: &TokenUsage,
) -> TokenUsage {
    TokenUsage {
        total: divergent_component(
            raw_baseline.map_or(0, |usage| usage.total),
            counted.map_or(0, |usage| usage.total),
            current.total,
        ),
        input: divergent_optional(
            raw_baseline.and_then(|usage| usage.input),
            counted.and_then(|usage| usage.input),
            current.input,
        ),
        cache_creation_input: divergent_optional(
            raw_baseline.and_then(|usage| usage.cache_creation_input),
            counted.and_then(|usage| usage.cache_creation_input),
            current.cache_creation_input,
        ),
        cache_creation_ephemeral_5m_input: divergent_optional(
            raw_baseline.and_then(|usage| usage.cache_creation_ephemeral_5m_input),
            counted.and_then(|usage| usage.cache_creation_ephemeral_5m_input),
            current.cache_creation_ephemeral_5m_input,
        ),
        cache_creation_ephemeral_1h_input: divergent_optional(
            raw_baseline.and_then(|usage| usage.cache_creation_ephemeral_1h_input),
            counted.and_then(|usage| usage.cache_creation_ephemeral_1h_input),
            current.cache_creation_ephemeral_1h_input,
        ),
        cached_input: divergent_optional(
            raw_baseline.and_then(|usage| usage.cached_input),
            counted.and_then(|usage| usage.cached_input),
            current.cached_input,
        ),
        output: divergent_optional(
            raw_baseline.and_then(|usage| usage.output),
            counted.and_then(|usage| usage.output),
            current.output,
        ),
        reasoning_output: divergent_optional(
            raw_baseline.and_then(|usage| usage.reasoning_output),
            counted.and_then(|usage| usage.reasoning_output),
            current.reasoning_output,
        ),
    }
}

fn divergent_component(raw: i64, counted: i64, current: i64) -> i64 {
    if current >= raw {
        current.saturating_sub(raw).max(0)
    } else {
        current.saturating_sub(counted).max(0)
    }
}

fn divergent_optional(raw: Option<i64>, counted: Option<i64>, current: Option<i64>) -> Option<i64> {
    raw.zip(counted)
        .zip(current)
        .map(|((raw, counted), current)| divergent_component(raw, counted, current))
}

fn contained_usage(
    watermark: Option<&TokenUsage>,
    counted: Option<&TokenUsage>,
    current: &TokenUsage,
) -> TokenUsage {
    TokenUsage {
        total: contained_component(
            watermark.map_or(0, |usage| usage.total),
            counted.map_or(0, |usage| usage.total),
            current.total,
        ),
        input: contained_optional(
            watermark.and_then(|usage| usage.input),
            counted.and_then(|usage| usage.input),
            current.input,
        ),
        cache_creation_input: contained_optional(
            watermark.and_then(|usage| usage.cache_creation_input),
            counted.and_then(|usage| usage.cache_creation_input),
            current.cache_creation_input,
        ),
        cache_creation_ephemeral_5m_input: contained_optional(
            watermark.and_then(|usage| usage.cache_creation_ephemeral_5m_input),
            counted.and_then(|usage| usage.cache_creation_ephemeral_5m_input),
            current.cache_creation_ephemeral_5m_input,
        ),
        cache_creation_ephemeral_1h_input: contained_optional(
            watermark.and_then(|usage| usage.cache_creation_ephemeral_1h_input),
            counted.and_then(|usage| usage.cache_creation_ephemeral_1h_input),
            current.cache_creation_ephemeral_1h_input,
        ),
        cached_input: contained_optional(
            watermark.and_then(|usage| usage.cached_input),
            counted.and_then(|usage| usage.cached_input),
            current.cached_input,
        ),
        output: contained_optional(
            watermark.and_then(|usage| usage.output),
            counted.and_then(|usage| usage.output),
            current.output,
        ),
        reasoning_output: contained_optional(
            watermark.and_then(|usage| usage.reasoning_output),
            counted.and_then(|usage| usage.reasoning_output),
            current.reasoning_output,
        ),
    }
}

fn contained_component(watermark: i64, counted: i64, current: i64) -> i64 {
    if current >= watermark {
        current.saturating_sub(watermark.max(counted)).max(0)
    } else {
        current.saturating_sub(counted).max(0)
    }
}

fn contained_optional(
    watermark: Option<i64>,
    counted: Option<i64>,
    current: Option<i64>,
) -> Option<i64> {
    watermark
        .zip(counted)
        .zip(current)
        .map(|((watermark, counted), current)| contained_component(watermark, counted, current))
}

fn subtract_usage(current: &TokenUsage, previous: Option<&TokenUsage>) -> TokenUsage {
    let has_baseline = previous.is_some();
    TokenUsage {
        total: current
            .total
            .saturating_sub(previous.map_or(0, |usage| usage.total))
            .max(0),
        input: subtract_optional(
            current.input,
            previous.and_then(|usage| usage.input),
            has_baseline,
        ),
        cache_creation_input: subtract_optional(
            current.cache_creation_input,
            previous.and_then(|usage| usage.cache_creation_input),
            has_baseline,
        ),
        cache_creation_ephemeral_5m_input: subtract_optional(
            current.cache_creation_ephemeral_5m_input,
            previous.and_then(|usage| usage.cache_creation_ephemeral_5m_input),
            has_baseline,
        ),
        cache_creation_ephemeral_1h_input: subtract_optional(
            current.cache_creation_ephemeral_1h_input,
            previous.and_then(|usage| usage.cache_creation_ephemeral_1h_input),
            has_baseline,
        ),
        cached_input: subtract_optional(
            current.cached_input,
            previous.and_then(|usage| usage.cached_input),
            has_baseline,
        ),
        output: subtract_optional(
            current.output,
            previous.and_then(|usage| usage.output),
            has_baseline,
        ),
        reasoning_output: subtract_optional(
            current.reasoning_output,
            previous.and_then(|usage| usage.reasoning_output),
            has_baseline,
        ),
    }
}

fn subtract_optional(
    current: Option<i64>,
    previous: Option<i64>,
    has_baseline: bool,
) -> Option<i64> {
    if !has_baseline {
        return current.map(|current| current.max(0));
    }
    current
        .zip(previous)
        .map(|(current, previous)| current.saturating_sub(previous).max(0))
}

fn min_usage(left: &TokenUsage, right: &TokenUsage) -> TokenUsage {
    TokenUsage {
        total: left.total.min(right.total).max(0),
        input: min_optional(left.input, right.input),
        cache_creation_input: min_optional(left.cache_creation_input, right.cache_creation_input),
        cache_creation_ephemeral_5m_input: min_optional(
            left.cache_creation_ephemeral_5m_input,
            right.cache_creation_ephemeral_5m_input,
        ),
        cache_creation_ephemeral_1h_input: min_optional(
            left.cache_creation_ephemeral_1h_input,
            right.cache_creation_ephemeral_1h_input,
        ),
        cached_input: min_optional(left.cached_input, right.cached_input),
        output: min_optional(left.output, right.output),
        reasoning_output: min_optional(left.reasoning_output, right.reasoning_output),
    }
}

fn min_optional(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    left.zip(right).map(|(left, right)| left.min(right).max(0))
}

fn add_usage(left: Option<&TokenUsage>, right: &TokenUsage) -> TokenUsage {
    TokenUsage {
        total: left
            .map_or(0, |usage| usage.total)
            .saturating_add(right.total),
        input: add_optional_usage(left, right.input, |usage| usage.input),
        cache_creation_input: add_optional_usage(left, right.cache_creation_input, |usage| {
            usage.cache_creation_input
        }),
        cache_creation_ephemeral_5m_input: add_optional_usage(
            left,
            right.cache_creation_ephemeral_5m_input,
            |usage| usage.cache_creation_ephemeral_5m_input,
        ),
        cache_creation_ephemeral_1h_input: add_optional_usage(
            left,
            right.cache_creation_ephemeral_1h_input,
            |usage| usage.cache_creation_ephemeral_1h_input,
        ),
        cached_input: add_optional_usage(left, right.cached_input, |usage| usage.cached_input),
        output: add_optional_usage(left, right.output, |usage| usage.output),
        reasoning_output: add_optional_usage(left, right.reasoning_output, |usage| {
            usage.reasoning_output
        }),
    }
}

fn add_optional_usage(
    left: Option<&TokenUsage>,
    right: Option<i64>,
    component: impl FnOnce(&TokenUsage) -> Option<i64>,
) -> Option<i64> {
    match left {
        Some(left) => component(left)
            .zip(right)
            .map(|(left, right)| left.saturating_add(right)),
        None => right,
    }
}

fn max_usage(left: Option<&TokenUsage>, right: &TokenUsage) -> TokenUsage {
    TokenUsage {
        total: left.map_or(right.total, |usage| usage.total.max(right.total)),
        input: max_optional(left.and_then(|usage| usage.input), right.input),
        cache_creation_input: max_optional(
            left.and_then(|usage| usage.cache_creation_input),
            right.cache_creation_input,
        ),
        cache_creation_ephemeral_5m_input: max_optional(
            left.and_then(|usage| usage.cache_creation_ephemeral_5m_input),
            right.cache_creation_ephemeral_5m_input,
        ),
        cache_creation_ephemeral_1h_input: max_optional(
            left.and_then(|usage| usage.cache_creation_ephemeral_1h_input),
            right.cache_creation_ephemeral_1h_input,
        ),
        cached_input: max_optional(
            left.and_then(|usage| usage.cached_input),
            right.cached_input,
        ),
        output: max_optional(left.and_then(|usage| usage.output), right.output),
        reasoning_output: max_optional(
            left.and_then(|usage| usage.reasoning_output),
            right.reasoning_output,
        ),
    }
}

fn max_optional(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}
