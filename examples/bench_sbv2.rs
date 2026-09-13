//! SBV2 synthesis RTF benchmark.
//!
//! Mirrors euhadra's bench-example posture: requires the `onnx`
//! feature and a model directory laid out by `scripts/setup_sbv2.sh`;
//! prints RTF and latency stats for the acceptance criterion
//! 「こんにちは」が WAV に出る + RTF 計測例.
//!
//! Run: cargo run --release --features onnx --example bench_sbv2 -- \
//!          --dir vendor/sbv2 --text こんにちは --repeats 3

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use musculus::prelude::TtsAdapter as _;

#[derive(Parser)]
struct Args {
    /// Model directory (setup_sbv2.sh output).
    #[arg(long, default_value = "vendor/sbv2")]
    dir: PathBuf,
    /// Text to synthesize.
    #[arg(long, default_value = "こんにちは")]
    text: String,
    /// Synthesis repeats after one warmup run.
    #[arg(long, default_value_t = 3)]
    repeats: usize,
    /// Write the final render to this WAV path.
    #[arg(long)]
    out: Option<PathBuf>,
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    let start = Instant::now();
    let adapter = musculus::sbv2::Sbv2Adapter::load_dir(&args.dir)
        .map_err(|e| format!("load models: {e}"))?;
    let load_secs = start.elapsed().as_secs_f64();

    let segment = musculus::prelude::SpeechSegment::new(args.text.clone());
    let runtime = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;

    let mut audio_durations = Vec::new();
    let mut latencies = Vec::new();
    let mut rtfs = Vec::new();
    let mut audio_out = None;
    for i in 0..=args.repeats {
        let t0 = Instant::now();
        let synthesis = runtime
            .block_on(adapter.synthesize(std::slice::from_ref(&segment)))
            .map_err(|e| format!("synthesis: {e}"))?;
        let elapsed = t0.elapsed().as_secs_f64();
        let audio_secs = synthesis.duration().as_secs_f64();
        let rtf = elapsed / audio_secs;
        if i == 0 {
            eprintln!("warmup: {elapsed:.3} s wall, {audio_secs:.3} s audio, RTF {rtf:.3}");
            audio_out = Some(synthesis);
        } else {
            eprintln!("run {i}:   {elapsed:.3} s wall, {audio_secs:.3} s audio, RTF {rtf:.3}");
            audio_durations.push(audio_secs);
            latencies.push(elapsed);
            rtfs.push(rtf);
        }
    }

    let median = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let max = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() - 1]
    };

    println!("load:              {load_secs:.3} s");
    println!(
        "audio duration:    p50 {:.3} s",
        median(&mut audio_durations)
    );
    println!(
        "latency (wall):    p50 {:.3} s, max {:.3} s",
        median(&mut latencies),
        max(&mut latencies)
    );
    println!(
        "RTF:               p50 {:.3}, max {:.3}  (lower is better; 1.0 = real time)",
        median(&mut rtfs),
        max(&mut rtfs)
    );

    if let Some(out) = args.out {
        let synthesis = audio_out.expect("warmup produced audio");
        let sample_rate = synthesis.sample_rate().expect("non-empty audio");
        let mut samples = Vec::new();
        for chunk in &synthesis.audio {
            samples.extend_from_slice(&chunk.samples);
        }
        let chunk = musculus::types::AudioChunk {
            samples,
            sample_rate,
        };
        musculus::wav::write_wav(&out, &chunk)
            .map_err(|e| format!("write {}: {e}", out.display()))?;
        eprintln!("wrote {}", out.display());
    }
    Ok(())
}
