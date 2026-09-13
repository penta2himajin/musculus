//! musculus — a programmable text-to-speech framework.
//!
//! The mirror image of euhadra: where euhadra turns speech into clean,
//! formatted text through composable adapters, musculus turns text into
//! speech the same way:
//!
//! ```text
//! テキスト入力
//!     → SpeechNormalizer   (読み展開: 数値・記号・日付・漢字読み)
//!     → TextProcessor      (ユーザ辞書・表記揺れ)
//!     → TtsAdapter         (ローカル合成エンジン: ONNX)
//!     → AudioEmitter       (再生 / WAV / stdout)
//! ```
//!
//! Each stage is a Rust trait. Swap any component without touching the
//! rest. The default build is deliberately lean — no ML runtime and no
//! system libraries; synthesis adapters live behind the `onnx` feature.
//!
//! See `docs/spec.md` for the architecture, `docs/evaluation.md` for the
//! measurement policy, and `docs/decisions/` for the ADRs.

pub mod prelude;
pub mod traits;
pub mod types;

// ONNX synthesis adapters (SBV2 JP-Extra baseline, ADR-0002/0003).
#[cfg(feature = "onnx")]
pub mod sbv2;

// Mocks are how musculus is tested, not how it is used — same posture
// as euhadra's `testing` feature. Enable it via [dev-dependencies].
#[cfg(feature = "testing")]
pub mod mock;

// WAV file I/O behind the `wav` feature (implied by `cli`).
#[cfg(feature = "wav")]
pub mod wav;
