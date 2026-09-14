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
use musculus::prelude::{SpeechNormalizer as _, TextProcessor as _};
use musculus::sbv2::ja::JaFrontend;
use rubato::{FftFixedIn, Resampler};
use serde::Serialize;

#[derive(Parser)]
struct Args {
    /// Synthesis engine: sbv2 (default) or irodori.
    #[arg(long, default_value = "sbv2")]
    engine: String,
    /// Model directory. Defaults per engine (vendor/sbv2 or vendor/irodori).
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Voice id (sbv2 only).
    #[arg(long)]
    voice: Option<String>,
    /// Reference voice WAV (irodori only).
    #[arg(long)]
    ref_wav: Option<PathBuf>,
    /// Enable musculus's deliberate accent deviations from the reference
    /// frontend (experimental; docs/accent-resources.md).
    #[arg(long)]
    accent_deviations: bool,
    /// User accent overrides (sbv2 only).
    #[arg(long)]
    accent: Option<PathBuf>,
    /// Rectified-flow Euler steps (irodori only; default 40).
    #[arg(long)]
    steps: Option<usize>,
    /// Feed the engine a katakana reading produced by musculus's ja
    /// frontend instead of the original text (experiment: borrow SBV2's
    /// reading accuracy for another engine). Reading-level CER stays
    /// comparable; text-level CER still measures the intended text.
    #[arg(long)]
    kana_readings: bool,
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
    engine: String,
    kana_readings: bool,
    ruler: String,
    mean_text_cer: f64,
    mean_reading_cer: f64,
    mean_rtf: f64,
    items: Vec<SentenceResult>,
}

fn gcd_usize(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd_usize(b, a % b)
    }
}

/// Resample mono audio to 16 kHz (the ruler's rate) with an FFT-based
/// resampler so aliasing does not bias the ASR and inflate our own CER.
/// Works for both engines' native rates (SBV2 44.1 kHz, Irodori 48 kHz).
fn resample_to_16000(samples: &[f32], input_rate: u32) -> Result<Vec<f32>, String> {
    if input_rate == 16_000 {
        return Ok(samples.to_vec());
    }
    let min_chunk = input_rate as usize / gcd_usize(input_rate as usize, 16_000);
    // A multiple of min_chunk keeps the in/out ratio exact.
    let chunk_in = min_chunk * ((441 / min_chunk).max(1));
    let mut resampler = FftFixedIn::<f32>::new(input_rate as usize, 16_000, chunk_in, 4, 1)
        .map_err(|e| format!("rubato: {e}"))?;
    let mut out = Vec::with_capacity(samples.len() * 16_000 / input_rate as usize + 16_000);
    for chunk in samples.chunks(chunk_in) {
        let mut padded = chunk.to_vec();
        if padded.len() < chunk_in {
            padded.resize(chunk_in, 0.0);
        }
        let frames = resampler
            .process(&[padded], None)
            .map_err(|e| format!("rubato process: {e}"))?;
        out.extend_from_slice(&frames[0]);
    }
    // Trim the zero-padded tail to the true expected length.
    let expected = samples.len() as u64 * 16_000 / input_rate as u64;
    out.truncate(expected as usize);
    Ok(out)
}

/// Katakana reading of `text` via the musculus ja frontend
/// (JaNormalizer -> num2word -> normalize -> g2p -> mora kana).
fn kana_reading(frontend: &JaFrontend, text: &str) -> Result<String, String> {
    let normalized = musculus::sbv2::ja_norm::JaNormalizer::new()
        .normalize(text)
        .map_err(|e| e.to_string())?
        .text;
    let read = frontend.num2word(&normalized).map_err(|e| e.to_string())?;
    let normalized = musculus::sbv2::normalize::normalize_text(&read);
    let process = frontend
        .process_text(&normalized)
        .map_err(|e| e.to_string())?;
    let (phones, _tones, _word2ph) = process.g2p().map_err(|e| e.to_string())?;
    musculus::sbv2::ja::phones_to_kana(&phones).map_err(|e| e.to_string())
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

    use musculus::factory as f;
    f::parse_engine_name(&args.engine)?;
    let engine = match args.engine.as_str() {
        "sbv2" => f::Engine::Sbv2 {
            dir: args
                .dir
                .clone()
                .unwrap_or_else(|| PathBuf::from(f::SBV2_DEFAULT_DIR)),
            voice: args.voice.clone(),
            style_id: 0,
            style_weight: 1.0,
            accent: match &args.accent {
                Some(path) => musculus::accent::AccentTable::from_file(path)?,
                None => Default::default(),
            },
            accent_deviations: args.accent_deviations,
        },
        "irodori" => f::Engine::Irodori {
            dir: args
                .dir
                .clone()
                .unwrap_or_else(|| PathBuf::from(f::IRODORI_DEFAULT_DIR)),
            ref_wav: args
                .ref_wav
                .clone()
                .unwrap_or_else(|| PathBuf::from(f::IRODORI_DEFAULT_REF)),
            steps: args.steps.unwrap_or(f::IRODORI_DEFAULT_STEPS),
            seed: 0,
        },
        other => return Err(format!("unknown engine: {other}")),
    };
    let adapter = engine
        .build()
        .map_err(|e| format!("load models from {}: {e}", engine.model_dir().display()))?;
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

        // What the engine actually receives: the intended text, or a
        // katakana reading of it (the frontend-borrowing experiment).
        // Both CERs below still measure against the intended text.
        let spoken = if args.kana_readings {
            kana_reading(&frontend, &rewritten)?
        } else {
            rewritten.clone()
        };
        let mut segment = musculus::prelude::SpeechSegment::new(spoken);
        if let Some(voice) = engine.voice_hint() {
            segment = segment.with_voice(voice);
        }
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

        let at_16k = resample_to_16000(&samples, synthesis.sample_rate().unwrap_or(44_100))?;
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
        engine: engine.name().to_string(),
        kana_readings: args.kana_readings,
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
