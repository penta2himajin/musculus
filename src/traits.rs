//! The adapter traits — the part of musculus meant to be stable.
//!
//! Implementing one is the reason to depend on this crate. They will
//! still change if a real integration shows they are wrong; everything
//! around them (the concrete adapters, the CLI, the evaluation harness)
//! is fluid until `1.0`.
//!
//! Streaming synthesis is deliberately absent. An adapter that can emit
//! audio as it synthesizes will implement an additional trait alongside
//! `TtsAdapter`; like euhadra's streaming-ASR posture, batch correctness
//! and measurement come first (see `docs/spec.md` §2, 非目標).

use async_trait::async_trait;

use crate::types::*;

// ---------------------------------------------------------------------------
// Tts Adapter
// ---------------------------------------------------------------------------

/// Turns text into audio.
///
/// Implementors may be local (SBV2JE and Irodori via ONNX, later
/// milestones) or cloud-based. The pipeline treats them identically.
///
/// The whole request is passed at once and the full audio comes back —
/// the batch counterpart of euhadra's `AsrAdapter::transcribe`. An
/// adapter that can also stream will do so through an additional trait;
/// this shape is deliberately the one every backend can satisfy.
#[async_trait]
pub trait TtsAdapter: Send + Sync {
    /// Synthesize the given segments, in order.
    ///
    /// Implementors may process the segments jointly; the split between
    /// segments carries prosodic intent (sentence boundaries, speaker
    /// changes) but imposes no acoustic promise. Returning one chunk per
    /// segment keeps chunk boundaries auditable without constraining
    /// adapters that must resynthesize jointly.
    async fn synthesize(&self, segments: &[SpeechSegment]) -> Result<Synthesis, TtsError>;
}

/// Why a [`TtsAdapter`] could not produce audio.
///
/// The variants are the distinctions a caller can act on: a missing
/// model needs a different response than a runtime failure, and neither
/// is the same as the caller having passed no text. Marked
/// `#[non_exhaustive]` so finer distinctions can be added later without
/// breaking a `match`.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum TtsError {
    /// The model bundle could not be loaded — a missing file, a
    /// malformed vocabulary, an unreadable style matrix.
    #[error("failed to load model: {0}")]
    ModelLoad(String),

    /// The adapter was asked for something it cannot do, e.g. an
    /// unknown voice id or a length scale outside its supported range.
    #[error("invalid TTS configuration: {0}")]
    Config(String),

    /// No text reached the adapter.
    #[error("no text received")]
    NoText,

    /// The model loaded but inference failed.
    #[error("synthesis failed: {0}")]
    Inference(String),

    /// The request is structurally beyond this adapter — e.g. a voice
    /// or language it was never built for, rather than a misconfiguration.
    #[error("the adapter does not support this request: {0}")]
    Unsupported(String),

    /// Aborted before completion.
    #[error("cancelled")]
    Cancelled,
}

// ---------------------------------------------------------------------------
// Text Normalization
// ---------------------------------------------------------------------------

/// Rewrites written text into speakable form before synthesis.
///
/// The inverse of euhadra's `InverseTextNormalizer`: expands numerals,
/// symbols, dates and abbreviations into the form the synthesizer should
/// speak (e.g. 「3.14」→「さんてんいちよん」). Per-language rule work,
/// grounded in annotated gold sets (`tests/evaluation/annotations/`) —
/// never an LLM.
///
/// This is a synchronous trait: normalization is CPU-bound text
/// rewriting with no I/O, so async would only add ceremony.
pub trait SpeechNormalizer: Send + Sync {
    /// Normalize one unit of text.
    ///
    /// Returns the rewritten text plus every substitution as a
    /// codepoint-spanned correction. An empty input is valid and
    /// normalizes to an empty output; adapters that need the
    /// surrounding utterance for context take it through their own
    /// configuration, not through this signature.
    fn normalize(&self, input: &str) -> Result<NormalizedText, NormalizerError>;
}

/// Why a [`SpeechNormalizer`] could not normalize.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum NormalizerError {
    /// The normalizer was configured with something unusable — a
    /// malformed dictionary, an unknown language.
    #[error("invalid normalizer configuration: {0}")]
    Config(String),

    /// The normalization pass itself failed.
    #[error("normalization failed: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// Text Processing
// ---------------------------------------------------------------------------

/// Post-normalization text adjustment — the Tier 2 stage.
///
/// The mirror of euhadra's `TextProcessor`: user dictionaries, term
/// replacement, style-neutral rewrites that need the whole normalized
/// text rather than one token. musculus owns the behaviour, not the
/// dictionary: entries arrive from the consuming application.
///
/// Reserved for M3; defined now so the pipeline shape is fixed early.
pub trait TextProcessor: Send + Sync {
    /// Process one normalized unit of text.
    fn process(&self, input: &str) -> Result<NormalizedText, NormalizerError>;
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// Delivers synthesized audio to its destination.
///
/// Implementors handle speaker playback, WAV files, pipes, or any other
/// output mechanism — the mirror of euhadra's `OutputEmitter`, where
/// "clipboard" becomes "speaker".
#[async_trait]
pub trait AudioEmitter: Send + Sync {
    /// Emit the synthesis to the target.
    async fn emit(&self, synthesis: &Synthesis) -> Result<(), EmitError>;
}

/// Why an [`AudioEmitter`] could not emit.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum EmitError {
    /// Playback failed — no device, device busy, or the host API
    /// refused the stream configuration.
    #[error("playback failed: {0}")]
    Playback(String),

    /// Writing to a file or stream failed.
    #[error("audio output failed: {0}")]
    Write(String),

    /// Aborted before completion.
    #[error("cancelled")]
    Cancelled,
}
