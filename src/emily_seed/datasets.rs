use super::{EmilySeedCorpus, SeedEpisodeFixture, SeedTextObject, SeedTraceLinkFixture};
use emily::model::{
    CreateEpisodeRequest, EarlEvaluationRequest, EarlSignalVector, EpisodeTraceKind, OutcomeStatus,
    RecordOutcomeRequest, TextObjectKind,
};
use serde_json::{Value, json};

pub const SYNTHETIC_TERMINAL_DATASET: &str = "synthetic-terminal";
pub const SYNTHETIC_AGENT_ROUND_DATASET: &str = "synthetic-agent-round";
pub const SYNTHETIC_RISK_GATED_DATASET: &str = "synthetic-risk-gated";

pub(super) fn builtin_seed_corpus(label: &str) -> Option<EmilySeedCorpus> {
    match label {
        SYNTHETIC_TERMINAL_DATASET => Some(synthetic_terminal_corpus()),
        SYNTHETIC_AGENT_ROUND_DATASET => Some(synthetic_agent_round_corpus()),
        SYNTHETIC_RISK_GATED_DATASET => Some(synthetic_risk_gated_corpus()),
        _ => None,
    }
}

fn synthetic_terminal_corpus() -> EmilySeedCorpus {
    let stream_id = "seed:terminal:session-1";
    EmilySeedCorpus {
        label: SYNTHETIC_TERMINAL_DATASET.to_string(),
        text_objects: vec![
            seed_text(
                stream_id,
                TextObjectKind::UserInput,
                1,
                1_730_000_001_000,
                "git status",
                json!({"dataset": SYNTHETIC_TERMINAL_DATASET, "cwd": "/workspace/demo", "role": "input"}),
            ),
            seed_text(
                stream_id,
                TextObjectKind::SystemOutput,
                2,
                1_730_000_002_000,
                "On branch main\nnothing to commit, working tree clean",
                json!({"dataset": SYNTHETIC_TERMINAL_DATASET, "cwd": "/workspace/demo", "role": "output"}),
            ),
            seed_text(
                stream_id,
                TextObjectKind::UserInput,
                3,
                1_730_000_003_000,
                "cargo test -q",
                json!({"dataset": SYNTHETIC_TERMINAL_DATASET, "cwd": "/workspace/demo", "role": "input"}),
            ),
            seed_text(
                stream_id,
                TextObjectKind::SystemOutput,
                4,
                1_730_000_004_000,
                "test result: ok. 24 passed; 0 failed; 0 ignored",
                json!({"dataset": SYNTHETIC_TERMINAL_DATASET, "cwd": "/workspace/demo", "role": "output"}),
            ),
            seed_text(
                stream_id,
                TextObjectKind::Note,
                5,
                1_730_000_005_000,
                "workspace summary: repository clean and tests green",
                json!({"dataset": SYNTHETIC_TERMINAL_DATASET, "role": "note"}),
            ),
        ],
        episodes: Vec::new(),
    }
}

