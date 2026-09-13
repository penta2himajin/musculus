//! The `musculus` CLI entry point. Built only with the `cli` feature.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// A programmable text-to-speech framework.
#[derive(Parser)]
#[command(name = "musculus", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Synthesize text into a WAV file.
    Synth(SynthArgs),
}

#[derive(Args)]
struct SynthArgs {
    /// Text to synthesize; omit and pass --file instead.
    text: Option<String>,
    /// Read the text from a UTF-8 file instead of the argument.
    #[arg(long)]
    file: Option<PathBuf>,
    /// Voice id (file stem of a .sbv2 in the model directory).
    /// Defaults to the first voice found.
    #[arg(long)]
    voice: Option<String>,
    /// Style id within the voice's style table (0 = neutral).
    #[arg(long, default_value_t = 0)]
    style: i32,
    /// Style blend weight: 0 = neutral mean, 1 = raw style.
    #[arg(long, default_value_t = 1.0)]
    style_weight: f32,
    /// Directory with tokenizer.json, deberta.onnx and *.sbv2 voices.
    #[arg(long, default_value = "vendor/sbv2")]
    dir: PathBuf,
    /// User term dictionary (JSON array of {term, aliases}); applied
    /// before the frontend. The dictionary is yours — musculus
    /// bundles none (docs/model-licenses.md).
    #[arg(long)]
    dict: Option<PathBuf>,
    /// Output WAV path.
    #[arg(long, default_value = "out.wav")]
    out: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Synth(args) => {
            if let Err(err) = run_synth(args) {
                eprintln!("error: {err}");
                std::process::exit(2);
            }
        }
    }
}

#[cfg(feature = "onnx")]
use musculus::prelude::{TextProcessor as _, TtsAdapter as _};

#[cfg(feature = "onnx")]
fn run_synth(args: SynthArgs) -> Result<(), String> {
    let text = match (&args.text, &args.file) {
        (Some(text), None) => text.clone(),
        (None, Some(path)) => {
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?
        }
        (Some(_), Some(_)) => return Err("pass either TEXT or --file, not both".into()),
        (None, None) => return Err("no text: pass TEXT or --file".into()),
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("text is empty".into());
    }

    let text = match &args.dict {
        Some(path) => {
            let file = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
            let entries: Vec<musculus::dictionary::TermEntry> = serde_json::from_slice(&file)
                .map_err(|e| format!("dict {}: {e}", path.display()))?;
            let dictionary = musculus::dictionary::TermDictionary::new(
                entries,
                musculus::dictionary::MatchPolicy::for_japanese(),
            )
            .map_err(|e| format!("dict {}: {e}", path.display()))?;
            let processed = dictionary
                .process(&text)
                .map_err(|e| format!("dict: {e}"))?;
            for correction in &processed.corrections {
                eprintln!("dict: {:?} -> {:?}", correction.from, correction.to);
            }
            processed.text
        }
        None => text,
    };

    let adapter = musculus::sbv2::Sbv2Adapter::load_dir(&args.dir)
        .map_err(|e| format!("load models from {}: {e}", args.dir.display()))?;
    let adapter = adapter
        .with_style_id(args.style)
        .with_style_weight(args.style_weight);

    let mut segment = musculus::prelude::SpeechSegment::new(text);
    if let Some(voice) = &args.voice {
        segment = segment.with_voice(voice.clone());
    }

    eprintln!(
        "voice credit reminder: each voice carries its own license terms — see \
         docs/model-licenses.md (tsukuyomi: つくよみちゃん(CV. 夢前黎), credit \
         required, https://tyc.rei-yumesaki.net/)"
    );

    let synthesis = tokio::runtime::Runtime::new()
        .map_err(|e| format!("tokio runtime: {e}"))?
        .block_on(adapter.synthesize(std::slice::from_ref(&segment)))
        .map_err(|e| format!("synthesis: {e}"))?;

    // Concatenate chunk samples; the SBV2 decode rate is uniform.
    let sample_rate = synthesis
        .sample_rate()
        .ok_or_else(|| "synthesis produced no audio".to_string())?;
    let mut samples = Vec::new();
    for chunk in &synthesis.audio {
        samples.extend_from_slice(&chunk.samples);
    }
    let chunk = musculus::types::AudioChunk {
        samples,
        sample_rate,
    };
    musculus::wav::write_wav(&args.out, &chunk)
        .map_err(|e| format!("write {}: {e}", args.out.display()))?;

    eprintln!(
        "wrote {}: {:.2} s @ {} Hz ({} chunks)",
        args.out.display(),
        synthesis.duration().as_secs_f64(),
        sample_rate,
        synthesis.audio.len()
    );
    Ok(())
}

#[cfg(not(feature = "onnx"))]
fn run_synth(_args: SynthArgs) -> Result<(), String> {
    Err("the synth command requires the onnx feature: cargo build --features cli,onnx".into())
}
