use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::{Diagnostic, HistorySegment, ProviderInfo, SourceRef};

use super::normalize;
use super::replay::{OwnedHistoryBaseMetadata, RolloutMetadata};
use super::rollout::push_diagnostic;
use super::state_db::IndexSnapshot;

#[derive(Debug)]
pub(super) struct LineagePlan {
    entries: BTreeMap<PathBuf, LineageEntry>,
}

#[derive(Debug)]
struct LineageEntry {
    segments: Vec<HistorySegment>,
    own_start_ordinal: Option<u64>,
}

impl LineagePlan {
    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    pub(super) fn build(
        files: &[PathBuf],
        index: &IndexSnapshot,
        metadata_cache: &BTreeMap<PathBuf, Option<RolloutMetadata>>,
        info: &ProviderInfo,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Self {
        let mut metadata_by_path = BTreeMap::new();
        for path in files {
            if let Some(Some(metadata)) = metadata_cache.get(path) {
                metadata_by_path.insert(path.clone(), metadata.clone());
            }
        }

        let mut paths_by_session = BTreeMap::new();
        for (path, binding) in &index.transcripts {
            if metadata_by_path
                .get(path)
                .is_some_and(|metadata| metadata.session_id == binding.external_id)
            {
                paths_by_session.insert(binding.external_id.clone(), path.clone());
            }
        }
        for (path, metadata) in &metadata_by_path {
            paths_by_session
                .entry(metadata.session_id.clone())
                .or_insert_with(|| path.clone());
        }

        let mut entries = BTreeMap::new();
        for (path, metadata) in &metadata_by_path {
            let segments = if metadata.history_mode.as_deref() == Some("paginated") {
                resolve(
                    &metadata.session_id,
                    path,
                    &paths_by_session,
                    &metadata_by_path,
                    info,
                    diagnostics,
                )
                .unwrap_or_default()
            } else {
                Vec::new()
            };
            entries.insert(
                path.clone(),
                LineageEntry {
                    segments,
                    own_start_ordinal: metadata.own_start_ordinal,
                },
            );
        }
        Self { entries }
    }

    pub(super) fn segments(&self, path: &Path) -> Option<&[HistorySegment]> {
        self.entries
            .get(path)
            .map(|entry| entry.segments.as_slice())
    }

    pub(super) fn own_start_ordinal(&self, path: &Path) -> Option<u64> {
        self.entries
            .get(path)
            .and_then(|entry| entry.own_start_ordinal)
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve(
    requested_session: &str,
    requested_path: &Path,
    paths_by_session: &BTreeMap<String, PathBuf>,
    metadata_by_path: &BTreeMap<PathBuf, RolloutMetadata>,
    info: &ProviderInfo,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<HistorySegment>> {
    let mut reversed = Vec::new();
    let mut seen = BTreeSet::new();
    let mut session_id = requested_session;
    let mut path = requested_path;
    let mut end: Option<&OwnedHistoryBaseMetadata> = None;

    loop {
        if !seen.insert(session_id.to_owned()) {
            lineage_diagnostic(
                diagnostics,
                info,
                requested_path,
                "codex.lineage.cycle",
                format!(
                    "paginated history for {requested_session} contains a cycle at {session_id}"
                ),
            );
            return None;
        }
        let Some(metadata) = metadata_by_path.get(path) else {
            lineage_diagnostic(
                diagnostics,
                info,
                requested_path,
                "codex.lineage.metadata_missing",
                format!(
                    "paginated history for {requested_session} cannot read metadata for {session_id}"
                ),
            );
            return None;
        };
        if metadata.session_id != session_id {
            lineage_diagnostic(
                diagnostics,
                info,
                path,
                "codex.lineage.owner_mismatch",
                format!(
                    "rollout selected for {session_id} belongs to {}",
                    metadata.session_id
                ),
            );
            return None;
        }
        if metadata.history_mode.as_deref() != Some("paginated") {
            lineage_diagnostic(
                diagnostics,
                info,
                path,
                "codex.lineage.mode_mismatch",
                format!("lineage session {session_id} is not a paginated rollout"),
            );
            return None;
        }

        if let Some(cutoff) = end {
            if cutoff.end_ordinal_exclusive == 0 {
                lineage_diagnostic(
                    diagnostics,
                    info,
                    path,
                    "codex.lineage.invalid_cutoff",
                    format!("lineage cutoff for {session_id} cannot include session metadata"),
                );
                return None;
            }
            let file_len = match fs::metadata(path) {
                Ok(metadata) => metadata.len(),
                Err(error) => {
                    lineage_diagnostic(
                        diagnostics,
                        info,
                        path,
                        "codex.lineage.metadata_unavailable",
                        format!("cannot inspect lineage rollout for {session_id}: {error}"),
                    );
                    return None;
                }
            };
            if cutoff.end_byte_offset > file_len {
                lineage_diagnostic(
                    diagnostics,
                    info,
                    path,
                    "codex.lineage.byte_offset_out_of_bounds",
                    format!(
                        "lineage cutoff byte offset {} for {session_id} exceeds rollout length {file_len}",
                        cutoff.end_byte_offset
                    ),
                );
                return None;
            }
        }

        let start_ordinal = match metadata.history_base.as_ref() {
            Some(base) => match base.end_ordinal_exclusive.checked_add(1) {
                Some(value) => value,
                None => {
                    lineage_diagnostic(
                        diagnostics,
                        info,
                        path,
                        "codex.lineage.ordinal_overflow",
                        format!("lineage start ordinal overflows for {session_id}"),
                    );
                    return None;
                }
            },
            None => 1,
        };
        reversed.push(HistorySegment {
            session: normalize::session_id(info, session_id),
            start_ordinal,
            end_ordinal_exclusive: end.map(|cutoff| cutoff.end_ordinal_exclusive),
        });

        let Some(base) = metadata.history_base.as_ref() else {
            break;
        };
        let Some(parent_path) = paths_by_session.get(&base.thread_id) else {
            lineage_diagnostic(
                diagnostics,
                info,
                requested_path,
                "codex.lineage.parent_missing",
                format!(
                    "paginated history for {requested_session} references missing session {}",
                    base.thread_id
                ),
            );
            return None;
        };
        session_id = &base.thread_id;
        path = parent_path;
        end = Some(base);
    }

    reversed.reverse();
    Some(reversed)
}

fn lineage_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    info: &ProviderInfo,
    path: &Path,
    code: &'static str,
    message: String,
) {
    push_diagnostic(
        diagnostics,
        Diagnostic::warning(code, message)
            .with_origin(SourceRef::whole_file(info.source.clone(), path)),
    );
}
