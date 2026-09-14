//! A/B prep: put both engines' output in one comparable format.
//!
//! A blind A/B must not leak the engine through the container or the
//! loudness: the SBV2 path decodes at 44.1 kHz with peaks around 0.3–0.5,
//! the Irodori path at 48 kHz peak-limited to 1.0. This tool resamples
//! to a common rate and normalizes both to the same integrated loudness
//! (ITU-R BS.1770, fp64 K-weighting) before peak limiting.
//!
//! Run:
//!   cargo run --release --features onnx,wav --example prep_ab_audio -- \
//!       --input raw.wav --out ab.wav --rate 48000 --lufs -16

use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
struct Args {
    /// Input WAV (mono; any rate).
    #[arg(long)]
    input: PathBuf,
    /// Output WAV path.
    #[arg(long)]
    out: PathBuf,
    /// Common sample rate (default 48000).
    #[arg(long, default_value_t = 48_000)]
    rate: u32,
    /// Target integrated loudness in LUFS (default -16).
    #[arg(long, default_value_t = -16.0)]
    lufs: f64,
}

fn main() -> Result<(), String> {
    let args = Args::parse();
    let chunk = musculus::wav::read_wav(&args.input)
        .map_err(|e| format!("read {}: {e}", args.input.display()))?;
    let samples = if chunk.sample_rate == args.rate {
        chunk.samples
    } else {
        musculus::irodori::resample_mono(&chunk.samples, chunk.sample_rate, args.rate)
            .map_err(|e| format!("resample: {e}"))?
    };
    let normalized = musculus::irodori::lufs_normalize(&samples, args.rate, args.lufs);
    let peak = normalized.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    musculus::wav::write_wav(
        &args.out,
        &musculus::types::AudioChunk {
            samples: normalized,
            sample_rate: args.rate,
        },
    )
    .map_err(|e| format!("write {}: {e}", args.out.display()))?;
    eprintln!(
        "prepped {} -> {} ({} Hz, target {} LUFS, peak {:.3})",
        args.input.display(),
        args.out.display(),
        args.rate,
        args.lufs,
        peak
    );
    Ok(())
}
