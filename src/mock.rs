//! Mock adapters behind the `testing` feature.
//!
//! Deterministic, dependency-free doubles for building tests against
//! musculus's traits — the same posture as euhadra's `testing` feature.
//! They are how musculus is tested and how a consumer should test their
//! own pipeline stages; they are not library surface, which is why the
//! feature is off by default and belongs under `[dev-dependencies]`.

use std::sync::Mutex;

use async_trait::async_trait;

use crate::traits::{
    AudioEmitter, EmitError, NormalizerError, SpeechNormalizer, TtsAdapter, TtsError,
};
use crate::types::{AudioChunk, NormalizedText, SpeechSegment, Synthesis};

// ---------------------------------------------------------------------------
// TtsAdapter
// ---------------------------------------------------------------------------

/// A deterministic [`TtsAdapter`] that synthesizes a sine wave whose
/// length is proportional to the character count.
///
/// Deterministic and dependency-free: every call with the same input
/// produces the same samples, so tests can assert on sample counts and
/// durations exactly. The audio itself is a quiet sine — non-silent, so
/// tests can also assert that output is audible, but not meant to be
/// listened to.
pub struct MockTts {
    sample_rate: u32,
    seconds_per_char: f64,
    frequency_hz: f32,
    amplitude: f32,
}

impl MockTts {
    /// A mock synthesizing at `sample_rate`, defaulting to 100 ms per
    /// character.
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            seconds_per_char: 0.1,
            frequency_hz: 440.0,
            amplitude: 0.25,
        }
    }

    /// Builder-style per-character duration setter.
    pub fn with_seconds_per_char(mut self, seconds_per_char: f64) -> Self {
        self.seconds_per_char = seconds_per_char;
        self
    }

    fn render(&self, text: &str) -> AudioChunk {
        let n = (text.chars().count() as f64 * self.seconds_per_char * self.sample_rate as f64)
            .ceil() as usize;
        let samples = (0..n)
            .map(|i| {
                self.amplitude
                    * (2.0 * std::f32::consts::PI * self.frequency_hz * i as f32
                        / self.sample_rate as f32)
                        .sin()
            })
            .collect();
        AudioChunk {
            samples,
            sample_rate: self.sample_rate,
        }
    }
}

#[async_trait]
impl TtsAdapter for MockTts {
    async fn synthesize(&self, segments: &[SpeechSegment]) -> Result<Synthesis, TtsError> {
        if segments.is_empty() || segments.iter().all(|s| s.text.is_empty()) {
            return Err(TtsError::NoText);
        }
        let audio = segments.iter().map(|s| self.render(&s.text)).collect();
        Ok(Synthesis { audio })
    }
}

// ---------------------------------------------------------------------------
// SpeechNormalizer
// ---------------------------------------------------------------------------

/// The identity [`SpeechNormalizer`] — passes text through untouched.
///
/// Useful as the neutral element in pipeline tests: any delta between a
/// run with and without a real normalizer is that normalizer's doing.
#[derive(Debug, Clone, Default)]
pub struct MockNormalizer;

impl SpeechNormalizer for MockNormalizer {
    fn normalize(&self, input: &str) -> Result<NormalizedText, NormalizerError> {
        Ok(NormalizedText {
            text: input.to_string(),
            corrections: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// AudioEmitter
// ---------------------------------------------------------------------------

/// An [`AudioEmitter`] that records every synthesis it receives.
///
/// The test double for asserting on what would have been played or
/// written — `emitted()` returns the collected syntheses in emission
/// order.
#[derive(Debug, Default)]
pub struct MockEmitter {
    inner: Mutex<Vec<Synthesis>>,
}

impl MockEmitter {
    /// The syntheses emitted so far, in emission order.
    pub fn emitted(&self) -> Vec<Synthesis> {
        self.inner
            .lock()
            .expect("mock emitter mutex poisoned")
            .clone()
    }
}

#[async_trait]
impl AudioEmitter for MockEmitter {
    async fn emit(&self, synthesis: &Synthesis) -> Result<(), EmitError> {
        self.inner
            .lock()
            .expect("mock emitter mutex poisoned")
            .push(synthesis.clone());
        Ok(())
    }
}
