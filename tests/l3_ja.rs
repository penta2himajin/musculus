//! L3 evaluation: reading accuracy of the Japanese frontend against
//! annotated gold (docs/evaluation.md §3).
//!
//! Pure text — no model bundle needed; runs wherever the `onnx`
//! feature compiles (jpreprocess is bundled). The metric is per-item
//! phoneme-sequence equality after punctuation stripping; comparison
//! happens at the phoneme level so spelling variants that sound
//! identical (「ジュウ」 vs 「ジュー」) count as equal.
//!
//! Annotations carry a `gap` field for readings the frontend cannot
//! produce yet (measured 2026-09-13: minus-sign, date-compound,
//! colon-time, currency-symbol, percent-symbol, unit-letter,
//! latin-letters — the M3+ worklist). The gate holds the line on
//! non-gap items at 100%; gap items are reported as the worklist and
//! are expected to flip to passing as normalization work lands.

#![cfg(feature = "onnx")]

use std::collections::BTreeMap;

use musculus::prelude::SpeechNormalizer as _;
use musculus::sbv2::ja::{self, JaFrontend};
use musculus::sbv2::symbols::PUNCTUATIONS;
use serde::Deserialize;

#[derive(Deserialize)]
struct Gold {
    input: String,
    reading: String,
    category: String,
    #[serde(default)]
    gap: Option<String>,
}

/// The frontend reading for one input, as katakana (punctuation kept).
fn frontend_reading(frontend: &JaFrontend, input: &str) -> Result<String, String> {
    // Mirror the production chain: JaNormalizer -> num2word -> ...
    let normalized = musculus::sbv2::ja_norm::JaNormalizer::new()
        .normalize(input)
        .map_err(|e| e.to_string())?
        .text;
    let read = frontend.num2word(&normalized).map_err(|e| e.to_string())?;
    let normalized = musculus::sbv2::normalize::normalize_text(&read);
    let process = frontend
        .process_text(&normalized)
        .map_err(|e| e.to_string())?;
    let (phones, _tones, _word2ph) = process.g2p().map_err(|e| e.to_string())?;
    ja::phones_to_kana(&phones).map_err(|e| e.to_string())
}

fn strip_punctuation(text: &str) -> String {
    text.chars()
        .filter(|c| !PUNCTUATIONS.contains(&c.to_string().as_str()) && *c != '_' && *c != '\'')
        .collect()
}

/// Katakana string → phoneme sequence via the mora table (the same
/// splitter the frontend uses for readings, so both sides are in the
/// same space).
fn to_phonemes(katakana: &str) -> Option<Vec<String>> {
    let kana = strip_punctuation(katakana);
    if kana.is_empty() {
        return Some(Vec::new());
    }
    ja::kata_to_phoneme_list(kana).ok()
}

#[test]
fn l3_ja_reading_accuracy() {
    let annotations = include_str!("evaluation/annotations/ja.jsonl");
    let gold: Vec<Gold> = annotations
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("annotation line is valid JSON"))
        .collect();

    let frontend = JaFrontend::new().expect("ja frontend");
    let mut per_category: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // (correct, total)
    let mut failures: Vec<String> = Vec::new();
    let mut correct_total = 0;

    for item in &gold {
        let produced = frontend_reading(&frontend, &item.input);
        let (got, expected) = match &produced {
            Ok(kana) => (
                to_phonemes(kana),
                to_phonemes(&ja::hiragana_to_katakana(&item.reading)),
            ),
            Err(_) => (None, Some(Vec::new())),
        };
        let ok = got.is_some() && got == expected;
        let entry = per_category.entry(item.category.clone()).or_insert((0, 0));
        entry.1 += 1;
        if ok {
            entry.0 += 1;
            correct_total += 1;
        } else if item.gap.is_none() {
            failures.push(format!(
                "[{}] {:?}: expected {:?}, got {:?} (raw: {:?})",
                item.category,
                item.input,
                item.reading,
                produced.as_ref().map(|k| strip_punctuation(k)),
                produced
            ));
        }
    }

    // Report: per-category accuracy, then the gap worklist.
    for (category, (correct, total)) in &per_category {
        println!("{category}: {correct}/{total}");
    }
    let gap_items: Vec<&Gold> = gold.iter().filter(|g| g.gap.is_some()).collect();
    let gap_passing = gap_items
        .iter()
        .filter(|g| {
            let produced = frontend_reading(&frontend, &g.input);
            match produced {
                Ok(kana) => {
                    to_phonemes(&kana) == to_phonemes(&ja::hiragana_to_katakana(&g.reading))
                }
                Err(_) => false,
            }
        })
        .count();
    println!(
        "L3 ja: {correct_total}/{} = {:.3} (gap worklist: {}/{} items closed)",
        gold.len(),
        correct_total as f64 / gold.len() as f64,
        gap_passing,
        gap_items.len()
    );
    for g in &gap_items {
        println!("gap [{}]: {}", g.category, g.gap.as_deref().unwrap_or(""));
    }
    for failure in &failures {
        println!("MISMATCH {failure}");
    }

    // Gate: items without a known gap must all pass — these are the
    // frontend's proven-correct behaviours; a failure here is a real
    // regression.
    assert!(
        failures.is_empty(),
        "non-gap reading accuracy regressed: {failures:?}"
    );
    println!(
        "gap worklist: {} items still open (see docs/benchmarks/l3-ja/baseline.json)",
        gap_items.len() - gap_passing
    );
}

