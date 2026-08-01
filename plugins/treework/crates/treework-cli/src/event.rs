use crate::tree_diff::TreeOperation;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const EVENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition<T> {
    pub before: T,
    pub after: T,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitialTransition {
    pub before: Option<String>,
    pub after: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInitializedData {
    pub stage: InitialTransition,
    pub current_branch: InitialTransition,
    pub snapshot_ref: String,
    pub checkpoint_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlignmentData {
    pub stage: Transition<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeEditingSummary {
    pub mode: String,
    pub base_tree_revision: u64,
    pub base_event_seq: u64,
    pub base_state_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeEditingData {
    pub stage: Transition<String>,
    pub editing: TreeEditingSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeAppliedBase {
    pub event_seq: u64,
    pub tree_revision: u64,
    pub state_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeAppliedResult {
    pub tree_revision: u64,
    pub tree_document_hash: String,
    pub accepted_tree_state_hash: String,
    pub topology_changed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeAppliedData {
    pub base: TreeAppliedBase,
    pub result: TreeAppliedResult,
    pub operations: Vec<TreeOperation>,
    pub affected_subjects: Vec<String>,
    pub snapshot_ref: String,
    pub checkpoint_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IsolationEventData {
    pub mode: String,
    pub workspace_path: String,
    pub git_branch: String,
    pub managed_by_treework: bool,
    pub action: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchEnteredData {
    pub current_branch: Transition<String>,
    pub status: Transition<String>,
    pub reason: Transition<String>,
    pub isolation: IsolationEventData,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchStatusData {
    pub status: Transition<String>,
    pub reason: Transition<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationSummary {
    pub status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchCompletedData {
    pub status: Transition<String>,
    pub reason: Transition<String>,
    pub verification: VerificationSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationEvidence {
    pub command: String,
    pub result: String,
    pub gap: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationRecordedData {
    pub verification: Transition<String>,
    pub evidence: VerificationEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventData {
    ProjectInitialized(ProjectInitializedData),
    AlignmentStarted(AlignmentData),
    AlignmentAccepted(AlignmentData),
    TreeEditingStarted(TreeEditingData),
    TreeEditingUpdated(TreeEditingData),
    TreeApplied(TreeAppliedData),
    BranchEntered(BranchEnteredData),
    BranchPaused(BranchStatusData),
    BranchCompleted(BranchCompletedData),
    BranchAborted(BranchStatusData),
    VerificationRecorded(VerificationRecordedData),
}

impl EventData {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::ProjectInitialized(_) => "project.initialized",
            Self::AlignmentStarted(_) => "alignment.started",
            Self::AlignmentAccepted(_) => "alignment.accepted",
            Self::TreeEditingStarted(_) => "tree.editing_started",
            Self::TreeEditingUpdated(_) => "tree.editing_updated",
            Self::TreeApplied(_) => "tree.applied",
            Self::BranchEntered(_) => "branch.entered",
            Self::BranchPaused(_) => "branch.paused",
            Self::BranchCompleted(_) => "branch.completed",
            Self::BranchAborted(_) => "branch.aborted",
            Self::VerificationRecorded(_) => "verification.recorded",
        }
    }

    pub(crate) fn to_value(&self) -> Result<Value, serde_json::Error> {
        match self {
            Self::ProjectInitialized(value) => serde_json::to_value(value),
            Self::AlignmentStarted(value) | Self::AlignmentAccepted(value) => {
                serde_json::to_value(value)
            }
            Self::TreeEditingStarted(value) | Self::TreeEditingUpdated(value) => {
                serde_json::to_value(value)
            }
            Self::TreeApplied(value) => serde_json::to_value(value),
            Self::BranchEntered(value) => serde_json::to_value(value),
            Self::BranchPaused(value) | Self::BranchAborted(value) => serde_json::to_value(value),
            Self::BranchCompleted(value) => serde_json::to_value(value),
            Self::VerificationRecorded(value) => serde_json::to_value(value),
        }
    }

    fn from_value(event_type: &str, value: Value) -> Result<Option<Self>, serde_json::Error> {
        match event_type {
            "project.initialized" => serde_json::from_value(value)
                .map(Self::ProjectInitialized)
                .map(Some),
            "alignment.started" => serde_json::from_value(value)
                .map(Self::AlignmentStarted)
                .map(Some),
            "alignment.accepted" => serde_json::from_value(value)
                .map(Self::AlignmentAccepted)
                .map(Some),
            "tree.editing_started" => serde_json::from_value(value)
                .map(Self::TreeEditingStarted)
                .map(Some),
            "tree.editing_updated" => serde_json::from_value(value)
                .map(Self::TreeEditingUpdated)
                .map(Some),
            "tree.applied" => serde_json::from_value(value)
                .map(Self::TreeApplied)
                .map(Some),
            "branch.entered" => serde_json::from_value(value)
                .map(Self::BranchEntered)
                .map(Some),
            "branch.paused" => serde_json::from_value(value)
                .map(Self::BranchPaused)
                .map(Some),
            "branch.completed" => serde_json::from_value(value)
                .map(Self::BranchCompleted)
                .map(Some),
            "branch.aborted" => serde_json::from_value(value)
                .map(Self::BranchAborted)
                .map(Some),
            "verification.recorded" => serde_json::from_value(value)
                .map(Self::VerificationRecorded)
                .map(Some),
            _ => Ok(None),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventEnvelope {
    pub schema_version: u32,
    pub seq: u64,
    pub time: String,
    pub event_type: String,
    pub subject: String,
    pub message: String,
    pub tree_revision: u64,
    pub data: EventData,
}

impl EventEnvelope {
    pub fn new(
        seq: u64,
        time: String,
        subject: impl Into<String>,
        message: impl Into<String>,
        tree_revision: u64,
        data: EventData,
    ) -> Self {
        Self {
            schema_version: EVENT_SCHEMA_VERSION,
            seq,
            time,
            event_type: data.event_type().to_string(),
            subject: subject.into(),
            message: message.into(),
            tree_revision,
            data,
        }
    }

    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        let value = serde_json::json!({
            "schema_version": self.schema_version,
            "seq": self.seq,
            "time": self.time,
            "type": self.event_type,
            "subject": self.subject,
            "message": self.message,
            "tree_revision": self.tree_revision,
            "data": self.data.to_value()?,
        });
        Ok(format!("{}\n", serde_json::to_string(&value)?))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegacyEvent {
    pub seq: u64,
    pub time: String,
    pub event_type: String,
    pub subject: String,
    pub message: String,
    pub tree_revision: Option<u64>,
    pub raw: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedEvent {
    pub seq: u64,
    pub time: String,
    pub event_type: String,
    pub subject: String,
    pub message: String,
    pub tree_revision: Option<u64>,
    pub schema_version: Option<u32>,
    pub reason: String,
    pub raw: Value,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Replayability {
    Replayable,
    Unsupported(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedEvent {
    Current(EventEnvelope),
    Legacy(LegacyEvent),
    Unsupported(UnsupportedEvent),
}

impl ParsedEvent {
    pub fn seq(&self) -> u64 {
        match self {
            Self::Current(event) => event.seq,
            Self::Legacy(event) => event.seq,
            Self::Unsupported(event) => event.seq,
        }
    }

    #[allow(dead_code)]
    pub fn replayability(&self) -> Replayability {
        match self {
            Self::Current(_) => Replayability::Replayable,
            Self::Legacy(_) => Replayability::Unsupported(
                "legacy event lacks typed before/after replay data".to_string(),
            ),
            Self::Unsupported(event) => Replayability::Unsupported(event.reason.clone()),
        }
    }
}

pub fn parse_event_log(bytes: &[u8]) -> Result<Vec<ParsedEvent>, String> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    if !bytes.ends_with(b"\n") {
        return Err("events.jsonl has a partial final record".to_string());
    }
    let source = std::str::from_utf8(bytes)
        .map_err(|error| format!("events.jsonl is not valid UTF-8: {}", error))?;
    let mut events = Vec::new();
    let mut previous_seq = 0;
    for (index, line) in source.lines().enumerate() {
        if line.trim().is_empty() {
            return Err(format!("events.jsonl line {} is empty", index + 1));
        }
        let event = parse_event_line(line)
            .map_err(|error| format!("events.jsonl line {}: {}", index + 1, error))?;
        let expected = previous_seq + 1;
        if event.seq() != expected {
            return Err(format!(
                "events.jsonl line {} has sequence {}, expected {}",
                index + 1,
                event.seq(),
                expected
            ));
        }
        previous_seq = event.seq();
        events.push(event);
    }
    Ok(events)
}

pub fn parse_event_line(line: &str) -> Result<ParsedEvent, String> {
    let raw: Value =
        serde_json::from_str(line).map_err(|error| format!("invalid JSON: {}", error))?;
    let object = raw
        .as_object()
        .ok_or_else(|| "event record must be a JSON object".to_string())?;
    let seq = required_u64(object.get("seq"), "seq")?;
    let time = required_string(object.get("time"), "time")?;
    let event_type = required_string(object.get("type"), "type")?;
    let subject = required_string(object.get("subject"), "subject")?;
    let message = required_string(object.get("message"), "message")?;
    let tree_revision = object.get("tree_revision").and_then(Value::as_u64);
    let schema_value = object.get("schema_version");
    let has_data = object.contains_key("data");
    if schema_value.is_none() && !has_data {
        return Ok(ParsedEvent::Legacy(LegacyEvent {
            seq,
            time,
            event_type,
            subject,
            message,
            tree_revision,
            raw,
        }));
    }

    let schema_version = match schema_value {
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| {
                    "event schema_version must be an unsigned 32-bit integer".to_string()
                })?,
        ),
        None => None,
    };

    if schema_version.is_none() {
        return Ok(ParsedEvent::Unsupported(UnsupportedEvent {
            seq,
            time,
            event_type,
            subject,
            message,
            tree_revision,
            schema_version,
            reason: "event data is present without schema_version".to_string(),
            raw,
        }));
    }

    if schema_version != Some(EVENT_SCHEMA_VERSION) {
        return Ok(ParsedEvent::Unsupported(UnsupportedEvent {
            seq,
            time,
            event_type,
            subject,
            message,
            tree_revision,
            schema_version,
            reason: format!(
                "unsupported event schema version {}",
                schema_version.unwrap_or_default()
            ),
            raw,
        }));
    }

    if !has_data {
        return Ok(ParsedEvent::Unsupported(UnsupportedEvent {
            seq,
            time,
            event_type,
            subject,
            message,
            tree_revision,
            schema_version,
            reason: "current event is missing data".to_string(),
            raw,
        }));
    }

    let Some(tree_revision) = tree_revision else {
        return Err("current event is missing tree_revision".to_string());
    };
    let data_value = object
        .get("data")
        .cloned()
        .ok_or_else(|| "current event is missing data".to_string())?;
    let Some(data) = EventData::from_value(&event_type, data_value)
        .map_err(|error| format!("invalid {} data: {}", event_type, error))?
    else {
        return Ok(ParsedEvent::Unsupported(UnsupportedEvent {
            seq,
            time,
            event_type,
            subject,
            message,
            tree_revision: Some(tree_revision),
            schema_version,
            reason: "unknown current event type".to_string(),
            raw,
        }));
    };

    Ok(ParsedEvent::Current(EventEnvelope {
        schema_version: EVENT_SCHEMA_VERSION,
        seq,
        time,
        event_type,
        subject,
        message,
        tree_revision,
        data,
    }))
}

fn required_u64(value: Option<&Value>, name: &str) -> Result<u64, String> {
    value
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("event {} must be an unsigned integer", name))
}

fn required_string(value: Option<&Value>, name: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("event {} must be a string", name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paused_event() -> EventEnvelope {
        EventEnvelope::new(
            1,
            "unix:1".to_string(),
            "alpha",
            "Waiting",
            2,
            EventData::BranchPaused(BranchStatusData {
                status: Transition {
                    before: "in_progress".to_string(),
                    after: "paused".to_string(),
                },
                reason: Transition {
                    before: String::new(),
                    after: "Waiting".to_string(),
                },
            }),
        )
    }

    #[test]
    fn current_event_round_trips_as_replayable() {
        let line = paused_event().to_json_line().unwrap();
        let parsed = parse_event_log(line.as_bytes()).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].replayability(), Replayability::Replayable);
        assert!(matches!(
            &parsed[0],
            ParsedEvent::Current(EventEnvelope {
                data: EventData::BranchPaused(_),
                ..
            })
        ));
    }

    #[test]
    fn legacy_event_is_read_but_not_upgraded() {
        let source = br#"{"seq":1,"time":"unix:1","type":"branch.paused","subject":"alpha","message":"Waiting"}
"#;
        let parsed = parse_event_log(source).unwrap();
        assert!(matches!(parsed[0], ParsedEvent::Legacy(_)));
        assert!(matches!(
            parsed[0].replayability(),
            Replayability::Unsupported(_)
        ));
    }

    #[test]
    fn unknown_current_event_is_explicitly_unsupported() {
        let source = br#"{"schema_version":1,"seq":1,"time":"unix:1","type":"future.changed","subject":"root","message":"Future","tree_revision":0,"data":{}}
"#;
        let parsed = parse_event_log(source).unwrap();
        assert!(matches!(parsed[0], ParsedEvent::Unsupported(_)));
    }

    #[test]
    fn versioned_event_without_data_is_not_legacy() {
        let source = r#"{"schema_version":1,"seq":1,"time":"unix:1","type":"branch.paused","subject":"alpha","message":"Waiting","tree_revision":2}"#;
        assert!(matches!(
            parse_event_line(source).unwrap(),
            ParsedEvent::Unsupported(_)
        ));
    }

    #[test]
    fn unknown_version_without_data_is_not_legacy() {
        let source = r#"{"schema_version":2,"seq":1,"time":"unix:1","type":"branch.paused","subject":"alpha","message":"Waiting","tree_revision":2}"#;
        assert!(matches!(
            parse_event_line(source).unwrap(),
            ParsedEvent::Unsupported(_)
        ));
    }

    #[test]
    fn data_without_version_is_not_legacy() {
        let source = r#"{"seq":1,"time":"unix:1","type":"branch.paused","subject":"alpha","message":"Waiting","tree_revision":2,"data":{}}"#;
        assert!(matches!(
            parse_event_line(source).unwrap(),
            ParsedEvent::Unsupported(_)
        ));
    }

    #[test]
    fn malformed_schema_version_is_rejected() {
        let source = r#"{"schema_version":"1","seq":1,"time":"unix:1","type":"branch.paused","subject":"alpha","message":"Waiting","tree_revision":2}"#;
        assert!(parse_event_line(source).is_err());
    }

    #[test]
    fn partial_tail_is_rejected() {
        let line = paused_event().to_json_line().unwrap();
        assert!(parse_event_log(line.trim_end().as_bytes()).is_err());
    }
}
