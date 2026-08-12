use std::path::PathBuf;

use coding_agent_data::{
    Actor, AdapterCoverage, AgentInvocation, AgentInvocationStatus, AgentOperation, Batch,
    CapabilityCoverage, Change, Checkpoint, ContentAnnotations, ContentAudience, ContentBlock,
    ContentIcon, ContentIconTheme, ContentPriority, Diagnostic, Error, Event, EventData,
    EventSequence, FileChangeKind, HistoryMode, HistoryPosition, HistorySegment, ModeChangeKind,
    ProviderId, ProviderInfo, QueueOperation, Reasoning, ReasoningVisibility, Record, RecordData,
    RecordId, Session, SessionHistory, SessionRelation, SessionRelationKind, SourceCoverage,
    SourceId, SourceLocation, SourceRef, StopReason, Timestamp, ToolKind, UnknownRecord,
    UsageReport,
};

fn provider_info() -> ProviderInfo {
    ProviderInfo {
        id: ProviderId::new("test-provider"),
        name: "Test provider",
        source: SourceId::new("test-provider:source"),
    }
}

fn checkpoint() -> Checkpoint {
    Checkpoint::from_state(&provider_info(), &serde_json::json!({})).unwrap()
}

#[test]
fn capability_coverage_rejects_contradictory_source_and_adapter_states() {
    assert!(CapabilityCoverage::new(
        "persisted",
        SourceCoverage::Persisted,
        AdapterCoverage::Normalized,
    )
    .is_consistent());
    assert!(CapabilityCoverage::new(
        "partial",
        SourceCoverage::PartiallyPersisted,
        AdapterCoverage::PartiallyNormalized,
    )
    .is_consistent());
    assert!(CapabilityCoverage::new(
        "not-persisted",
        SourceCoverage::NotPersisted,
        AdapterCoverage::NotApplicable,
    )
    .is_consistent());
    assert!(
        CapabilityCoverage::new("unknown", SourceCoverage::Unknown, AdapterCoverage::Unknown,)
            .is_consistent()
    );
    assert!(CapabilityCoverage::new(
        "adapter-not-audited",
        SourceCoverage::Persisted,
        AdapterCoverage::Unknown,
    )
    .is_consistent());

    assert!(!CapabilityCoverage::new(
        "missing-source",
        SourceCoverage::NotPersisted,
        AdapterCoverage::Normalized,
    )
    .is_consistent());
    assert!(!CapabilityCoverage::new(
        "unknown-source",
        SourceCoverage::Unknown,
        AdapterCoverage::RawOnly,
    )
    .is_consistent());
    assert!(!CapabilityCoverage::new(
        "missing-adapter",
        SourceCoverage::Persisted,
        AdapterCoverage::NotApplicable,
    )
    .is_consistent());
}

#[test]
fn batch_and_diagnostic_origin_survive_serialization() {
    let info = provider_info();
    let diagnostic = Diagnostic::warning("test.invalid_json", "line could not be decoded")
        .with_origin(SourceRef::json_line(
            info.source.clone(),
            "/tmp/session.jsonl",
            7,
            Some(120),
            Some(180),
        ));
    let batch = Batch::new(Vec::new(), checkpoint(), vec![diagnostic], false);

    batch.validate_for(&info).unwrap();
    let serialized = serde_json::to_string(&batch).unwrap();
    let serialized_value: serde_json::Value = serde_json::from_str(&serialized).unwrap();
    let batch_fields = serialized_value.as_object().unwrap();
    assert_eq!(batch_fields.len(), 4);
    assert!(["changes", "checkpoint", "diagnostics", "has_more"]
        .iter()
        .all(|field| batch_fields.contains_key(*field)));
    let checkpoint_fields = serialized_value["checkpoint"].as_object().unwrap();
    assert_eq!(checkpoint_fields.len(), 3);
    assert!(["provider", "source", "state"]
        .iter()
        .all(|field| checkpoint_fields.contains_key(*field)));

    let decoded: Batch = serde_json::from_str(&serialized).unwrap();
    decoded.validate_for(&info).unwrap();
    assert_eq!(
        decoded.diagnostics[0].origin,
        Some(SourceRef {
            source: info.source,
            path: PathBuf::from("/tmp/session.jsonl"),
            location: SourceLocation::JsonLine {
                line: 7,
                byte_start: Some(120),
                byte_end: Some(180),
            },
        })
    );
}

