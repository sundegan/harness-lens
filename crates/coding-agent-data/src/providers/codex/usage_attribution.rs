use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Timestamp, TokenUsage};

use super::checkpoint::{CodexCheckpoint, RolloutContext};

/// Stable, compact identity for one additive Codex usage observation.
///
/// The identity intentionally excludes the session and source location. Codex
/// can copy the same model request into multiple session histories, while the
/// request must contribute to aggregate usage only once.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(transparent)]
pub(super) struct UsageFingerprint(String);

pub(super) struct UsageAttributionPlan {
    owners: BTreeMap<UsageFingerprint, PathBuf>,
    pending_paths: BTreeSet<PathBuf>,
    rebuilds: BTreeSet<PathBuf>,
}

impl UsageAttributionPlan {
    pub(super) fn build(state: &CodexCheckpoint) -> Self {
        let pending_paths = state
            .pending_usage_rebuilds
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut owners = BTreeMap::new();

        for (path, rollout) in &state.rollouts {
            for fingerprint in rollout.context().usage_attribution.keys() {
                select_owner(&mut owners, fingerprint, path);
            }
        }
        for (path, fingerprints) in &state.pending_usage_rebuilds {
            for fingerprint in fingerprints {
                select_owner(&mut owners, fingerprint, path);
            }
        }

        let mut rebuilds = BTreeSet::new();
        for (path, rollout) in &state.rollouts {
            if pending_paths.contains(path) {
                continue;
            }
            for (fingerprint, attributed) in &rollout.context().usage_attribution {
                let should_be_attributed = owners.get(fingerprint) == Some(path);
                if *attributed != should_be_attributed {
                    rebuilds.insert(path.clone());
                    break;
                }
            }
        }

        Self {
            owners,
            pending_paths,
            rebuilds,
        }
    }

    pub(super) fn attribute(
        &mut self,
        path: &Path,
        context: &mut RolloutContext,
        timestamp: Option<Timestamp>,
        model: Option<&str>,
        delta: Option<TokenUsage>,
    ) -> Option<TokenUsage> {
        let delta = delta?;
        let Some(fingerprint) = usage_fingerprint(timestamp, model, &delta) else {
            return Some(delta);
        };

        // A later copy inside the same rollout is never a second request.
        if context.usage_attribution.contains_key(&fingerprint) {
            return None;
        }

        let attributed = match self.owners.get_mut(&fingerprint) {
            None => {
                self.owners.insert(fingerprint.clone(), path.to_path_buf());
                true
            }
            Some(owner) if path < owner.as_path() => {
                if !self.pending_paths.contains(owner.as_path()) {
                    self.rebuilds.insert(owner.clone());
                }
                *owner = path.to_path_buf();
                true
            }
            Some(owner) => owner.as_path() == path,
        };
        context.usage_attribution.insert(fingerprint, attributed);
        attributed.then_some(delta)
    }

    pub(super) fn take_rebuilds(&mut self) -> BTreeSet<PathBuf> {
        std::mem::take(&mut self.rebuilds)
    }
}

fn select_owner(
    owners: &mut BTreeMap<UsageFingerprint, PathBuf>,
    fingerprint: &UsageFingerprint,
    candidate: &Path,
) {
    match owners.get_mut(fingerprint) {
        Some(owner) if candidate < owner.as_path() => *owner = candidate.to_path_buf(),
        Some(_) => {}
        None => {
            owners.insert(fingerprint.clone(), candidate.to_path_buf());
        }
    }
}

fn usage_fingerprint(
    timestamp: Option<Timestamp>,
    model: Option<&str>,
    usage: &TokenUsage,
) -> Option<UsageFingerprint> {
    let timestamp = timestamp?;
    let mut hasher = Sha256::new();
    hasher.update(b"codex-usage-attribution");
    hasher.update(timestamp.as_millis().to_be_bytes());
    update_optional_text(&mut hasher, model);
    update_i64(&mut hasher, usage.total);
    update_optional_i64(&mut hasher, usage.input);
    update_optional_i64(&mut hasher, usage.cache_creation_input);
    update_optional_i64(&mut hasher, usage.cache_creation_ephemeral_5m_input);
    update_optional_i64(&mut hasher, usage.cache_creation_ephemeral_1h_input);
    update_optional_i64(&mut hasher, usage.cached_input);
    update_optional_i64(&mut hasher, usage.output);
    update_optional_i64(&mut hasher, usage.reasoning_output);

    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(32);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in &digest[..16] {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Some(UsageFingerprint(encoded))
}

fn update_i64(hasher: &mut Sha256, value: i64) {
    hasher.update(value.to_be_bytes());
}

fn update_optional_i64(hasher: &mut Sha256, value: Option<i64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            update_i64(hasher, value);
        }
        None => hasher.update([0]),
    }
}

fn update_optional_text(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.len().to_be_bytes());
            hasher.update(value.as_bytes());
        }
        None => hasher.update([0]),
    }
}
