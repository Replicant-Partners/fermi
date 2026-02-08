//! Core domain types for the Collaboration Coherence Evaluator.
//!
//! These types model multi-party conversations at the level needed for
//! Thagard's TEC analysis: participants, messages, and the utterance-propositions
//! extracted from them.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Newtype IDs ───────────────────────────────────────────────────────────

/// Unique identifier for an utterance-proposition in the coherence network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UtteranceId(pub Uuid);

impl UtteranceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for UtteranceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for UtteranceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a participant in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParticipantId(pub Uuid);

impl ParticipantId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ParticipantId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ParticipantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a conversation being observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConversationId(pub Uuid);

impl ConversationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for ConversationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a raw message in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(pub Uuid);

impl MessageId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ─── Utterance Kinds ───────────────────────────────────────────────────────

/// Classification of an utterance-proposition within the TEC framework.
///
/// Each message in a conversation may yield one or more utterances, each
/// classified into one of these kinds. The kind determines how the utterance
/// participates in coherence/incoherence relations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UtteranceKind {
    /// A factual or evaluative assertion by a participant.
    /// Example: "I think we should use Rust for the backend."
    Claim,

    /// A piece of shared data, measurement, or agreed-upon fact.
    /// These have intrinsic acceptability per Thagard's Principle 4 (Data Priority).
    /// Example: "The benchmark shows 3x throughput improvement."
    Evidence,

    /// A justification or reasoning chain that connects claims to evidence
    /// or to other claims.
    /// Example: "Rust gives us memory safety without a GC, which explains the
    /// lower tail latency."
    Explanation,

    /// A structural parallel drawn between the current topic and another domain.
    /// Creates coherence links per Thagard's Principle 3.
    /// Example: "This is like the TCP congestion control problem — we need
    /// backpressure."
    Analogy,

    /// A question that solicits information, clarification, or reasoning.
    /// Questions themselves don't directly participate in coherence scoring
    /// but may trigger utterances that do.
    Question,

    /// An acknowledgment or agreement with another participant's utterance.
    /// Creates a coherence link between the acknowledger and the acknowledged.
    Acknowledgment,

    /// A meta-comment about the conversation process itself.
    /// Example: "Let's step back and summarize where we are."
    Procedural,
}

impl UtteranceKind {
    /// Returns `true` if this kind has intrinsic acceptability (Data Priority).
    pub fn has_intrinsic_acceptability(&self) -> bool {
        matches!(self, UtteranceKind::Evidence)
    }

    /// Returns `true` if this kind directly participates in coherence scoring.
    pub fn is_scorable(&self) -> bool {
        !matches!(self, UtteranceKind::Question | UtteranceKind::Procedural)
    }

    /// Returns a human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            UtteranceKind::Claim => "claim",
            UtteranceKind::Evidence => "evidence",
            UtteranceKind::Explanation => "explanation",
            UtteranceKind::Analogy => "analogy",
            UtteranceKind::Question => "question",
            UtteranceKind::Acknowledgment => "acknowledgment",
            UtteranceKind::Procedural => "procedural",
        }
    }
}

impl std::fmt::Display for UtteranceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ─── Utterance ─────────────────────────────────────────────────────────────

/// An utterance-proposition extracted from a participant's message.
///
/// This is a node in the coherence network. A single message may produce
/// multiple utterances (e.g. a message that both states a claim and provides
/// evidence). Each utterance is classified by kind and attributed to a
/// participant and source message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utterance {
    /// Unique identifier for this utterance.
    pub id: UtteranceId,

    /// The participant who produced this utterance.
    pub participant_id: ParticipantId,

    /// The source message this utterance was extracted from.
    pub message_id: MessageId,

    /// Classification of the utterance.
    pub kind: UtteranceKind,

    /// The textual content of the utterance (may be a subset of the full message).
    pub content: String,

    /// A short normalized form for comparison and deduplication.
    /// Optional — populated by the observer when NLP extraction is available.
    pub normalized: Option<String>,

    /// Confidence that the `kind` classification is correct, in [0, 1].
    pub classification_confidence: f64,

    /// When this utterance was produced.
    pub timestamp: DateTime<Utc>,

    /// Optional tags for domain-specific annotation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl Utterance {
    /// Create a new utterance with the given properties.
    pub fn new(
        participant_id: ParticipantId,
        message_id: MessageId,
        kind: UtteranceKind,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: UtteranceId::new(),
            participant_id,
            message_id,
            kind,
            content: content.into(),
            normalized: None,
            classification_confidence: 1.0,
            timestamp: Utc::now(),
            tags: Vec::new(),
        }
    }

    /// Returns `true` if this utterance has intrinsic acceptability (evidence).
    pub fn is_evidence(&self) -> bool {
        self.kind.has_intrinsic_acceptability()
    }

    /// Returns `true` if this utterance participates in coherence scoring.
    pub fn is_scorable(&self) -> bool {
        self.kind.is_scorable()
    }

    /// Set the normalized form.
    pub fn with_normalized(mut self, normalized: impl Into<String>) -> Self {
        self.normalized = Some(normalized.into());
        self
    }

    /// Set the classification confidence.
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.classification_confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }
}

