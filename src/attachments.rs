//! # Image attachments — and the rule that a dropped frame is an error
//!
//! An agent that accepts a camera frame has a failure mode with no analogue in
//! the text-only platform: **the picture goes missing and the answer still
//! arrives.**
//!
//! Consider `hud_field_scout`. The wearer looks at a mushroom, presses the
//! temple button, and says "what is this?". If the frame is dropped anywhere
//! between the glasses and the model, the model receives the words "what is
//! this?" and nothing else. It will answer. It has to answer — that is what it
//! is for — and the answer will be a species name generated from no evidence
//! whatsoever, delivered through a boundary that correctly labels it
//! `model_inference`, on a card that correctly reads `medium`.
//!
//! Every check in `hud_contract` passes. The provenance tag is accurate. And
//! the answer is about nothing.
//!
//! That is worse than the `genome_profiler` incident rather than equivalent to
//! it, because there the missing source was *always* missing and a contract
//! could name the gap once. Here the same field is well-sourced on Tuesday and
//! evidence-free on Wednesday, depending on whether a base64 blob survived the
//! trip, and nothing downstream can tell the two apart.
//!
//! So this module's central claim is not about encoding:
//!
//! > **An attachment that cannot be delivered is an error. It is never
//! > dropped, never downgraded, and never delivered partially.**
//!
//! [`ensure_deliverable`] is that rule. Everything else here exists to make it
//! enforceable.
//!
//! ## Fail-closed on capability
//!
//! Whether a given model can accept an image is not something this platform can
//! discover at runtime, so [`VISION_CAPABLE`] is a hand-maintained list and an
//! **unrecognised model is treated as text-only**. That direction matters: the
//! optimistic version sends the image, the provider ignores or rejects it, and
//! the least visible outcome is the one where it answers anyway.

use std::fmt;

/// Media types an attachment may declare.
///
/// Transcribed from Anthropic's documented set for the Messages API. It is a
/// closed allowlist rather than a passthrough because an unsupported type
/// reaching the provider produces a 400 at best, and at worst a silently
/// ignored block — which is the dropped-frame failure this module exists to
/// prevent, arriving from the provider's side instead of ours.
///
/// If a provider adds a type, add it here deliberately. Do not widen this to
/// `image/*`.
pub const ALLOWED_MEDIA_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif", "image/webp"];

/// Largest decoded image accepted, in bytes.
///
/// Anthropic's documented per-image ceiling is 5 MB, so this sits under it with
/// room for the request's other content. It is a real constraint for this use
/// case rather than a formality: the glasses shoot 4K, and an unresized 4K JPEG
/// can exceed it. The right fix is downscaling in the relay — which does not
/// exist yet — so until then this returns a clear error naming the size instead
/// of letting the provider reject the whole request opaquely.
pub const MAX_DECODED_BYTES: usize = 4 * 1024 * 1024;

/// Models known to accept image content.
///
/// Matched by prefix, because model ids carry dated suffixes
/// (`claude-sonnet-4-5-20250929`). Hand-maintained: no API reports this, so the
/// alternative to a list is a guess, and a guess in the optimistic direction
/// sends a frame to a model that will quietly ignore it.
///
/// `("prefix", "why it is here")` — the note is required for the same reason
/// `grounding_trust` requires a `why`: the next author cannot tell a verified
/// entry from an assumed one, so they copy whichever is nearest.
pub const VISION_CAPABLE: &[(&str, &str)] = &[
    (
        "claude-3",
        "Anthropic's first vision-capable family; documented image support on \
         the Messages API.",
    ),
    (
        "claude-sonnet-4",
        "Documented image support. This is the rung `hud_field_scout` declares \
         for its premium tier, which is why the camera path is gated to it.",
    ),
    (
        "claude-opus-4",
        "Same family and API surface as claude-sonnet-4.",
    ),
    (
        "claude-haiku-4",
        "Same family and API surface as claude-sonnet-4. Listed so the standard \
         tier is usable for image work; if a specific haiku rung turns out not \
         to accept images, remove it here rather than handling the failure at \
         the call site.",
    ),
    (
        "gpt-4o",
        "OpenAI-compatible vision. Reachable through the openrouter path, which \
         cannot yet CARRY an image — see `ensure_deliverable`. Listed so that \
         the eventual failure is about the transport rather than about the \
         model.",
    ),
];

