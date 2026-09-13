//! The `musculus` CLI entry point. Built only with the `cli` feature.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

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
    Synth {
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
        /// Output WAV path.
        #[arg(long, default_value = "out.wav")]
        out: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Synth {
            text,
            file,
            voice,
            style,
            style_weight,
            dir,
            out,
        } => {
            if let Err(err) = run_synth(text, file, voice, style, style_weight, dir, out) {
                eprintln!("error: {err}");
                std::process::exit(2);
            }
        }
    }
}

#[cfg(feature = "onnx")]
use musculus::prelude::TtsAdapter as _;

#[cfg(feature = "onnx")]
fn run_synth(
    text: Option<String>,
    file: Option<PathBuf>,
    voice: Option<String>,
    style: i32,
    style_weight: f32,
    dir: PathBuf,
    out: PathBuf,
) -> Result<(), String> {
    let text = match (text, file) {
        (Some(text), None) => text,
        (None, Some(path)) => {
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?
        }
        (Some(_), Some(_)) => return Err("pass either TEXT or --file, not both".into()),
        (None, None) => return Err("no text: pass TEXT or --file".into()),
    };
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("text is empty".into());
    }

    let adapter = musculus::sbv2::Sbv2Adapter::load_dir(&dir)
        .map_err(|e| format!("load models from {}: {e}", dir.display()))?;
    let adapter = adapter.with_style_id(style).with_style_weight(style_weight);

    let mut segment = musculus::prelude::SpeechSegment::new(text);
    if let Some(voice) = voice {
        segment = segment.with_voice(voice);
    }

    eprintln!(
        "voice credit reminder: each voice carries its own license terms — see \
         docs/model-licenses.md (tsukuyomi: つくよみちゃん(CV. 夢前黎), credit \
         required, https://tyc.rei-yumesaki.net/)"
    );

    let synthesis = tokio::runtime::Runtime::new()
        .map_err(|e| format!("tokio runtime: {e}"))?
        .block_on(adapter.synthesize(&[segment]))
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
    musculus::wav::write_wav(&out, &chunk).map_err(|e| format!("write {}: {e}", out.display()))?;

    eprintln!(
        "wrote {}: {:.2} s @ {} Hz ({} chunks)",
        out.display(),
        synthesis.duration().as_secs_f64(),
        sample_rate,
        synthesis.audio.len()
    );
    Ok(())
}

#[cfg(not(feature = "onnx"))]
fn run_synth(
    _text: Option<String>,
    _file: Option<PathBuf>,
    _voice: Option<String>,
    _style: i32,
    _style_weight: f32,
    _dir: PathBuf,
    _out: PathBuf,
) -> Result<(), String> {
    Err("the synth command requires the onnx feature: cargo build --features cli,onnx".into())
}