#[test]
fn custom_provider_checkpoint_state_round_trips_through_the_public_api() {
    let info = provider_info();
    let state = serde_json::json!({"cursor": 42, "path": "/tmp/session.jsonl"});
    let checkpoint = Checkpoint::from_state(&info, &state).unwrap();
    let encoded = checkpoint.to_json().unwrap();
    let decoded = Checkpoint::from_json(&encoded).unwrap();

    assert_eq!(
        decoded.decode_state::<serde_json::Value>(&info).unwrap(),
        state
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).unwrap()["state"],
        state
    );
}

#[test]
fn batch_validation_rejects_cross_source_records() {
    let info = provider_info();
    let other_source = SourceId::new("test-provider:other-source");
    let record = Record::new(
        RecordId::new("test-provider:other-source:unknown:1"),
        other_source.clone(),
        SourceRef::whole_file(other_source, "/tmp/other.jsonl"),
        RecordData::Unknown(UnknownRecord { kind: None }),
    );
    let batch = Batch::new(
        vec![Change::upsert(record)],
        checkpoint(),
        Vec::new(),
        false,
    );

    assert!(matches!(
        batch.validate_for(&info),
        Err(Error::InvalidBatch(message)) if message.contains("another source")
    ));
}

#[test]
fn batch_validation_rejects_every_cross_source_record_link() {
    let info = provider_info();
    let source = info.source.clone();
    let other = RecordId::new("test-provider:other-source:session:foreign");
    let origin = SourceRef::whole_file(source.clone(), "/tmp/session.jsonl");
    let mut changes = Vec::new();

    let mut record = Record::new(
        RecordId::scoped(&source, "unknown", "record-id"),
        source.clone(),
        origin.clone(),
        RecordData::Unknown(UnknownRecord { kind: None }),
    );
    record.id = other.clone();
    changes.push(Change::upsert(record));

    let mut record = Record::new(
        RecordId::scoped(&source, "unknown", "session-link"),
        source.clone(),
        origin.clone(),
        RecordData::Unknown(UnknownRecord { kind: None }),
    );
    record.session = Some(other.clone());
    changes.push(Change::upsert(record));

    let mut record = Record::new(
        RecordId::scoped(&source, "unknown", "invocation-link"),
        source.clone(),
        origin.clone(),
        RecordData::Unknown(UnknownRecord { kind: None }),
    );
    record.invocation = Some(other.clone());
    changes.push(Change::upsert(record));

    let mut session = Session::new("session-relation");
    session.relations.push(SessionRelation::new(
        SessionRelationKind::Fork,
        other.clone(),
    ));
    changes.push(Change::upsert(Record::new(
        RecordId::scoped(&source, "session", "relation"),
        source.clone(),
        origin.clone(),
        RecordData::Session(session),
    )));

    let mut session = Session::new("session-history");
    session.history = Some(SessionHistory {
        mode: HistoryMode::Paginated,
        base: Some(HistoryPosition {
            session: other.clone(),
            end_ordinal_exclusive: 2,
            end_byte_offset: 10,
        }),
        own_start_ordinal: Some(3),
        context_window_id: None,
        lineage: Vec::new(),
    });
    changes.push(Change::upsert(Record::new(
        RecordId::scoped(&source, "session", "history-base"),
        source.clone(),
        origin.clone(),
        RecordData::Session(session),
    )));

    let mut session = Session::new("session-lineage");
    session.history = Some(SessionHistory {
        mode: HistoryMode::Paginated,
        base: None,
        own_start_ordinal: Some(1),
        context_window_id: None,
        lineage: vec![HistorySegment {
            session: other.clone(),
            start_ordinal: 1,
            end_ordinal_exclusive: None,
        }],
    });
    changes.push(Change::upsert(Record::new(
        RecordId::scoped(&source, "session", "history-lineage"),
        source.clone(),
        origin.clone(),
        RecordData::Session(session),
    )));

    let mut event = Event::new(
        EventSequence::new(1, 0),
        Actor::Agent,
        EventData::Unknown(coding_agent_data::UnknownEvent { kind: None }),
    );
    event.parent = Some(other.clone());
    changes.push(Change::upsert(Record::new(
        RecordId::scoped(&source, "event", "parent"),
        source.clone(),
        origin.clone(),
        RecordData::Event(event),
    )));

    let mut event = Event::new(
        EventSequence::new(2, 0),
        Actor::Agent,
        EventData::Unknown(coding_agent_data::UnknownEvent { kind: None }),
    );
    event.inherited_from = Some(other.clone());
    changes.push(Change::upsert(Record::new(
        RecordId::scoped(&source, "event", "inherited"),
        source.clone(),
        origin.clone(),
        RecordData::Event(event),
    )));

    let event = Event::new(
        EventSequence::new(3, 0),
        Actor::Agent,
        EventData::AgentInvocation(AgentInvocation {
            invocation_id: "invocation-1".to_owned(),
            context_id: None,
            task_id: None,
            operation: AgentOperation::Spawn,
            sender_id: None,
            receiver_ids: Vec::new(),
            child_session: Some(other.clone()),
            status: AgentInvocationStatus::InProgress,
            started_at: None,
            completed_at: None,
            duration_ms: None,
            stop_reason: None,
            trace_id: None,
            model_context_window: None,
            time_to_first_token_ms: None,
            input: None,
            output: None,
            artifacts: Vec::new(),
            error: None,
        }),
    );
    changes.push(Change::upsert(Record::new(
        RecordId::scoped(&source, "event", "child-session"),
        source,
        origin,
        RecordData::Event(event),
    )));

    changes.push(Change::Delete(other));

    for change in changes {
        let batch = Batch::new(vec![change], checkpoint(), Vec::new(), false);
        assert!(matches!(
            batch.validate_for(&info),
            Err(Error::InvalidBatch(message)) if message.contains("another source")
        ));
    }
}

