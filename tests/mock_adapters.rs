//! Integration tests over the `testing` feature's mock adapters.
//!
//! M0 scope: these pin the behaviour the real adapters must satisfy —
//! ordered segments in, one `Synthesis` out, deterministic sample
//! counts, `NoText` on empty input, and an emitter that records what
//! it received. They are the "red" that the M0 trait surface and
//! mocks are built against.

use std::time::Duration;

use musculus::mock::{MockEmitter, MockNormalizer, MockTts};
use musculus::prelude::*;
use musculus::types::Correction;

// ---------------------------------------------------------------------------
// TtsAdapter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_tts_produces_deterministic_sample_counts() {
    let tts = MockTts::new(24_000).with_seconds_per_char(0.1);

    let out = tts
        .synthesize(&[SpeechSegment::new("こんにちは")])
        .await
        .unwrap();

    // 5 chars * 0.1 s = 0.5 s at 24 kHz = 12 000 samples.
    assert_eq!(out.duration(), Duration::from_secs_f64(0.5));
    assert_eq!(out.audio.len(), 1);
    assert_eq!(out.audio[0].samples.len(), 12_000);
    assert_eq!(out.audio[0].sample_rate, 24_000);
}

#[tokio::test]
async fn mock_tts_preserves_segment_order_and_boundaries() {
    let tts = MockTts::new(8_000).with_seconds_per_char(0.1);

    let out = tts
        .synthesize(&[
            SpeechSegment::new("あ"),
            SpeechSegment::new("いうえお"),
            SpeechSegment::new("か"),
        ])
        .await
        .unwrap();

    assert_eq!(out.audio.len(), 3);
    let counts: Vec<usize> = out.audio.iter().map(|c| c.samples.len()).collect();
    assert_eq!(counts, vec![800, 3_200, 800]);
}

#[tokio::test]
async fn mock_tts_output_is_not_silent() {
    let tts = MockTts::new(16_000);

    let out = tts
        .synthesize(&[SpeechSegment::new("テスト")])
        .await
        .unwrap();

    let peak = out.audio[0]
        .samples
        .iter()
        .fold(0.0f32, |m, &s| m.max(s.abs()));
    assert!(peak > 0.1, "mock synthesis should produce audible samples");
}

#[tokio::test]
async fn mock_tts_reports_no_text_on_empty_input() {
    let tts = MockTts::new(16_000);

    let err = tts.synthesize(&[]).await.unwrap_err();
    assert!(matches!(err, TtsError::NoText));

    let err = tts.synthesize(&[SpeechSegment::new("")]).await.unwrap_err();
    assert!(matches!(err, TtsError::NoText));
}

// ---------------------------------------------------------------------------
// SpeechNormalizer
// ---------------------------------------------------------------------------

#[test]
fn mock_normalizer_is_the_identity_pass() {
    // Unit struct — clippy prefers direct construction over ::default().
    let normalizer = MockNormalizer;

    let out = normalizer.normalize("2026年2月14日").unwrap();

    assert_eq!(out.text, "2026年2月14日");
    assert!(out.corrections.is_empty());
}

#[test]
fn normalized_text_carries_correction_spans() {
    let out = NormalizedText {
        text: "千二百円".to_string(),
        corrections: vec![Correction {
            span: 0..4,
            from: "¥1,200".to_string(),
            to: "千二百円".to_string(),
        }],
    };

    assert_eq!(out.corrections.len(), 1);
    // Correction::span indexes codepoints, not bytes (see types.rs).
    let chars: Vec<char> = out.text.chars().collect();
    let sliced: String = chars[out.corrections[0].span.clone()].iter().collect();
    assert_eq!(sliced, "千二百円");
}

// ---------------------------------------------------------------------------
// AudioEmitter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mock_emitter_collects_every_synthesis() {
    let emitter = MockEmitter::default();
    let tts = MockTts::new(16_000);

    let first = tts.synthesize(&[SpeechSegment::new("一")]).await.unwrap();
    let second = tts.synthesize(&[SpeechSegment::new("二")]).await.unwrap();

    emitter.emit(&first).await.unwrap();
    emitter.emit(&second).await.unwrap();

    let emitted = emitter.emitted();
    assert_eq!(emitted.len(), 2);
    assert_eq!(
        emitted[0].audio[0].samples.len(),
        first.audio[0].samples.len()
    );
    assert_eq!(
        emitted[1].audio[0].samples.len(),
        second.audio[0].samples.len()
    );
}

// ---------------------------------------------------------------------------
// Domain invariants
// ---------------------------------------------------------------------------

#[test]
fn duration_of_empty_synthesis_is_zero() {
    let empty = Synthesis { audio: Vec::new() };
    assert_eq!(empty.duration(), Duration::ZERO);
    assert_eq!(empty.sample_rate(), None);
}

#[test]
fn speech_segment_builders_chain() {
    let seg = SpeechSegment::new("こんにちは").with_voice("tsukuyomi");

    assert_eq!(seg.text, "こんにちは");
    assert_eq!(seg.voice.as_deref(), Some("tsukuyomi"));
}