/// The same L3 set measured **with the fixture dictionary applied**
/// (tests/evaluation/fixtures/dict.json — measurement apparatus, not a
/// shipped opinion). The dictionary is a pipeline stage that runs
/// before the frontend; it must close the latin-letters gap and may
/// not disturb a single proven-correct reading.
#[test]
fn l3_ja_reading_accuracy_with_dictionary() {
    use musculus::dictionary::{MatchPolicy, TermDictionary};
    use musculus::traits::TextProcessor as _;

    let annotations = include_str!("evaluation/annotations/ja.jsonl");
    let gold: Vec<Gold> = annotations
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("annotation line is valid JSON"))
        .collect();
    let dict_entries: Vec<musculus::dictionary::TermEntry> =
        serde_json::from_str(include_str!("evaluation/fixtures/dict.json")).expect("fixture dict");
    let dictionary =
        TermDictionary::new(dict_entries, MatchPolicy::for_japanese()).expect("fixture dict");

    let frontend = JaFrontend::new().expect("ja frontend");
    let mut correct = 0;
    let mut failures: Vec<String> = Vec::new();

    for item in &gold {
        let rewritten = dictionary.process(&item.input).expect("dict process");
        let produced = frontend_reading(&frontend, &rewritten.text);
        let (got, expected) = match &produced {
            Ok(kana) => (
                to_phonemes(kana),
                to_phonemes(&ja::hiragana_to_katakana(&item.reading)),
            ),
            Err(_) => (None, Some(Vec::new())),
        };
        let ok = got.is_some() && got == expected;
        if ok {
            correct += 1;
        } else {
            failures.push(format!(
                "[{}] {:?}: expected {:?}, got {:?}",
                item.category,
                item.input,
                item.reading,
                produced.as_ref().map(|k| strip_punctuation(k))
            ));
        }
    }

    println!(
        "L3 ja with dictionary: {correct}/{} = {:.3}",
        gold.len(),
        correct as f64 / gold.len() as f64
    );
    for failure in &failures {
        println!("MISMATCH {failure}");
    }

    // The coined-Latin gap must close with the fixture dictionary.
    let musculus = gold
        .iter()
        .find(|g| g.input == "musculus")
        .expect("musculus entry");
    let rewritten = dictionary.process(&musculus.input).expect("dict process");
    let produced = frontend_reading(&frontend, &rewritten.text).expect("reading");
    assert!(
        to_phonemes(&produced) == to_phonemes(&ja::hiragana_to_katakana(&musculus.reading)),
        "dictionary must close the latin-letters gap: got {produced:?}"
    );

    // No item that passed without the dictionary may regress.
    for item in &gold {
        let raw = frontend_reading(&frontend, &item.input);
        let ok_without = match &raw {
            Ok(kana) => to_phonemes(kana) == to_phonemes(&ja::hiragana_to_katakana(&item.reading)),
            Err(_) => false,
        };
        if ok_without {
            let rewritten = dictionary.process(&item.input).expect("dict process");
            let produced = frontend_reading(&frontend, &rewritten.text);
            let ok_with = match &produced {
                Ok(kana) => {
                    to_phonemes(kana) == to_phonemes(&ja::hiragana_to_katakana(&item.reading))
                }
                Err(_) => false,
            };
            assert!(
                ok_with,
                "dictionary regressed a proven-correct item: {:?}",
                item.input
            );
        }
    }
}
