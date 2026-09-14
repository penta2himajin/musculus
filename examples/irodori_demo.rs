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
    /// Execution provider: cpu (default) or coreml (needs the `coreml`
    /// feature: --features onnx,wav,coreml).
    #[arg(long, default_value = "cpu")]
    ep: String,
    /// CoreML compute units when --ep coreml: all | ane | gpu.
    #[arg(long, default_value = "all")]
    coreml_units: String,
    /// CoreML model format when --ep coreml: mlprogram | nn.
    #[arg(long, default_value = "mlprogram")]
    coreml_format: String,
    /// Synthesis repeats (1 cold + N-1 warm) for cold/warm separation.
    #[arg(long, default_value_t = 3)]
    repeats: usize,
}

/// Map the CLI strings to the adapter's execution-provider selection.
fn resolve_ep(
    ep: &str,
    coreml_units: &str,
    coreml_format: &str,
) -> Result<musculus::irodori::ExecutionProvider, String> {
    use musculus::irodori::ExecutionProvider;
    match ep {
        "cpu" => Ok(ExecutionProvider::Cpu),
        "coreml" => {
            #[cfg(feature = "coreml")]
            {
                use musculus::irodori::{CoreMlFormat, CoreMlOptions, CoreMlUnits};
                let units = match coreml_units {
                    "all" => CoreMlUnits::All,
                    "ane" => CoreMlUnits::NeuralEngine,
                    "gpu" => CoreMlUnits::Gpu,
                    other => return Err(format!("unknown --coreml-units: {other}")),
                };
                let format = match coreml_format {
                    "mlprogram" => CoreMlFormat::MlProgram,
                    "nn" => CoreMlFormat::NeuralNetwork,
                    other => return Err(format!("unknown --coreml-format: {other}")),
                };
                Ok(ExecutionProvider::CoreMl(CoreMlOptions { units, format }))
            }
            #[cfg(not(feature = "coreml"))]
            {
                let _ = (coreml_units, coreml_format);
                Err("--ep coreml requires --features onnx,wav,coreml".into())
            }
        }
        other => Err(format!("unknown --ep: {other}")),
    }
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    let ep = resolve_ep(&args.ep, &args.coreml_units, &args.coreml_format)?;

    let t0 = Instant::now();
    let adapter = musculus::irodori::IrodoriAdapter::load_with_ep(&args.dir, &args.ref_wav, ep)
        .map_err(|e| format!("load: {e}"))?
        .with_seed(args.seed)
        .with_num_steps(args.steps);
    let coreml_sessions = adapter.coreml_sessions().to_vec();
    eprintln!(
        "loaded (sessions + tokenizer + reference encode): {:.2} s; coreml sessions: {}",
        t0.elapsed().as_secs_f64(),
        if coreml_sessions.is_empty() {
            "none".to_string()
        } else {
            coreml_sessions.join(", ")
        }
    );

    let segment = musculus::prelude::SpeechSegment::new(args.text.clone());
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;

    // Cold run first (CoreML specializes per shape here), then warm runs.
    let mut synthesis = None;
    let mut cold_rtf = 0.0;
    let mut warm = Vec::new();
    for run in 0..args.repeats.max(1) {
        let t1 = Instant::now();
        let out = runtime
            .block_on(adapter.synthesize(std::slice::from_ref(&segment)))
            .map_err(|e| format!("synthesis: {e}"))?;
        let wall = t1.elapsed().as_secs_f64();
        let audio_secs = out.duration().as_secs_f64();
        let rtf = wall / audio_secs;
        if run == 0 {
            cold_rtf = rtf;
            eprintln!(
                "cold:  {wall:.3} s wall, {audio_secs:.3} s audio, RTF {rtf:.3} (steps = {})",
                args.steps
            );
            synthesis = Some(out);
        } else {
            eprintln!("warm{run}:  {wall:.3} s wall, {audio_secs:.3} s audio, RTF {rtf:.3}");
            warm.push(rtf);
        }
    }
    let warm_p50 = if warm.is_empty() {
        cold_rtf
    } else {
        let mut sorted = warm.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted[sorted.len() / 2]
    };
    println!(
        "ep={} steps={} | cold RTF {:.3} | warm p50 RTF {:.3}",
        args.ep, args.steps, cold_rtf, warm_p50
    );

    let synthesis = synthesis.ok_or_else(|| "no audio".to_string())?;
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