#[test]
fn acp_v2_content_metadata_survives_serialization() {
    let mut icon = ContentIcon::new("https://example.com/report.svg");
    icon.mime_type = Some("image/svg+xml".to_owned());
    icon.sizes = vec!["any".to_owned()];
    icon.theme = Some(ContentIconTheme::Dark);

    let mut annotations = ContentAnnotations::default();
    annotations.audience = vec![
        ContentAudience::User,
        ContentAudience::Other("_reviewer".to_owned()),
    ];
    annotations.last_modified = Some(Timestamp::from_millis(1_767_323_045_000));
    annotations.priority = ContentPriority::new(0.8);

    let block = ContentBlock::ResourceLink {
        uri: "file:///tmp/report.pdf".to_owned(),
        name: Some("report.pdf".to_owned()),
        title: Some("Report".to_owned()),
        description: None,
        mime_type: Some("application/pdf".to_owned()),
        size: Some(42),
        icons: vec![icon],
        annotations: Some(annotations),
    };

    let serialized = serde_json::to_string(&block).unwrap();
    let decoded: ContentBlock = serde_json::from_str(&serialized).unwrap();

    assert_eq!(decoded, block);
    assert!(serde_json::from_str::<ContentPriority>("1.1").is_err());
}

#[test]
fn unknown_content_survives_serialization() {
    let block = ContentBlock::Unknown {
        kind: None,
        value: serde_json::json!({"series": [1, 2, 3]}),
    };

    let serialized = serde_json::to_string(&block).unwrap();
    let decoded: ContentBlock = serde_json::from_str(&serialized).unwrap();

    assert_eq!(decoded, block);
}

#[test]
fn usage_report_record_data_uses_the_usage_report_variant_name() {
    let data = RecordData::UsageReport(UsageReport {
        model_provider: None,
        model: None,
        service_tier: None,
        request_id: None,
        invocation_id: None,
        cumulative: None,
        delta: None,
        cost: None,
    });

    let serialized = serde_json::to_value(&data).unwrap();
    assert_eq!(serialized["type"], "usage_report");

    let decoded: RecordData = serde_json::from_value(serialized).unwrap();
    assert!(matches!(decoded, RecordData::UsageReport(_)));
}

#[test]
fn extensible_execution_vocabulary_has_stable_serialized_names() {
    assert_eq!(
        serde_json::to_string(&StopReason::ContextWindowExceeded).unwrap(),
        r#""context_window_exceeded""#
    );
    assert_eq!(
        serde_json::to_string(&ToolKind::SwitchMode).unwrap(),
        r#""switch_mode""#
    );
    assert_eq!(
        serde_json::to_string(&FileChangeKind::Write).unwrap(),
        r#""write""#
    );
    assert_eq!(
        serde_json::to_string(&ModeChangeKind::Selected).unwrap(),
        r#""selected""#
    );

    let reasoning: Reasoning = serde_json::from_str(r#"{"summary":[],"content":[]}"#).unwrap();
    assert_eq!(reasoning.visibility, ReasoningVisibility::Visible);
}

#[test]
fn additive_history_lineage_and_queue_vocabulary_are_backward_compatible() {
    let history: SessionHistory = serde_json::from_str(
        r#"{"mode":"paginated","base":null,"own_start_ordinal":3,"context_window_id":null}"#,
    )
    .unwrap();
    assert_eq!(history.mode, HistoryMode::Paginated);
    assert!(history.lineage.is_empty());
    assert_eq!(
        serde_json::to_string(&QueueOperation::PopAll).unwrap(),
        r#""pop_all""#
    );
}
