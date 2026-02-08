//! Top-level conversation observer.
//!
//! Ingests raw [`Message`](coherence_core::types::Message) objects, classifies
//! them into [`Utterance`](coherence_core::types::Utterance)s, detects
//! relations, and populates a [`CoherenceSystem`].

use coherence_core::{
    types::{ConversationId, Message, Utterance},
    CoherenceSystem,
};

use crate::classifier::UtteranceClassifier;
use crate::detector::{DetectedRelation, RelationDetector};

/// Observes a conversation and builds a [`CoherenceSystem`] from raw messages.
pub struct ConversationObserver {
    conversation_id: ConversationId,
}

impl ConversationObserver {
    /// Create a new observer for the given conversation.
    pub fn new(conversation_id: ConversationId) -> Self {
        Self { conversation_id }
    }

    /// Process a list of messages and return a fully populated [`CoherenceSystem`].
    ///
    /// Steps:
    /// 1. Classify each message into an utterance kind
    /// 2. Add all utterances to the system
    /// 3. Detect relations between utterance pairs
    /// 4. Add detected relations to the system
    pub fn observe(&self, messages: &[Message]) -> CoherenceSystem {
        let mut system = CoherenceSystem::new(self.conversation_id);

        // Step 1 & 2: Classify and add utterances
        let mut utterances = Vec::with_capacity(messages.len());
        for msg in messages {
            let classification = UtteranceClassifier::classify(&msg.content);
            let utterance = Utterance::new(
                msg.participant_id,
                msg.id,
                classification.kind,
                &msg.content,
            );
            utterances.push(utterance.clone());
            system.add_utterance(utterance);
        }

        // Step 3 & 4: Detect and add relations
        let relations = RelationDetector::detect_all(&utterances);
        for rel in relations {
            match rel {
                DetectedRelation::Coherence(cr) => {
                    let _ = system.add_coherence(cr);
                }
                DetectedRelation::Incoherence(ir) => {
                    let _ = system.add_incoherence(ir);
                }
            }
        }

        system
    }

    /// Process a single new message into an existing system.
    ///
    /// Classifies the message, adds the utterance, then detects relations
    /// between the new utterance and all existing utterances.
    pub fn observe_message(&self, system: &mut CoherenceSystem, message: &Message) {
        let classification = UtteranceClassifier::classify(&message.content);
        let utterance = Utterance::new(
            message.participant_id,
            message.id,
            classification.kind,
            &message.content,
        );

        let new_id = utterance.id;
        let new_utterance = utterance.clone();
        system.add_utterance(utterance);

        // Detect relations between the new utterance and all previous ones
        let existing: Vec<Utterance> = system
            .utterances
            .iter()
            .filter(|u| u.id != new_id)
            .cloned()
            .collect();

        for existing_u in &existing {
            let rels = RelationDetector::detect_all(&[existing_u.clone(), new_utterance.clone()]);
            for rel in rels {
                match rel {
                    DetectedRelation::Coherence(cr) => {
                        let _ = system.add_coherence(cr);
                    }
                    DetectedRelation::Incoherence(ir) => {
                        let _ = system.add_incoherence(ir);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coherence_core::types::{ParticipantId, UtteranceKind};

    fn make_message(participant: ParticipantId, content: &str) -> Message {
        Message::new(participant, content)
    }

    #[test]
    fn observe_creates_system_with_utterances() {
        let p1 = ParticipantId::new();
        let p2 = ParticipantId::new();

        let messages = vec![
            make_message(p1, "We need to improve performance"),
            make_message(p2, "Data shows response time is 500ms"),
            make_message(p1, "I agree, that's too slow"),
        ];

        let observer = ConversationObserver::new(ConversationId::new());
        let system = observer.observe(&messages);

        assert_eq!(system.utterance_count(), 3);
        // Should have detected some relations
        assert!(system.relation_count() > 0);
    }

    #[test]
    fn observe_classifies_correctly() {
        let p1 = ParticipantId::new();

        let messages = vec![
            make_message(p1, "What is the current latency?"),
            make_message(p1, "Data shows 50ms average"),
            make_message(p1, "Let's move on to the next topic"),
        ];

        let observer = ConversationObserver::new(ConversationId::new());
        let system = observer.observe(&messages);

        let kinds: Vec<UtteranceKind> = system.utterances.iter().map(|u| u.kind).collect();
        assert_eq!(kinds[0], UtteranceKind::Question);
        assert_eq!(kinds[1], UtteranceKind::Evidence);
        assert_eq!(kinds[2], UtteranceKind::Procedural);
    }

    #[test]
    fn observe_message_incrementally() {
        let p1 = ParticipantId::new();
        let p2 = ParticipantId::new();
        let conv_id = ConversationId::new();

        let observer = ConversationObserver::new(conv_id);
        let mut system = CoherenceSystem::new(conv_id);

        let m1 = make_message(p1, "Performance is critical for our app");
        observer.observe_message(&mut system, &m1);
        assert_eq!(system.utterance_count(), 1);

        let m2 = make_message(p2, "I agree, we should optimize");
        observer.observe_message(&mut system, &m2);
        assert_eq!(system.utterance_count(), 2);
        // The acknowledgment should create a relation
        assert!(system.relation_count() > 0);
    }

    #[test]
    fn full_pipeline_coherent_conversation() {
        let p1 = ParticipantId::new();
        let p2 = ParticipantId::new();

        let messages = vec![
            make_message(p1, "We should migrate to Rust for performance"),
            make_message(
                p2,
                "Data shows Rust is 10x faster than Python for this workload",
            ),
            make_message(p1, "Good point, that supports the migration"),
            make_message(p2, "Because the GC pauses are causing latency spikes"),
            make_message(p1, "I agree, the evidence is clear"),
        ];

        let observer = ConversationObserver::new(ConversationId::new());
        let system = observer.observe(&messages);

        assert_eq!(system.utterance_count(), 5);
        assert!(
            system.relation_count() >= 2,
            "should detect multiple relations in a coherent conversation, got {}",
            system.relation_count()
        );
    }
}