/// Why an attachment could not be delivered.
///
/// Every variant is a refusal. There is deliberately no variant meaning
/// "delivered without the image".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachmentError {
    /// Media type is not in [`ALLOWED_MEDIA_TYPES`].
    UnsupportedMediaType { declared: String },
    /// Payload is empty, or not valid base64.
    Malformed { why: String },
    /// Decoded payload exceeds [`MAX_DECODED_BYTES`].
    TooLarge { decoded_bytes: usize },
    /// The resolved model is not known to accept images.
    ModelCannotSee { model: String },
    /// The provider path cannot carry image content, whatever the model can do.
    ProviderCannotCarry { provider: String, detail: String },
}

impl fmt::Display for AttachmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttachmentError::UnsupportedMediaType { declared } => write!(
                f,
                "attachment declares media type `{declared}`, which is not one of \
                 {ALLOWED_MEDIA_TYPES:?}. Refused rather than forwarded: an \
                 unsupported type is either rejected by the provider or ignored by \
                 it, and the ignored case answers the question with no picture."
            ),
            AttachmentError::Malformed { why } => write!(
                f,
                "attachment payload is not usable base64 ({why}). Refused rather \
                 than sent as-is. If this came from a data URL, strip the \
                 `data:image/...;base64,` prefix before attaching."
            ),
            AttachmentError::TooLarge { decoded_bytes } => write!(
                f,
                "attachment decodes to {decoded_bytes} bytes, over the \
                 {MAX_DECODED_BYTES}-byte ceiling. The capture device shoots 4K and \
                 nothing downscales it yet, so this is expected for a full-\
                 resolution frame: resize before attaching."
            ),
            AttachmentError::ModelCannotSee { model } => write!(
                f,
                "`{model}` is not known to accept image content, so the frame \
                 cannot be delivered. Refused, because the alternative is a model \
                 answering \"what is this?\" from the words alone — which it will \
                 do, fluently, about nothing. If this model does support images, \
                 add it to VISION_CAPABLE with a note saying how that was \
                 established."
            ),
            AttachmentError::ProviderCannotCarry { provider, detail } => write!(
                f,
                "the `{provider}` execution path cannot carry image content: \
                 {detail}. Refused rather than silently text-only."
            ),
        }
    }
}

impl std::error::Error for AttachmentError {}

/// One image travelling with a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageAttachment {
    /// e.g. `image/jpeg`. Checked against [`ALLOWED_MEDIA_TYPES`].
    pub media_type: String,
    /// Base64 payload, without any `data:` URL prefix.
    pub data_base64: String,
    /// Where the frame came from, for the audit trail and for
    /// `hud_contract`'s `capture` block. Not interpreted.
    pub source: Option<String>,
}

impl ImageAttachment {
    /// Build an attachment, tolerating a `data:` URL prefix.
    ///
    /// The prefix is stripped rather than rejected because it is the single most
    /// common shape an image arrives in from a browser or a phone SDK, and
    /// failing on it would push callers toward hand-rolled string surgery.
    pub fn new(media_type: impl Into<String>, payload: &str, source: Option<String>) -> Self {
        let payload = payload.trim();
        let data = match payload.strip_prefix("data:") {
            Some(rest) => rest.split_once(";base64,").map_or(payload, |(_, b64)| b64),
            None => payload,
        };
        Self {
            media_type: media_type.into().trim().to_ascii_lowercase(),
            data_base64: data.trim().to_string(),
            source,
        }
    }

    /// Decoded size in bytes, computed from the base64 length.
    ///
    /// Arithmetic rather than an actual decode: this runs on a request path and
    /// the answer only decides whether to refuse.
    pub fn decoded_len(&self) -> usize {
        let len = self.data_base64.len();
        if len == 0 {
            return 0;
        }
        let padding = self
            .data_base64
            .bytes()
            .rev()
            .take_while(|b| *b == b'=')
            .count();
        (len / 4) * 3 - padding.min(2)
    }

