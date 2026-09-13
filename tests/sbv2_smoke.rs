//! End-to-end SBV2 synthesis smoke test — the M1 acceptance criterion:
//! 「こんにちは」が WAV に出る.
//!
//! Requires the `onnx` feature and a model bundle laid out by
//! `scripts/setup_sbv2.sh` (vendor/sbv2). Explicitly `#[ignore]`d so
//! plain `cargo test` stays model-free; run with:
//!
//!     cargo test --features onnx --test sbv2_smoke -- --ignored

#![cfg(feature = "onnx")]

use std::time::Instant;

use musculus::prelude::*;
use musculus::sbv2::Sbv2Adapter;

const MODELS_DIR: &str = "vendor/sbv2";

fn models_present() -> bool {
    std::path::Path::new(MODELS_DIR)
        .join("tsukuyomi.sbv2")
        .is_file()
        && std::path::Path::new(MODELS_DIR)
            .join("deberta.onnx")
            .is_file()
}

#[tokio::test]
#[ignore = "requires the vendor/sbv2 model bundle; run scripts/setup_sbv2.sh first"]
async fn synthesizes_konnichiwa_to_wav_rate_audio() {
    assert!(models_present(), "run scripts/setup_sbv2.sh first");

    let adapter = Sbv2Adapter::load_dir(MODELS_DIR).expect("load models");
    assert_eq!(adapter.voice_names(), vec!["tsukuyomi".to_string()]);

    let segment = SpeechSegment::new("こんにちは");
    let t0 = Instant::now();
    let synthesis = adapter
        .synthesize(&[segment])
        .await
        .expect("synthesis of こんにちは");
    let elapsed = t0.elapsed().as_secs_f64();

    // Non-trivial audio at the documented decode rate.
    assert_eq!(synthesis.audio.len(), 1);
    assert_eq!(synthesis.sample_rate(), Some(44_100));
    let audio_secs = synthesis.duration().as_secs_f64();
    assert!(
        (0.3..3.0).contains(&audio_secs),
        "unexpected duration {audio_secs:.3} s for こんにちは"
    );
    let peak = synthesis.audio[0]
        .samples
        .iter()
        .fold(0.0f32, |m, &s| m.max(s.abs()));
    assert!(peak > 0.05, "synthesis is silent (peak {peak})");

    // RTF observation, not an assertion: the baseline lives in
    // docs/benchmarks/; this only records what this run measured.
    eprintln!(
        "smoke: {elapsed:.3} s wall, {audio_secs:.3} s audio, RTF {:.3}",
        elapsed / audio_secs
    );

    // Unknown voices are a user-facing error, not a crash.
    let err = adapter
        .synthesize(&[SpeechSegment::new("こんにちは").with_voice("unknown-voice")])
        .await
        .expect_err("unknown voice must be rejected");
    assert!(matches!(err, TtsError::Unsupported(_)));
}
