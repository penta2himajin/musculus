//! Domain types shared by every adapter and the pipeline runtime.
//!
//! The shapes mirror euhadra's types where the direction is invertible
//! (`AudioChunk` is the same concept travelling the other way), and stay
//! deliberately minimal where TTS-specific requirements have not been
//! demonstrated yet — style/caption conditioning will move `SpeechSegment`
//! once a real integration shows the right form, exactly like euhadra's
//! `Command`/`StructuredInput` modes are expected to move `LlmRefiner`.

use std::ops::Range;
use std::time::Duration;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

/// A contiguous span of audio in capture/synthesis order.
///
/// euhadra's `AudioChunk` travelling the other direction. Adapters may
/// produce one chunk per requested segment; emitters and writers should
/// not assume chunk boundaries carry prosodic meaning.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    /// Samples in [-1.0, 1.0] nominal range, interleaved mono.
    pub samples: Vec<f32>,
    /// Sample rate in Hz (e.g. 24_000 for SBV2-family models, 48_000
    /// for DACVAE-based ones). Every chunk carries its own rate so a
    /// `Synthesis` never has to guess.
    pub sample_rate: u32,
}

impl AudioChunk {
    /// Duration of this chunk.
    pub fn duration(&self) -> Duration {
        Duration::from_secs_f64(self.samples.len() as f64 / self.sample_rate as f64)
    }
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// One unit of text to synthesize.
///
/// Minimal by design for `0.x`: the text plus an optional voice id.
/// Style vectors (SBV2), captions and reference audio (Irodori) are real
/// adapter concepts, but wiring them into the public type before an
/// adapter exists would freeze the wrong shape. Implementors that need
/// more conditioning take it through adapter-specific configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeechSegment {
    /// The text to speak, after normalization.
    pub text: String,
    /// Voice identifier, in whatever form the chosen adapter uses
    /// (a model name, a style id, a speaker embedding reference).
    pub voice: Option<String>,
}

impl SpeechSegment {
    /// A segment speaking `text` with the adapter's default voice.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            voice: None,
        }
    }

    /// Builder-style voice setter.
    pub fn with_voice(mut self, voice: impl Into<String>) -> Self {
        self.voice = Some(voice.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

/// What a [`TtsAdapter`](crate::traits::TtsAdapter) produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Synthesis {
    /// Audio, one chunk per requested segment, in input order.
    pub audio: Vec<AudioChunk>,
}

impl Synthesis {
    /// Total duration across every chunk.
    pub fn duration(&self) -> Duration {
        self.audio
            .iter()
            .map(AudioChunk::duration)
            .fold(Duration::ZERO, |a, b| a + b)
    }

    /// The sample rate of the synthesis, if any audio was produced.
    pub fn sample_rate(&self) -> Option<u32> {
        self.audio.first().map(|c| c.sample_rate)
    }
}

// ---------------------------------------------------------------------------
// Normalization
// ---------------------------------------------------------------------------

/// The result of [`SpeechNormalizer::normalize`](crate::traits::SpeechNormalizer::normalize).
///
/// Carries the rewritten text plus every substitution as a
/// codepoint-spanned correction, so callers can show, audit or undo
/// the rewrite — the same reporting contract as euhadra's
/// `TermDictionary::Correction`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedText {
    /// The normalized text.
    pub text: String,
    /// Every substitution, in source order.
    pub corrections: Vec<Correction>,
}

/// A single substitution performed during normalization.
///
/// `span` indexes **codepoints** (Rust `char`s) into
/// [`NormalizedText::text`], not bytes, so UIs can slice safely.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    /// The replacement's position in the normalized text.
    pub span: Range<usize>,
    /// The original fragment.
    pub from: String,
    /// What it became.
    pub to: String,
}