    /// Is the payload well-formed, correctly typed, and within the ceiling?
    pub fn validate(&self) -> Result<(), AttachmentError> {
        if !ALLOWED_MEDIA_TYPES.contains(&self.media_type.as_str()) {
            return Err(AttachmentError::UnsupportedMediaType {
                declared: self.media_type.clone(),
            });
        }
        if self.data_base64.is_empty() {
            return Err(AttachmentError::Malformed {
                why: "payload is empty".into(),
            });
        }
        // Length must be a multiple of 4, and every character must be in the
        // base64 alphabet. Cheap, and catches a truncated upload — which is the
        // realistic failure for a large frame over a flaky link.
        if !self.data_base64.len().is_multiple_of(4) {
            return Err(AttachmentError::Malformed {
                why: format!(
                    "length {} is not a multiple of 4, so the payload is truncated",
                    self.data_base64.len()
                ),
            });
        }
        if let Some(bad) = self
            .data_base64
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || *c == '+' || *c == '/' || *c == '='))
        {
            return Err(AttachmentError::Malformed {
                why: format!("contains `{bad}`, which is not a base64 character"),
            });
        }
        let decoded = self.decoded_len();
        if decoded > MAX_DECODED_BYTES {
            return Err(AttachmentError::TooLarge {
                decoded_bytes: decoded,
            });
        }
        Ok(())
    }
}

/// Can `model` accept image content?
///
/// Prefix match against [`VISION_CAPABLE`]. An unrecognised model is **not**
/// vision-capable, which is the whole point: the optimistic default sends a
/// frame to something that ignores it.
pub fn model_can_see(model: &str) -> bool {
    let m = model.trim().to_ascii_lowercase();
    // Tolerate `anthropic/claude-sonnet-4-5` style ids from proxies.
    let bare = m.rsplit('/').next().unwrap_or(&m);
    VISION_CAPABLE
        .iter()
        .any(|(prefix, _)| bare.starts_with(prefix) || m.starts_with(prefix))
}

/// Execution paths that can carry image content today.
///
/// Only the native Anthropic path can. The OpenAI-compatible path in
/// `multi_model_executor` builds `OpenAIMessage { content: Option<String> }`,
/// which has no room for an image block, so routing a frame through it would
/// drop it — the exact failure this module refuses.
pub fn provider_can_carry(provider: &str) -> Result<(), AttachmentError> {
    match provider.trim().to_ascii_lowercase().as_str() {
        "anthropic" | "" => Ok(()),
        other => Err(AttachmentError::ProviderCannotCarry {
            provider: other.to_string(),
            detail: "only the native Anthropic path builds multi-block message \
                     content; the OpenAI-compatible path carries `content` as a \
                     plain string and would drop the image"
                .to_string(),
        }),
    }
}

