//! Accent report — the L3 accent view the phoneme gate cannot see.
//!
//! The phoneme-level L3 gate passes a wrong pitch pattern silently
//! (docs/evaluation.md §3.4: the listener caught 「せんに→ひゃく」 while
//! every phoneme matched). This report prints, for each annotated item,
//! the mora sequence and its H/L pattern exactly as the decode receives
//! it, so a native speaker can annotate the expected pattern; once an
//! item carries `expected_tones`, the report also checks it and exits
//! non-zero on a mismatch (which makes it a CI gate candidate).
//!
//! Run:
//!   cargo run --release --features onnx --example accent_report -- \
//!       --annotations tests/evaluation/annotations/ja_accent.jsonl

use std::path::PathBuf;

use clap::Parser;
use musculus::prelude::SpeechNormalizer as _;
use musculus::sbv2::ja::{self, JaFrontend};
use serde::Deserialize;

#[derive(Parser)]
struct Args {
    /// Accent annotation file (JSONL: input, optional expected_tones).
    #[arg(long, default_value = "tests/evaluation/annotations/ja_accent.jsonl")]
    annotations: PathBuf,
    /// Also dump the prosody labels (accent-phrase boundaries and accent
    /// positions) behind each item's tones.
    #[arg(long)]
    labels: bool,
}

#[derive(Deserialize)]
struct AccentItem {
    input: String,
    /// Expected H/L string, one character per mora. Absent = report only.
    #[serde(default)]
    expected_tones: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    note: Option<String>,
}

/// The mora + tone view of one input, through the production chain
/// (JaNormalizer -> num2word -> normalize -> frontend -> g2p).
fn accent_view(frontend: &JaFrontend, input: &str) -> Result<Vec<(String, i32)>, String> {
    let normalized = musculus::sbv2::ja_norm::JaNormalizer::new()
        .normalize(input)
        .map_err(|e| e.to_string())?
        .text;
    let read = frontend.num2word(&normalized).map_err(|e| e.to_string())?;
    let normalized = musculus::sbv2::normalize::normalize_text(&read);
    let process = frontend
        .process_text(&normalized)
        .map_err(|e| e.to_string())?;
    let (phones, tones, _word2ph) = process.g2p().map_err(|e| e.to_string())?;
    ja::kana_tone(&phones, &tones).map_err(|e| e.to_string())
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let content = std::fs::read_to_string(&args.annotations).map_err(|e| format!("read: {e}"))?;
    let frontend = JaFrontend::new().map_err(|e| format!("ja frontend: {e}"))?;

    let mut mismatches = 0usize;
    let mut annotated = 0usize;
    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let item: AccentItem =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        if args.labels {
            let normalized = musculus::sbv2::ja_norm::JaNormalizer::new()
                .normalize(&item.input)
                .map_err(|e| e.to_string())?
                .text;
            let read = frontend.num2word(&normalized).map_err(|e| e.to_string())?;
            let normalized = musculus::sbv2::normalize::normalize_text(&read);
            let process = frontend
                .process_text(&normalized)
                .map_err(|e| e.to_string())?;
            println!(
                "--- labels for {:?} (normalized: {normalized:?})",
                item.input
            );
            for line in process.label_dump().map_err(|e| e.to_string())? {
                println!("      {line}");
            }
        }
        let pairs = accent_view(&frontend, &item.input)?;
        let kana: String = pairs.iter().map(|(m, _)| m.as_str()).collect();
        let tones = ja::tone_string(&pairs);
        // Aligned view: which mora carries which tone (needed to annotate
        // precisely, e.g. セH ンL where セン should share one tone).
        let aligned: String = pairs
            .iter()
            .map(|(m, t)| format!("{m}{}", if *t == 0 { 'L' } else { 'H' }))
            .collect::<Vec<_>>()
            .join(" ");
        match &item.expected_tones {
            None => {
                println!(
                    "[report] {:>10} | カナ {:24} | tones {tones}",
                    item.input, kana
                );
            }
            Some(expected) => {
                annotated += 1;
                let ok = expected == &tones;
                if !ok {
                    mismatches += 1;
                }
                println!(
                    "[{}] {:>10} | カナ {:24} | tones {tones} | expected {expected}\n         {:>10}   {aligned}",
                    if ok { "ok" } else { "NG" },
                    item.input,
                    kana,
                    ""
                );
            }
        }
    }
    println!("\nannotated: {annotated}, mismatches: {mismatches}");
    if mismatches > 0 {
        return Err(format!("{mismatches} accent mismatch(es)"));
    }
    Ok(())
}