// ─── Participant ───────────────────────────────────────────────────────────

/// The role a participant plays in the conversation, if known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticipantRole {
    /// A human participant.
    Human,
    /// An AI or automated agent participant.
    Agent,
    /// A facilitator or moderator.
    Facilitator,
    /// Role is unknown or not specified.
    Unknown,
}

impl Default for ParticipantRole {
    fn default() -> Self {
        Self::Unknown
    }
}

/// A participant in a multi-party conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    /// Unique identifier.
    pub id: ParticipantId,

    /// Display name or handle.
    pub name: String,

    /// Role in the conversation.
    #[serde(default)]
    pub role: ParticipantRole,

    /// Optional metadata (e.g., team, department, model name for agents).
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub metadata: std::collections::HashMap<String, String>,
}

impl Participant {
    /// Create a new participant with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: ParticipantId::new(),
            name: name.into(),
            role: ParticipantRole::default(),
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Set the role.
    pub fn with_role(mut self, role: ParticipantRole) -> Self {
        self.role = role;
        self
    }

    /// Add a metadata entry.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

// ─── Message ───────────────────────────────────────────────────────────────

/// A raw message in a conversation, before utterance extraction.
///
/// One message may produce zero or more [`Utterance`] values. For example,
/// "I agree with Alice (acknowledgment). The data shows 10% uplift (evidence),
/// which supports the Rust hypothesis (explanation)." would yield three utterances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique identifier.
    pub id: MessageId,

    /// The participant who sent this message.
    pub participant_id: ParticipantId,

    /// The full text content of the message.
    pub content: String,

    /// When the message was sent.
    pub timestamp: DateTime<Utc>,

    /// Optional: which message this is replying to (threading).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<MessageId>,

    /// IDs of utterances extracted from this message.
    /// Populated after the observer processes the message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub utterance_ids: Vec<UtteranceId>,
}

impl Message {
    /// Create a new message.
    pub fn new(participant_id: ParticipantId, content: impl Into<String>) -> Self {
        Self {
            id: MessageId::new(),
            participant_id,
            content: content.into(),
            timestamp: Utc::now(),
            reply_to: None,
            utterance_ids: Vec::new(),
        }
    }

    /// Set the reply-to reference.
    pub fn with_reply_to(mut self, reply_to: MessageId) -> Self {
        self.reply_to = Some(reply_to);
        self
    }

    /// Record that an utterance was extracted from this message.
    pub fn add_utterance(&mut self, utterance_id: UtteranceId) {
        self.utterance_ids.push(utterance_id);
    }
}

// ─── Conversation ──────────────────────────────────────────────────────────

/// The current status of a conversation being observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversationStatus {
    /// The conversation is actively being observed.
    Active,
    /// The conversation has been paused (no new messages expected temporarily).
    Paused,
    /// The conversation has concluded.
    Completed,
    /// The conversation was abandoned or errored out.
    Abandoned,
}

impl Default for ConversationStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// A multi-party conversation being observed by the coherence evaluator.
///
/// This is the top-level container that holds participants, messages, and
/// extracted utterances. The coherence engine operates on the utterances
/// extracted from this conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Unique identifier.
    pub id: ConversationId,

    /// Human-readable title or topic.
    pub title: Option<String>,

    /// The participants in this conversation.
    pub participants: Vec<Participant>,

    /// The raw messages, in chronological order.
    pub messages: Vec<Message>,

    /// Current status.
    #[serde(default)]
    pub status: ConversationStatus,

    /// When the conversation started.
    pub started_at: DateTime<Utc>,

    /// When the conversation was last updated.
    pub updated_at: DateTime<Utc>,

    /// Optional metadata.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub metadata: std::collections::HashMap<String, String>,
}