/// **The rule.** Refuse unless every attachment can actually be delivered.
///
/// Checked in this order so the error names the most fundamental problem first:
/// a malformed payload is a caller bug, an incapable model is a configuration
/// choice, an incapable provider is a platform limitation.
///
/// Returns `Ok(())` for an empty attachment list — a text-only request is not
/// an undelivered image.
pub fn ensure_deliverable(
    attachments: &[ImageAttachment],
    provider: &str,
    model: &str,
) -> Result<(), AttachmentError> {
    if attachments.is_empty() {
        return Ok(());
    }
    for a in attachments {
        a.validate()?;
    }
    if !model_can_see(model) {
        return Err(AttachmentError::ModelCannotSee {
            model: model.to_string(),
        });
    }
    provider_can_carry(provider)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 12 base64 chars -> 9 bytes. Valid alphabet, multiple of 4.
    fn tiny() -> ImageAttachment {
        ImageAttachment::new("image/jpeg", "AAAAAAAAAAAA", Some("test".into()))
    }

    // ─── the rule ──────────────────────────────────────────────────────

    /// The reason this module exists.
    #[test]
    fn an_undeliverable_frame_is_refused_not_dropped() {
        let a = vec![tiny()];
        // Text-only model.
        assert!(matches!(
            ensure_deliverable(&a, "anthropic", "some-text-only-model"),
            Err(AttachmentError::ModelCannotSee { .. })
        ));
        // Capable model, incapable transport.
        assert!(matches!(
            ensure_deliverable(&a, "openrouter", "gpt-4o"),
            Err(AttachmentError::ProviderCannotCarry { .. })
        ));
    }

    /// A text-only request must not be treated as a lost image.
    #[test]
    fn no_attachments_is_not_an_error_on_any_path() {
        for provider in ["anthropic", "openrouter", "ollama", ""] {
            for model in ["claude-sonnet-4-5-20250929", "whatever-text-only"] {
                assert_eq!(ensure_deliverable(&[], provider, model), Ok(()));
            }
        }
    }

    #[test]
    fn a_deliverable_frame_is_accepted() {
        assert_eq!(
            ensure_deliverable(&[tiny()], "anthropic", "claude-sonnet-4-5-20250929"),
            Ok(())
        );
    }

    /// An unknown model must fail closed. If this ever returns true for an
    /// unrecognised id, a frame gets sent somewhere that will ignore it and the
    /// model answers from the words alone.
    #[test]
    fn an_unknown_model_is_assumed_blind() {
        for m in [
            "",
            "mystery-model-9",
            "llama3",
            "claude-2",
            "gpt-3.5-turbo",
            "openrouter/free",
        ] {
            assert!(!model_can_see(m), "`{m}` was assumed to accept images");
        }
    }

    #[test]
    fn known_vision_models_are_recognised_including_proxy_prefixed_ids() {
        for m in [
            "claude-sonnet-4-5-20250929",
            "claude-haiku-4-5-20251001",
            "claude-opus-4-1",
            "claude-3-5-sonnet-20241022",
            "anthropic/claude-sonnet-4-5",
            "gpt-4o",
            "openai/gpt-4o-mini",
            "CLAUDE-SONNET-4-5-20250929",
        ] {
            assert!(model_can_see(m), "`{m}` was not recognised");
        }
    }

    /// The model `hud_field_scout` actually declares must be able to see, or its
    /// camera path is decoration.
    #[test]
    fn the_hud_agents_declared_rungs_can_see() {
        let raw = std::fs::read_to_string("agents/curated/hud_field_scout/agent_card.json")
            .expect("read hud_field_scout card");
        let card: serde_json::Value = serde_json::from_str(&raw).expect("parse card");
        let ladder = card
            .pointer("/capabilities/model_ladder")
            .and_then(|v| v.as_array())
            .expect("model_ladder");
        assert!(!ladder.is_empty(), "no rungs to check");
        for rung in ladder {
            let model = rung.get("model").and_then(|v| v.as_str()).unwrap_or("");
            let tier = rung.get("tier").and_then(|v| v.as_str()).unwrap_or("?");
            assert!(
                model_can_see(model),
                "hud_field_scout's `{tier}` rung is `{model}`, which is not known \
                 to accept images — so a frame sent at that tier would be refused \
                 by ensure_deliverable. Either drop the rung or verify the model."
            );
        }
    }

    // ─── validation ────────────────────────────────────────────────────

    #[test]
    fn only_allowlisted_media_types_pass() {
        for ok in ALLOWED_MEDIA_TYPES {
            let a = ImageAttachment::new(*ok, "AAAAAAAAAAAA", None);
            assert_eq!(a.validate(), Ok(()), "{ok} was rejected");
        }
        for bad in ["image/tiff", "image/svg+xml", "image/*", "text/plain", ""] {
            let a = ImageAttachment::new(bad, "AAAAAAAAAAAA", None);
            assert!(
                matches!(
                    a.validate(),
                    Err(AttachmentError::UnsupportedMediaType { .. })
                ),
                "`{bad}` was accepted"
            );
        }
    }

    #[test]
    fn media_type_is_normalised() {
        let a = ImageAttachment::new("  IMAGE/JPEG  ", "AAAAAAAAAAAA", None);
        assert_eq!(a.media_type, "image/jpeg");
        assert_eq!(a.validate(), Ok(()));
    }

    /// A data URL is the commonest shape an image arrives in, so it is handled
    /// rather than rejected.
    #[test]
    fn a_data_url_prefix_is_stripped() {
        let a = ImageAttachment::new("image/png", "data:image/png;base64,AAAAAAAAAAAA", None);
        assert_eq!(a.data_base64, "AAAAAAAAAAAA");
        assert_eq!(a.validate(), Ok(()));
    }

    /// The realistic failure for a large frame over a flaky link.
    #[test]
    fn a_truncated_payload_is_caught() {
        let a = ImageAttachment::new("image/jpeg", "AAAAAAAAAAA", None); // 11 chars
        assert!(matches!(
            a.validate(),
            Err(AttachmentError::Malformed { .. })
        ));
    }

    #[test]
    fn a_non_base64_payload_is_caught() {
        let a = ImageAttachment::new("image/jpeg", "AAAA!AAAAAAA", None);
        match a.validate() {
            Err(AttachmentError::Malformed { why }) => assert!(why.contains('!')),
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn an_empty_payload_is_caught() {
        let a = ImageAttachment::new("image/jpeg", "", None);
        assert!(matches!(
            a.validate(),
            Err(AttachmentError::Malformed { .. })
        ));
    }

    #[test]
    fn decoded_length_accounts_for_padding() {
        assert_eq!(
            ImageAttachment::new("image/png", "AAAA", None).decoded_len(),
            3
        );
        assert_eq!(
            ImageAttachment::new("image/png", "AAA=", None).decoded_len(),
            2
        );
        assert_eq!(
            ImageAttachment::new("image/png", "AA==", None).decoded_len(),
            1
        );
        assert_eq!(ImageAttachment::new("image/png", "", None).decoded_len(), 0);
    }

    /// Expected for an unresized 4K frame, which is what the target hardware
    /// produces, so the error has to be legible rather than a provider 400.
    #[test]
    fn an_oversized_frame_is_refused_with_its_size() {
        let chars = (MAX_DECODED_BYTES / 3 + 16) * 4;
        let a = ImageAttachment::new("image/jpeg", &"A".repeat(chars), None);
        match a.validate() {
            Err(AttachmentError::TooLarge { decoded_bytes }) => {
                assert!(decoded_bytes > MAX_DECODED_BYTES);
                assert!(a.validate().unwrap_err().to_string().contains("resize"));
            }
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    // ─── the tables ────────────────────────────────────────────────────

    /// Same discipline `grounding_trust` applies to its own tables: an entry
    /// nobody justified is one the next author copies without thinking.
    #[test]
    fn every_vision_entry_explains_itself() {
        for (prefix, why) in VISION_CAPABLE {
            assert!(!prefix.is_empty(), "empty prefix would match everything");
            assert!(
                why.len() >= 40,
                "`{prefix}` has a {}-char justification; say how its image \
                 support was established",
                why.len()
            );
            assert_eq!(
                *prefix,
                prefix.to_ascii_lowercase(),
                "`{prefix}` can never match, since lookup lowercases"
            );
        }
    }

    /// Every error must say what to do next. An error a caller cannot act on
    /// gets caught and ignored, which reintroduces the silent drop.
    #[test]
    fn every_error_is_actionable() {
        let errs = [
            AttachmentError::UnsupportedMediaType {
                declared: "image/tiff".into(),
            },
            AttachmentError::Malformed {
                why: "empty".into(),
            },
            AttachmentError::TooLarge {
                decoded_bytes: 9_000_000,
            },
            AttachmentError::ModelCannotSee {
                model: "mystery".into(),
            },
            AttachmentError::ProviderCannotCarry {
                provider: "openrouter".into(),
                detail: "no multi-block content".into(),
            },
        ];
        for e in errs {
            let msg = e.to_string();
            assert!(msg.len() > 60, "terse error: {msg}");
            assert!(
                msg.contains("efus") || msg.contains("annot") || msg.contains("esize"),
                "error does not say it refused or why: {msg}"
            );
        }
    }
}