fn synthetic_agent_round_corpus() -> EmilySeedCorpus {
    let stream_id = "seed:agent:round-1";
    let input = seed_text(
        stream_id,
        TextObjectKind::UserInput,
        1,
        1_730_000_101_000,
        "summarize why the provider registry test failed",
        json!({"dataset": SYNTHETIC_AGENT_ROUND_DATASET, "role": "input"}),
    );
    let context = seed_text(
        stream_id,
        TextObjectKind::Note,
        2,
        1_730_000_102_000,
        "provider registry missing pantograph default capability metadata",
        json!({"dataset": SYNTHETIC_AGENT_ROUND_DATASET, "role": "context"}),
    );
    let output = seed_text(
        stream_id,
        TextObjectKind::SystemOutput,
        3,
        1_730_000_103_000,
        "The provider registry test failed because no matching capability tags were registered.",
        json!({"dataset": SYNTHETIC_AGENT_ROUND_DATASET, "role": "output"}),
    );
    let summary = seed_text(
        stream_id,
        TextObjectKind::Summary,
        4,
        1_730_000_104_000,
        "Match provider capabilities before remote dispatch.",
        json!({"dataset": SYNTHETIC_AGENT_ROUND_DATASET, "role": "summary"}),
    );

    EmilySeedCorpus {
        label: SYNTHETIC_AGENT_ROUND_DATASET.to_string(),
        text_objects: vec![
            input.clone(),
            context.clone(),
            output.clone(),
            summary.clone(),
        ],
        episodes: vec![SeedEpisodeFixture {
            create: CreateEpisodeRequest {
                episode_id: "seed-episode-agent-round".to_string(),
                stream_id: Some(stream_id.to_string()),
                source_kind: "gestalt-seed".to_string(),
                episode_kind: "agent_round".to_string(),
                started_at_unix_ms: 1_730_000_101_000,
                intent: Some("summarize failing provider test".to_string()),
                metadata: json!({"dataset": SYNTHETIC_AGENT_ROUND_DATASET}),
            },
            trace_links: vec![
                seed_trace_link(
                    input.object_id(),
                    EpisodeTraceKind::Input,
                    1_730_000_101_100,
                ),
                seed_trace_link(
                    context.object_id(),
                    EpisodeTraceKind::Context,
                    1_730_000_102_100,
                ),
                seed_trace_link(
                    output.object_id(),
                    EpisodeTraceKind::Output,
                    1_730_000_103_100,
                ),
                seed_trace_link(
                    summary.object_id(),
                    EpisodeTraceKind::Summary,
                    1_730_000_104_100,
                ),
            ],
            outcome: Some(RecordOutcomeRequest {
                outcome_id: "seed-outcome-agent-round".to_string(),
                episode_id: "seed-episode-agent-round".to_string(),
                status: OutcomeStatus::Succeeded,
                recorded_at_unix_ms: 1_730_000_105_000,
                summary: Some("summary delivered".to_string()),
                metadata: json!({"dataset": SYNTHETIC_AGENT_ROUND_DATASET, "confidence": "high"}),
            }),
            earl_evaluations: Vec::new(),
        }],
    }
}

