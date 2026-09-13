//! Convenience re-exports — `use musculus::prelude::*;` for the common
//! surface, the way euhadra's prelude works.

pub use crate::traits::{
    AudioEmitter, EmitError, NormalizerError, SpeechNormalizer, TextProcessor, TtsAdapter, TtsError,
};
pub use crate::types::{AudioChunk, Correction, NormalizedText, SpeechSegment, Synthesis};
