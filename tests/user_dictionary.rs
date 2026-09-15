//! The dictionary integration point (docs/accent-resources.md).
//!
//! A user dictionary in jpreprocess format carries accent type, mora count
//! and chain rule, so it can override the system dictionary's accent
//! assignment. This test pins that mechanism, because the generated
//! standard-accent dictionary route (tdmelodic) depends on it.
//!
//! The fixture is our own two-line CSV and the 1 KB dictionary built from
//! it; regenerate both with:
//!
//! ```text
//! dict_tools build --user jpreprocess \
//!   tests/evaluation/fixtures/accent-userdict.csv \
//!   tests/evaluation/fixtures/accent-userdict.bin
//! ```
//!
//! A dictionary-format bump in jpreprocess will fail this test loudly,
//! which is the intent: the fixture is version-bound and must be rebuilt.

#![cfg(feature = "onnx")]

use musculus::prelude::SpeechNormalizer as _;
use musculus::sbv2::ja::{self, JaFrontend};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/evaluation/fixtures")
        .join(name)
}

/// Mora tones for an input through the production chain (deviations off, so
/// only the dictionary difference shows).
fn tones(frontend: &JaFrontend, input: &str) -> String {
    let normalized = musculus::sbv2::ja_norm::JaNormalizer::new()
        .normalize(input)
        .expect("normalizer")
        .text;
    let read = frontend.num2word(&normalized).expect("num2word");
    let normalized = musculus::sbv2::normalize::normalize_text(&read);
    let process = frontend.process_text(&normalized).expect("frontend");
    let (phones, tones, _word2ph) = process.g2p().expect("g2p");
    let pairs = ja::kana_tone(&phones, &tones).expect("mora view");
    ja::tone_string(&pairs)
}

#[test]
fn a_user_dictionary_entry_changes_the_accent() {
    let system = JaFrontend::new().expect("bundled frontend");
    let with_user = JaFrontend::with_user_dictionary(fixture("accent-userdict.bin"))
        .expect("frontend with user dictionary");

    // The fixture gives 千 accent 2/2 instead of the system's 1/2, which
    // moves the realised pattern in 千二百 and 千五百.
    let baseline = tones(&system, "千二百");
    let overridden = tones(&with_user, "千二百");
    assert_eq!(baseline, "HLLHH", "system dictionary baseline changed");
    assert_eq!(
        overridden, "LHLHH",
        "user dictionary accent did not take effect"
    );

    // The phrase head governs the realisation: 千 is not the head of
    // 二千二百, so changing its entry alone does not move the tones.
    assert_eq!(tones(&system, "二千二百"), tones(&with_user, "二千二百"));
}
