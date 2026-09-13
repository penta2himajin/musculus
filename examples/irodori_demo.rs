//! Irodori-TTS demo + RTF spike (M4 Step A).
//!
//! Requires the `onnx` feature and the artifact bundle from
//! `scripts/setup_irodori.sh`, plus a reference WAV (any mono rate —
//! 44.1/48 kHz). The natural reference is the SBV2 baseline's own
//! output, so both engines are compared on the same voice.
//!
//! Run:
//!   scripts/setup_irodori.sh
//!   cargo run --release --features cli,onnx -- synth "参照音声です。…" --out vendor/irodori-ref.wav
//!   cargo run --release --features onnx,wav --example irodori_demo -- \
//!       --ref vendor/irodori-ref.wav --text "こんにちは" --out out-irodori.wav

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use musculus::prelude::TtsAdapter as _;

#[derive(Parser)]
struct Args {
    /// Model directory (setup_irodori.sh output).
    #[arg(long, default_value = "vendor/irodori")]
    dir: PathBuf,
    /// Reference voice WAV (mono; any rate; resampled to 48 kHz).
    #[arg(long, default_value = "vendor/irodori-ref.wav")]
    ref_wav: PathBuf,
    /// Text to synthesize.
    #[arg(long, default_value = "こんにちは")]
    text: String,
    /// Sampling seed (the RF noise is deterministic per seed).
    #[arg(long, default_value_t = 0)]
    seed: u32,
    /// Euler steps (default 40; halve for a first RTF taste).
    #[arg(long, default_value_t = 40)]
    steps: usize,
    /// Output WAV path (48 kHz).
    #[arg(long, default_value = "out-irodori.wav")]
    out: PathBuf,
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    let t0 = Instant::now();
    let adapter = musculus::irodori::IrodoriAdapter::load(&args.dir, &args.ref_wav)
        .map_err(|e| format!("load: {e}"))?
        .with_seed(args.seed)
        .with_num_steps(args.steps);
    eprintln!(
        "loaded (sessions + tokenizer + reference encode): {:.2} s",
        t0.elapsed().as_secs_f64()
    );

    let segment = musculus::prelude::SpeechSegment::new(args.text.clone());
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let t1 = Instant::now();
    let synthesis = runtime
        .block_on(adapter.synthesize(std::slice::from_ref(&segment)))
        .map_err(|e| format!("synthesis: {e}"))?;
    let wall = t1.elapsed().as_secs_f64();
    let audio_secs = synthesis.duration().as_secs_f64();
    eprintln!(
        "synthesized: {wall:.3} s wall, {audio_seconds:.3} s audio, RTF {rtf:.3} (steps = {})",
        args.steps,
        audio_seconds = audio_secs,
        rtf = wall / audio_secs,
    );

    let chunk = synthesis
        .audio
        .first()
        .cloned()
        .ok_or_else(|| "no audio".to_string())?;
    musculus::wav::write_wav(&args.out, &chunk)
        .map_err(|e| format!("write {}: {e}", args.out.display()))?;
    eprintln!("wrote {}", args.out.display());
    Ok(())
}
