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
    /// Output WAV path (required unless --measure).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Common sample rate (default 48000).
    #[arg(long, default_value_t = 48_000)]
    rate: u32,
    /// Target integrated loudness in LUFS (default -16).
    #[arg(long, default_value_t = -16.0)]
    lufs: f64,
    /// Measure only: print the input's integrated loudness and peak, do
    /// not write anything. Used to verify that an A/B pair really is
    /// loudness-matched before it is presented.
    #[arg(long)]
    measure: bool,
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
    if args.measure {
        // The achieved loudness is what matters: peak limiting can leave
        // a peaky file short of the target.
        let lufs = musculus::irodori::integrated_loudness(&samples, args.rate);
        let peak = samples.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        println!(
            "lufs={} peak={:.4}",
            lufs.map_or("nan".to_string(), |v| format!("{v:.2}")),
            peak
        );
        return Ok(());
    }
    let normalized = musculus::irodori::lufs_normalize(&samples, args.rate, args.lufs);
    let peak = normalized.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    let out = args
        .out
        .as_ref()
        .ok_or_else(|| "--out is required unless --measure".to_string())?;
    musculus::wav::write_wav(
        out,
        &musculus::types::AudioChunk {
            samples: normalized,
            sample_rate: args.rate,
        },
    )
    .map_err(|e| format!("write {}: {e}", out.display()))?;
    eprintln!(
        "prepped {} -> {} ({} Hz, target {} LUFS, peak {:.3})",
        args.input.display(),
        out.display(),
        args.rate,
        args.lufs,
        peak
    );
    Ok(())
}
