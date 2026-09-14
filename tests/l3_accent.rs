//! L3 accent regression gate (docs/evaluation.md §3.6).
//!
//! The accent layer has two jobs that pull in opposite directions: stay
//! faithful to the reference frontend, and deviate from it deliberately
//! where the reference disagrees with the standard norm
//! (docs/accent-resources.md). This test holds both lines:
//!
//! * **fidelity** — our baseline (deviations off) NJD node accents must
//!   match `ja_accent_reference.jsonl`, which is generated from pyopenjtalk
//!   (the OpenJTalk reference oracle) by
//!   `scripts/gen_accent_reference.py`. This catches accidental drift from
//!   a jpreprocess upgrade or a change in our frontend.
//! * **containment** — the deviation layer may only change the items we
//!   intend it to change: every item *without* `expected_tones` in
//!   `ja_accent.jsonl` must produce identical tones with and without the
//!   deviations, and every item *with* `expected_tones` must produce
//!   exactly those tones under the shipped configuration (deviations plus
//!   the sample override table).
//!
//! The chain-rule field is not compared: the oracle reports the chain flag
//! for some nodes where we report the rule, and the accent/mora pair we do
//! compare is the outcome of that machinery anyway.

#![cfg(feature = "onnx")]

use musculus::accent::AccentTable;
use musculus::prelude::SpeechNormalizer as _;
use musculus::sbv2::ja::{self, JaFrontend, NjdNode};
use serde::Deserialize;

#[derive(Deserialize)]
struct ReferenceNode {
    surface: String,
    pos: String,
    pron: String,
    accent: usize,
    mora_size: usize,
}

#[derive(Deserialize)]
struct ReferenceItem {
    input: String,
    nodes: Vec<ReferenceNode>,
}

#[derive(Deserialize)]
struct Annotation {
    input: String,
    #[serde(default)]
    expected_tones: Option<String>,
}

/// The mora tones of one input through the production chain
/// (JaNormalizer -> num2word -> normalize -> frontend -> [deviations] ->
/// g2p -> [override table] -> mora view).
fn tones(
    frontend: &JaFrontend,
    input: &str,
    deviations: bool,
    overrides: &AccentTable,
) -> Result<String, String> {
    let normalized = musculus::sbv2::ja_norm::JaNormalizer::new()
        .normalize(input)
        .map_err(|e| e.to_string())?
        .text;
    let read = frontend.num2word(&normalized).map_err(|e| e.to_string())?;
    let normalized = musculus::sbv2::normalize::normalize_text(&read);
    let mut process = frontend
        .process_text(&normalized)
        .map_err(|e| e.to_string())?;
    if deviations {
        process
            .apply_accent_deviations()
            .map_err(|e| e.to_string())?;
    }
    let (phones, tones, _word2ph) = process.g2p().map_err(|e| e.to_string())?;
    let tones = overrides.apply(&phones, &tones);
    let pairs = ja::kana_tone(&phones, &tones).map_err(|e| e.to_string())?;
    Ok(ja::tone_string(&pairs))
}

fn parse_reference() -> Vec<ReferenceItem> {
    include_str!("evaluation/annotations/ja_accent_reference.jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| !line.contains("\"_meta\""))
        .map(|line| serde_json::from_str(line).expect("reference line is valid JSON"))
        .collect()
}

fn parse_annotations() -> Vec<Annotation> {
    include_str!("evaluation/annotations/ja_accent.jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("annotation line is valid JSON"))
        .collect()
}

fn node_matches(ours: &NjdNode, reference: &ReferenceNode) -> bool {
    ours.surface == reference.surface
        && ours.pos == reference.pos
        && ours.pron == reference.pron
        && ours.accent == reference.accent
        && ours.mora_size == reference.mora_size
}

#[test]
fn l3_accent_matches_the_reference_or_is_an_intended_deviation() {
    let frontend = JaFrontend::new().expect("ja frontend");
    let overrides =
        AccentTable::from_json_slice(include_bytes!("../examples/accent-overrides.json"))
            .expect("sample override table parses");
    let reference = parse_reference();
    assert!(
        !reference.is_empty(),
        "reference fixture is empty — regenerate with scripts/gen_accent_reference.py"
    );
    let annotations = parse_annotations();
    let expected_by_input: std::collections::BTreeMap<&str, &str> = annotations
        .iter()
        .filter_map(|item| {
            item.expected_tones
                .as_deref()
                .map(|tones| (item.input.as_str(), tones))
        })
        .collect();

    let mut failures: Vec<String> = Vec::new();
    let mut fidelity_ok = 0usize;
    let mut contained = 0usize;
    let mut deviations_applied = 0usize;

    for item in &reference {
        // (A) fidelity: our baseline must equal the oracle.
        match frontend.njd_nodes(&item.input) {
            Ok(nodes) => {
                if nodes.len() == item.nodes.len()
                    && nodes
                        .iter()
                        .zip(item.nodes.iter())
                        .all(|(ours, reference)| node_matches(ours, reference))
                {
                    fidelity_ok += 1;
                } else {
                    failures.push(format!(
                        "[fidelity] {:?}: ours {:?} vs reference {:?}",
                        item.input,
                        nodes
                            .iter()
                            .map(|n| (n.surface.clone(), n.accent, n.mora_size))
                            .collect::<Vec<_>>(),
                        item.nodes
                            .iter()
                            .map(|n| (n.surface.clone(), n.accent, n.mora_size))
                            .collect::<Vec<_>>()
                    ));
                }
            }
            Err(error) => failures.push(format!("[fidelity] {:?}: {error}", item.input)),
        }

        // (B)/(C) containment and intended deviations.
        let baseline = tones(&frontend, &item.input, false, &overrides);
        let product = tones(&frontend, &item.input, true, &overrides);
        match (baseline, product) {
            (Ok(baseline), Ok(product)) => match expected_by_input.get(item.input.as_str()) {
                Some(expected) => {
                    deviations_applied += 1;
                    if &product.as_str() != expected {
                        failures.push(format!(
                            "[intended] {:?}: expected {expected}, product produced {product}",
                            item.input
                        ));
                    }
                }
                None => {
                    if product == baseline {
                        contained += 1;
                    } else {
                        failures.push(format!(
                            "[containment] {:?}: deviations changed tones {baseline} -> {product} \
                             without an expected_tones entry",
                            item.input
                        ));
                    }
                }
            },
            (baseline, product) => failures.push(format!(
                "[tones] {:?}: baseline {baseline:?}, product {product:?}",
                item.input
            )),
        }
    }

    println!(
        "accent gate: {fidelity_ok}/{} match the reference, {contained} contained, \
         {deviations_applied} intended deviations",
        reference.len()
    );
    assert!(
        failures.is_empty(),
        "accent regression gate failed:\n{}",
        failures.join("\n")
    );
}
