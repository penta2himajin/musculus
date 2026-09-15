//! Engine selection — the TTS-side mirror of euhadra's router.
//!
//! Consumers (the CLI, the evaluation runners) describe the engine they
//! want and get a boxed [`TtsAdapter`] back, so the call sites do not
//! grow an engine-specific branch each time one is added. The ja
//! posture this factory encodes is the one ADR-0005 decided: SBV2JE is
//! the realtime default, Irodori-TTS the quality choice.

use std::path::PathBuf;

use crate::traits::{TtsAdapter, TtsError};

/// Which synthesis engine to build.
#[derive(Debug, Clone, PartialEq)]
pub enum Engine {
    /// Style-Bert-VITS2 JP-Extra — a directory of `*.sbv2` voices.
    Sbv2 {
        /// Directory holding `tokenizer.json`, `deberta.onnx`, `*.sbv2`.
        dir: PathBuf,
        /// User-owned accent overrides (ADR-0006); empty = frontend only.
        accent: crate::accent::AccentTable,
        /// Enable musculus's deliberate accent deviations (opt-in).
        accent_deviations: bool,
        /// Optional standard-accent user dictionary (offline generated).
        user_dictionary: Option<PathBuf>,
        /// Voice id (a `.sbv2` file stem); `None` = the first voice.
        voice: Option<String>,
        /// Style id within the voice's style table.
        style_id: i32,
        /// Style blend weight (0 = neutral mean, 1 = raw style).
        style_weight: f32,
    },
    /// Irodori-TTS — ONNX artifacts plus a reference voice WAV.
    #[cfg(feature = "wav")]
    Irodori {
        /// Directory holding `onnx/` and `tokenizer/llmjp_tok/`.
        dir: PathBuf,
        /// Reference voice: any mono WAV, resampled and LUFS-normalized.
        ref_wav: PathBuf,
        /// Rectified-flow Euler steps (40 = the released default).
        steps: usize,
        /// Sampling seed for the RF noise.
        seed: u32,
    },
}

impl Engine {
    /// The engine's short name, as used on the command line.
    pub fn name(&self) -> &'static str {
        match self {
            Engine::Sbv2 { .. } => "sbv2",
            #[cfg(feature = "wav")]
            Engine::Irodori { .. } => "irodori",
        }
    }

    /// The voice id callers should put on the segment, if the engine
    /// selects voices by id (Irodori takes its voice from the reference
    /// audio instead, so it returns `None`).
    pub fn voice_hint(&self) -> Option<String> {
        match self {
            Engine::Sbv2 { voice, .. } => voice.clone(),
            #[cfg(feature = "wav")]
            Engine::Irodori { .. } => None,
        }
    }

    /// The directory this engine reads its models from (for messages).
    pub fn model_dir(&self) -> &std::path::Path {
        match self {
            Engine::Sbv2 { dir, .. } => dir,
            #[cfg(feature = "wav")]
            Engine::Irodori { dir, .. } => dir,
        }
    }

    /// Build the adapter.
    pub fn build(&self) -> Result<Box<dyn TtsAdapter>, TtsError> {
        match self {
            Engine::Sbv2 {
                dir,
                style_id,
                style_weight,
                accent,
                accent_deviations,
                user_dictionary,
                ..
            } => {
                let adapter = crate::sbv2::Sbv2Adapter::load_dir_with_user_dictionary(
                    dir,
                    user_dictionary.as_deref(),
                )?
                .with_style_id(*style_id)
                .with_style_weight(*style_weight)
                .with_accent_deviations(*accent_deviations)
                .with_accent_table(accent.clone());
                Ok(Box::new(adapter))
            }
            #[cfg(feature = "wav")]
            Engine::Irodori {
                dir,
                ref_wav,
                steps,
                seed,
            } => {
                let adapter = crate::irodori::IrodoriAdapter::load(dir, ref_wav)?
                    .with_num_steps(*steps)
                    .with_seed(*seed);
                Ok(Box::new(adapter))
            }
        }
    }
}

/// Default model directory for each engine (`--dir` is resolved by the
/// caller when it is omitted, because the default differs per engine).
pub const SBV2_DEFAULT_DIR: &str = "vendor/sbv2";
/// Default Irodori artifact directory.
pub const IRODORI_DEFAULT_DIR: &str = "vendor/irodori";
/// Default reference voice for the Irodori engine.
pub const IRODORI_DEFAULT_REF: &str = "vendor/irodori-ref.wav";
/// Default Euler steps for Irodori (the released configuration).
pub const IRODORI_DEFAULT_STEPS: usize = 40;

/// Parse an engine name from the command line.
pub fn parse_engine_name(name: &str) -> Result<(), String> {
    match name {
        "sbv2" => Ok(()),
        #[cfg(feature = "wav")]
        "irodori" => Ok(()),
        #[cfg(not(feature = "wav"))]
        "irodori" => Err("the irodori engine needs the wav feature".into()),
        other => Err(format!(
            "unknown engine {other:?} (expected \"sbv2\"{})",
            if cfg!(feature = "wav") {
                " or \"irodori\""
            } else {
                ""
            }
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_voice_hints() {
        let sbv2 = Engine::Sbv2 {
            dir: PathBuf::from("vendor/sbv2"),
            voice: Some("tsukuyomi".into()),
            style_id: 0,
            style_weight: 1.0,
            accent: Default::default(),
            accent_deviations: false,
            user_dictionary: None,
        };
        assert_eq!(sbv2.name(), "sbv2");
        assert_eq!(sbv2.voice_hint().as_deref(), Some("tsukuyomi"));

        #[cfg(feature = "wav")]
        {
            let irodori = Engine::Irodori {
                dir: PathBuf::from("vendor/irodori"),
                ref_wav: PathBuf::from("vendor/irodori-ref.wav"),
                steps: IRODORI_DEFAULT_STEPS,
                seed: 0,
            };
            assert_eq!(irodori.name(), "irodori");
            assert_eq!(irodori.voice_hint(), None);
        }
    }

    #[test]
    fn missing_models_are_a_model_load_error() {
        let engine = Engine::Sbv2 {
            dir: PathBuf::from("definitely/not/here"),
            voice: None,
            style_id: 0,
            style_weight: 1.0,
            accent: Default::default(),
            accent_deviations: false,
            user_dictionary: None,
        };
        assert!(matches!(engine.build(), Err(TtsError::ModelLoad(_))));

        #[cfg(feature = "wav")]
        {
            let engine = Engine::Irodori {
                dir: PathBuf::from("definitely/not/here"),
                ref_wav: PathBuf::from("also/missing.wav"),
                steps: IRODORI_DEFAULT_STEPS,
                seed: 0,
            };
            assert!(matches!(engine.build(), Err(TtsError::ModelLoad(_))));
        }
    }

    #[test]
    fn engine_names_parse() {
        assert!(parse_engine_name("sbv2").is_ok());
        assert!(parse_engine_name("nope").is_err());
        #[cfg(feature = "wav")]
        assert!(parse_engine_name("irodori").is_ok());
    }
}
