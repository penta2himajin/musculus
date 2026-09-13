//! Round-trip CER evaluation (L1): musculus synthesizes the sentence
//! set, the ruler ASR (euhadra's ParakeetAdapter, the same model
//! euhadra's L1 uses for ja) transcribes it back, and both a text-level
//! and a reading-level CER are computed.
//!
//! Two metrics on purpose (docs/evaluation.md §2):
//! - **text CER** (`cer_normalized`): the standard, literature-ish
//!   number; counts kanji/kana orthography differences as errors.
//! - **reading CER**: both sides pass through the same ja frontend and
//!   are compared at the phoneme level — homophone-fair, measuring
//!   whether the listener (the ASR) heard the right *sounds*.
//!
//! The ruler model arrives via scripts/setup_ruler_asr.sh (2.4 GB).
//! Manual run — the ruler is too heavy for CI; results go to
//! docs/benchmarks/cer-ja/ as a committed baseline.
//!
//! Run: cargo run --release --features onnx --example eval_cer -- \
//!          --json docs/benchmarks/cer-ja/run.json

use std::path::{Path, PathBuf};

use clap::Parser;
use euhadra::parakeet::ParakeetAdapter;
use euhadra::traits::AsrAdapter as _;
use musculus::prelude::{SpeechNormalizer as _, TextProcessor as _, TtsAdapter as _};
use musculus::sbv2::ja::JaFrontend;
use rubato::{FftFixedIn, Resampler};
use serde::Serialize;

#[derive(Parser)]
struct Args {
    /// Model directory (setup_sbv2.sh output).
    #[arg(long, default_value = "vendor/sbv2")]
    dir: PathBuf,
    /// Ruler ASR bundle (setup_ruler_asr.sh output).
    #[arg(long, default_value = "vendor/parakeet_ja")]
    ruler: PathBuf,
    /// Round-trip sentence set.
    #[arg(long, default_value = "tests/evaluation/sentences/ja.jsonl")]
    sentences: PathBuf,
    /// Optional user dictionary applied before the frontend.
    #[arg(long)]
    dict: Option<PathBuf>,
    /// Where to write the results JSON.
    #[arg(long)]
    json: PathBuf,
    /// Also write each synthesized WAV here for listening.
    #[arg(long)]
    audio_dir: Option<PathBuf>,
}

#[derive(Serialize)]
struct SentenceResult {
    text: String,
    transcript: String,
    text_cer: f64,
    reading_cer: f64,
    rtf: f64,
    audio_seconds: f64,
}

#[derive(Serialize)]
struct Report {
    ruler: String,
    mean_text_cer: f64,
    mean_reading_cer: f64,
    mean_rtf: f64,
    items: Vec<SentenceResult>,
}

/// Resample mono 44.1 kHz → 16 kHz with an FFT-based resampler so the
/// aliasing noise does not bias the ruler (linear interpolation would
/// systematically hurt the ASR and inflate our own CER).
fn resample_44100_to_16000(samples: &[f32]) -> Result<Vec<f32>, String> {
    // Fixed 441-sample input chunks; rubato derives the 160-sample
    // output chunks from the rates (44100:16000 = 441:160, exact).
    let mut resampler =
        FftFixedIn::<f32>::new(44100, 16000, 441, 4, 1).map_err(|e| format!("rubato: {e}"))?;
    let mut out = Vec::with_capacity(samples.len() * 160 / 441 + 160);
    for chunk in samples.chunks(441) {
        let mut padded = chunk.to_vec();
        if padded.len() < 441 {
            padded.resize(441, 0.0);
        }
        let frames = resampler
            .process(&[padded], None)
            .map_err(|e| format!("rubato process: {e}"))?;
        out.extend_from_slice(&frames[0]);
    }
    // Trim the zero-padded tail to the true expected length.
    let expected = samples.len() as u64 * 16000 / 44100;
    out.truncate(expected as usize);
    Ok(out)
}

/// The ja frontend's reading of `text`, as a phoneme sequence with
/// pads and punctuation removed — the homophone-fair comparison space.
fn reading_phonemes(frontend: &JaFrontend, text: &str) -> Option<Vec<String>> {
    // Mirror the production chain: JaNormalizer -> num2word -> ...
    let normalized = musculus::sbv2::ja_norm::JaNormalizer::new()
        .normalize(text)
        .ok()?
        .text;
    let read = frontend.num2word(&normalized).ok()?;
    let normalized = musculus::sbv2::normalize::normalize_text(&read);
    let process = frontend.process_text(&normalized).ok()?;
    let (phones, _tones, _word2ph) = process.g2p().ok()?;
    Some(
        phones
            .into_iter()
            .filter(|p| p != "_" && !p.contains('\''))
            .collect(),
    )
}