impl Conversation {
    /// Create a new conversation.
    pub fn new(title: Option<impl Into<String>>) -> Self {
        let now = Utc::now();
        Self {
            id: ConversationId::new(),
            title: title.map(Into::into),
            participants: Vec::new(),
            messages: Vec::new(),
            status: ConversationStatus::Active,
            started_at: now,
            updated_at: now,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Add a participant to the conversation.
    pub fn add_participant(&mut self, participant: Participant) {
        self.participants.push(participant);
        self.updated_at = Utc::now();
    }

    /// Add a message to the conversation.
    pub fn add_message(&mut self, message: Message) {
        self.messages.push(message);
        self.updated_at = Utc::now();
    }

    /// Look up a participant by ID.
    pub fn participant(&self, id: ParticipantId) -> Option<&Participant> {
        self.participants.iter().find(|p| p.id == id)
    }

    /// Look up a message by ID.
    pub fn message(&self, id: MessageId) -> Option<&Message> {
        self.messages.iter().find(|m| m.id == id)
    }

    /// Returns the number of messages in the conversation.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Returns the number of participants.
    pub fn participant_count(&self) -> usize {
        self.participants.len()
    }

    /// Returns `true` if the conversation is still active.
    pub fn is_active(&self) -> bool {
        self.status == ConversationStatus::Active
    }

    /// Returns all messages from a specific participant.
    pub fn messages_from(&self, participant_id: ParticipantId) -> Vec<&Message> {
        self.messages
            .iter()
            .filter(|m| m.participant_id == participant_id)
            .collect()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utterance_kind_properties() {
        assert!(UtteranceKind::Evidence.has_intrinsic_acceptability());
        assert!(!UtteranceKind::Claim.has_intrinsic_acceptability());

        assert!(UtteranceKind::Claim.is_scorable());
        assert!(UtteranceKind::Evidence.is_scorable());
        assert!(UtteranceKind::Explanation.is_scorable());
        assert!(UtteranceKind::Analogy.is_scorable());
        assert!(!UtteranceKind::Question.is_scorable());
        assert!(!UtteranceKind::Procedural.is_scorable());
        assert!(UtteranceKind::Acknowledgment.is_scorable());
    }

    #[test]
    fn create_utterance() {
        let pid = ParticipantId::new();
        let mid = MessageId::new();
        let u = Utterance::new(pid, mid, UtteranceKind::Claim, "Rust is fast");

        assert_eq!(u.participant_id, pid);
        assert_eq!(u.message_id, mid);
        assert_eq!(u.kind, UtteranceKind::Claim);
        assert_eq!(u.content, "Rust is fast");
        assert!(u.normalized.is_none());
        assert_eq!(u.classification_confidence, 1.0);
        assert!(u.tags.is_empty());
        assert!(u.is_scorable());
        assert!(!u.is_evidence());
    }

    #[test]
    fn utterance_builder_methods() {
        let pid = ParticipantId::new();
        let mid = MessageId::new();
        let u = Utterance::new(pid, mid, UtteranceKind::Evidence, "10x throughput")
            .with_normalized("throughput improvement")
            .with_confidence(0.85)
            .with_tag("benchmark");

        assert_eq!(u.normalized.as_deref(), Some("throughput improvement"));
        assert!((u.classification_confidence - 0.85).abs() < f64::EPSILON);
        assert_eq!(u.tags, vec!["benchmark"]);
        assert!(u.is_evidence());
    }

    #[test]
    fn confidence_is_clamped() {
        let pid = ParticipantId::new();
        let mid = MessageId::new();

        let u = Utterance::new(pid, mid, UtteranceKind::Claim, "test").with_confidence(5.0);
        assert!((u.classification_confidence - 1.0).abs() < f64::EPSILON);

        let u = Utterance::new(pid, mid, UtteranceKind::Claim, "test").with_confidence(-2.0);
        assert!(u.classification_confidence.abs() < f64::EPSILON);
    }

    #[test]
    fn create_participant() {
        let p = Participant::new("Alice")
            .with_role(ParticipantRole::Human)
            .with_metadata("team", "engineering");

        assert_eq!(p.name, "Alice");
        assert_eq!(p.role, ParticipantRole::Human);
        assert_eq!(
            p.metadata.get("team").map(String::as_str),
            Some("engineering")
        );
    }

    #[test]
    fn create_message() {
        let pid = ParticipantId::new();
        let m = Message::new(pid, "Hello everyone");

        assert_eq!(m.participant_id, pid);
        assert_eq!(m.content, "Hello everyone");
        assert!(m.reply_to.is_none());
        assert!(m.utterance_ids.is_empty());
    }

    #[test]
    fn message_reply_chain() {
        let pid = ParticipantId::new();
        let m1 = Message::new(pid, "First message");
        let m2 = Message::new(pid, "Reply").with_reply_to(m1.id);

        assert_eq!(m2.reply_to, Some(m1.id));
    }

    #[test]
    fn message_tracks_utterances() {
        let pid = ParticipantId::new();
        let mut m = Message::new(pid, "Complex message");
        let uid1 = UtteranceId::new();
        let uid2 = UtteranceId::new();

        m.add_utterance(uid1);
        m.add_utterance(uid2);

        assert_eq!(m.utterance_ids.len(), 2);
        assert_eq!(m.utterance_ids[0], uid1);
        assert_eq!(m.utterance_ids[1], uid2);
    }

    #[test]
    fn create_conversation() {
        let mut conv = Conversation::new(Some("Sprint Planning"));

        assert_eq!(conv.title.as_deref(), Some("Sprint Planning"));
        assert!(conv.is_active());
        assert_eq!(conv.message_count(), 0);
        assert_eq!(conv.participant_count(), 0);

        let alice = Participant::new("Alice").with_role(ParticipantRole::Human);
        let alice_id = alice.id;
        conv.add_participant(alice);

        let bob = Participant::new("Bob").with_role(ParticipantRole::Agent);
        conv.add_participant(bob);

        assert_eq!(conv.participant_count(), 2);

        let msg = Message::new(alice_id, "Let's discuss the roadmap");
        conv.add_message(msg);

        assert_eq!(conv.message_count(), 1);
        assert_eq!(conv.messages_from(alice_id).len(), 1);
    }

    #[test]
    fn conversation_lookup() {
        let mut conv = Conversation::new(None::<String>);

        let alice = Participant::new("Alice");
        let alice_id = alice.id;
        conv.add_participant(alice);

        let msg = Message::new(alice_id, "Hello");
        let msg_id = msg.id;
        conv.add_message(msg);

        assert!(conv.participant(alice_id).is_some());
        assert_eq!(conv.participant(alice_id).unwrap().name, "Alice");

        assert!(conv.message(msg_id).is_some());
        assert_eq!(conv.message(msg_id).unwrap().content, "Hello");

        // Nonexistent lookups return None
        assert!(conv.participant(ParticipantId::new()).is_none());
        assert!(conv.message(MessageId::new()).is_none());
    }

    #[test]
    fn conversation_status_transitions() {
        let mut conv = Conversation::new(Some("Test"));
        assert_eq!(conv.status, ConversationStatus::Active);
        assert!(conv.is_active());

        conv.status = ConversationStatus::Paused;
        assert!(!conv.is_active());

        conv.status = ConversationStatus::Completed;
        assert!(!conv.is_active());
    }

    #[test]
    fn ids_are_unique() {
        let ids: Vec<UtteranceId> = (0..100).map(|_| UtteranceId::new()).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len());
    }

    #[test]
    fn roundtrip_serialize_utterance() {
        let pid = ParticipantId::new();
        let mid = MessageId::new();
        let u = Utterance::new(pid, mid, UtteranceKind::Explanation, "Because X implies Y")
            .with_tag("logic");

        let json = serde_json::to_string(&u).unwrap();
        let deserialized: Utterance = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, u.id);
        assert_eq!(deserialized.kind, UtteranceKind::Explanation);
        assert_eq!(deserialized.content, "Because X implies Y");
        assert_eq!(deserialized.tags, vec!["logic"]);
    }

    #[test]
    fn roundtrip_serialize_conversation() {
        let mut conv = Conversation::new(Some("Roundtrip Test"));

        let p = Participant::new("TestUser").with_role(ParticipantRole::Human);
        let pid = p.id;
        conv.add_participant(p);

        let m = Message::new(pid, "Test message");
        conv.add_message(m);

        let json = serde_json::to_string_pretty(&conv).unwrap();
        let deserialized: Conversation = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, conv.id);
        assert_eq!(deserialized.title.as_deref(), Some("Roundtrip Test"));
        assert_eq!(deserialized.participant_count(), 1);
        assert_eq!(deserialized.message_count(), 1);
    }
}