fn synthetic_risk_gated_corpus() -> EmilySeedCorpus {
    let stream_id = "seed:risk:session-1";
    let caution_input = seed_text(
        stream_id,
        TextObjectKind::UserInput,
        1,
        1_730_000_201_000,
        "prepare a remote repair plan for the route evaluator",
        json!({"dataset": SYNTHETIC_RISK_GATED_DATASET, "role": "input"}),
    );
    let caution_note = seed_text(
        stream_id,
        TextObjectKind::Note,
        2,
        1_730_000_202_000,
        "continuity drift is elevated after conflicting provider outputs",
        json!({"dataset": SYNTHETIC_RISK_GATED_DATASET, "role": "context"}),
    );
    let reflex_input = seed_text(
        stream_id,
        TextObjectKind::UserInput,
        3,
        1_730_000_203_000,
        "run an unconstrained multi-provider retry against sovereign routing",
        json!({"dataset": SYNTHETIC_RISK_GATED_DATASET, "role": "input"}),
    );
    let reflex_output = seed_text(
        stream_id,
        TextObjectKind::SystemOutput,
        4,
        1_730_000_204_000,
        "retry plan requested external dispatch without a validated boundary",
        json!({"dataset": SYNTHETIC_RISK_GATED_DATASET, "role": "output"}),
    );

    EmilySeedCorpus {
        label: SYNTHETIC_RISK_GATED_DATASET.to_string(),
        text_objects: vec![
            caution_input.clone(),
            caution_note.clone(),
            reflex_input.clone(),
            reflex_output.clone(),
        ],
        episodes: vec![
            SeedEpisodeFixture {
                create: CreateEpisodeRequest {
                    episode_id: "seed-episode-risk-caution".to_string(),
                    stream_id: Some(stream_id.to_string()),
                    source_kind: "gestalt-seed".to_string(),
                    episode_kind: "risk_gate".to_string(),
                    started_at_unix_ms: 1_730_000_201_000,
                    intent: Some("prepare bounded repair plan".to_string()),
                    metadata: json!({"dataset": SYNTHETIC_RISK_GATED_DATASET, "risk_profile": "caution"}),
                },
                trace_links: vec![
                    seed_trace_link(
                        caution_input.object_id(),
                        EpisodeTraceKind::Input,
                        1_730_000_201_100,
                    ),
                    seed_trace_link(
                        caution_note.object_id(),
                        EpisodeTraceKind::Context,
                        1_730_000_202_100,
                    ),
                ],
                outcome: None,
                earl_evaluations: vec![EarlEvaluationRequest {
                    evaluation_id: "seed-earl-risk-caution".to_string(),
                    episode_id: "seed-episode-risk-caution".to_string(),
                    evaluated_at_unix_ms: 1_730_000_202_500,
                    signals: EarlSignalVector {
                        uncertainty: 0.60,
                        conflict: 0.45,
                        continuity_drift: 0.55,
                        constraint_pressure: 0.40,
                        tool_instability: 0.25,
                        novelty_spike: 0.30,
                    },
                    metadata: json!({"dataset": SYNTHETIC_RISK_GATED_DATASET, "risk_profile": "caution"}),
                }],
            },
            SeedEpisodeFixture {
                create: CreateEpisodeRequest {
                    episode_id: "seed-episode-risk-reflex".to_string(),
                    stream_id: Some(stream_id.to_string()),
                    source_kind: "gestalt-seed".to_string(),
                    episode_kind: "risk_gate".to_string(),
                    started_at_unix_ms: 1_730_000_203_000,
                    intent: Some("attempt remote retry without validated boundary".to_string()),
                    metadata: json!({"dataset": SYNTHETIC_RISK_GATED_DATASET, "risk_profile": "reflex"}),
                },
                trace_links: vec![
                    seed_trace_link(
                        reflex_input.object_id(),
                        EpisodeTraceKind::Input,
                        1_730_000_203_100,
                    ),
                    seed_trace_link(
                        reflex_output.object_id(),
                        EpisodeTraceKind::Output,
                        1_730_000_204_100,
                    ),
                ],
                outcome: None,
                earl_evaluations: vec![EarlEvaluationRequest {
                    evaluation_id: "seed-earl-risk-reflex".to_string(),
                    episode_id: "seed-episode-risk-reflex".to_string(),
                    evaluated_at_unix_ms: 1_730_000_204_500,
                    signals: EarlSignalVector {
                        uncertainty: 0.60,
                        conflict: 0.92,
                        continuity_drift: 0.95,
                        constraint_pressure: 0.60,
                        tool_instability: 0.30,
                        novelty_spike: 0.40,
                    },
                    metadata: json!({"dataset": SYNTHETIC_RISK_GATED_DATASET, "risk_profile": "reflex"}),
                }],
            },
        ],
    }
}

fn seed_text(
    stream_id: &str,
    object_kind: TextObjectKind,
    sequence: u64,
    ts_unix_ms: i64,
    text: &str,
    metadata: Value,
) -> SeedTextObject {
    SeedTextObject {
        stream_id: stream_id.to_string(),
        source_kind: "gestalt-seed".to_string(),
        object_kind,
        sequence,
        ts_unix_ms,
        text: text.to_string(),
        metadata,
    }
}

fn seed_trace_link(
    object_id: String,
    trace_kind: EpisodeTraceKind,
    linked_at_unix_ms: i64,
) -> SeedTraceLinkFixture {
    SeedTraceLinkFixture {
        object_id,
        trace_kind,
        linked_at_unix_ms,
        metadata: json!({}),
    }
}