fn load_sentences(path: &Path) -> Result<Vec<(String, String)>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read {path:?}: {e}"))?;
    let mut out = Vec::new();
    for line in content.lines().filter(|l| !l.trim().is_empty()) {
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("{path:?}: {e}"))?;
        let text = value["text"].as_str().ok_or("missing text")?.to_string();
        let category = value["category"]
            .as_str()
            .unwrap_or("uncategorised")
            .to_string();
        out.push((text, category));
    }
    Ok(out)
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    let adapter = musculus::sbv2::Sbv2Adapter::load_dir(&args.dir)
        .map_err(|e| format!("load models: {e}"))?;
    let ruler = ParakeetAdapter::load(&args.ruler).map_err(|e| format!("load ruler: {e}"))?;
    let frontend = JaFrontend::new().map_err(|e| format!("ja frontend: {e}"))?;

    let dictionary = match &args.dict {
        Some(path) => {
            let file = std::fs::read(path).map_err(|e| format!("read {path:?}: {e}"))?;
            let entries: Vec<musculus::dictionary::TermEntry> =
                serde_json::from_slice(&file).map_err(|e| format!("dict {path:?}: {e}"))?;
            Some(
                musculus::dictionary::TermDictionary::new(
                    entries,
                    musculus::dictionary::MatchPolicy::for_japanese(),
                )
                .map_err(|e| format!("dict: {e}"))?,
            )
        }
        None => None,
    };

    let sentences = load_sentences(&args.sentences)?;
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;

    let mut items: Vec<SentenceResult> = Vec::new();
    for (index, (text, category)) in sentences.iter().enumerate() {
        let rewritten = match &dictionary {
            Some(dict) => dict.process(text).map_err(|e| format!("dict: {e}"))?.text,
            None => text.clone(),
        };

        let segment = musculus::prelude::SpeechSegment::new(rewritten.clone());
        let t0 = std::time::Instant::now();
        let synthesis = runtime
            .block_on(adapter.synthesize(std::slice::from_ref(&segment)))
            .map_err(|e| format!("synthesis: {e}"))?;
        let wall = t0.elapsed().as_secs_f64();
        let audio_seconds = synthesis.duration().as_secs_f64();
        let rtf = wall / audio_seconds;

        let mut samples = Vec::new();
        for chunk in &synthesis.audio {
            samples.extend_from_slice(&chunk.samples);
        }
        if let Some(dir) = &args.audio_dir {
            std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {dir:?}: {e}"))?;
            let chunk = musculus::types::AudioChunk {
                samples: samples.clone(),
                sample_rate: synthesis.sample_rate().unwrap_or(44_100),
            };
            let wav_path = dir.join(format!("{index:02}.wav"));
            musculus::wav::write_wav(&wav_path, &chunk)
                .map_err(|e| format!("write {wav_path:?}: {e}"))?;
        }

        let at_16k = resample_44100_to_16000(&samples)?;
        let ruler_chunk = euhadra::types::AudioChunk {
            samples: at_16k,
            sample_rate: 16_000,
            channels: 1,
        };
        let transcript = runtime
            .block_on(ruler.transcribe(std::slice::from_ref(&ruler_chunk)))
            .map_err(|e| format!("ruler transcribe: {e}"))?
            .text;

        let text_cer = musculus::eval::cer_normalized(&rewritten, &transcript);
        let reading_cer = {
            let reference =
                reading_phonemes(&frontend, &rewritten).ok_or("frontend failed on reference")?;
            let hypothesis = reading_phonemes(&frontend, &transcript).unwrap_or_default();
            if reference.is_empty() {
                f64::NAN
            } else {
                musculus::eval::levenshtein(&reference, &hypothesis) as f64 / reference.len() as f64
            }
        };

        println!(
            "[{category}] {text}\n  ruler: {transcript}\n  text CER {text_cer:.3}, reading CER {reading_cer:.3}, RTF {rtf:.3}"
        );
        items.push(SentenceResult {
            text: text.clone(),
            transcript,
            text_cer,
            reading_cer,
            rtf,
            audio_seconds,
        });
    }

    let mean = |values: &[f64]| -> f64 {
        let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
        if finite.is_empty() {
            f64::NAN
        } else {
            finite.iter().sum::<f64>() / finite.len() as f64
        }
    };
    let report = Report {
        ruler: format!(
            "parakeet-tdt_ctc-0.6b-ja ONNX (euhadra L1 ja ruler, via euhadra {})",
            euhadra_version()
        ),
        mean_text_cer: mean(&items.iter().map(|i| i.text_cer).collect::<Vec<_>>()),
        mean_reading_cer: mean(&items.iter().map(|i| i.reading_cer).collect::<Vec<_>>()),
        mean_rtf: mean(&items.iter().map(|i| i.rtf).collect::<Vec<_>>()),
        items,
    };
    println!(
        "MEAN text CER {:.3}, reading CER {:.3}, RTF {:.3} over {} sentences",
        report.mean_text_cer,
        report.mean_reading_cer,
        report.mean_rtf,
        report.items.len()
    );

    if let Some(parent) = args.json.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {parent:?}: {e}"))?;
    }
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    std::fs::write(&args.json, json).map_err(|e| format!("write {:?}: {e}", args.json))?;
    eprintln!("wrote {}", args.json.display());
    Ok(())
}

fn euhadra_version() -> String {
    // The ruler's provenance is a measurement fact; record it from the
    // dependency itself rather than a hand-maintained string.
    option_env!("EUHADRA_VERSION")
        .unwrap_or("0.3 (crates.io)")
        .to_string()
}
