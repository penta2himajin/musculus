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
    /// Synthesis engine: sbv2 (realtime default) or irodori (quality).
    /// The ja posture is two engines; see docs/decisions/0005.
    #[arg(long, default_value = "sbv2")]
    engine: String,
    /// Model directory. Defaults to vendor/sbv2 for the sbv2 engine and
    /// vendor/irodori for the irodori engine.
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Voice id (file stem of a .sbv2); sbv2 only.
    #[arg(long)]
    voice: Option<String>,
    /// Style id within the voice's style table (0 = neutral); sbv2 only.
    #[arg(long)]
    style: Option<i32>,
    /// Style blend weight: 0 = neutral mean, 1 = raw style; sbv2 only.
    #[arg(long)]
    style_weight: Option<f32>,
    /// Reference voice WAV for the irodori engine (any mono rate; it is
    /// resampled to 48 kHz and loudness-normalized).
    #[arg(long)]
    ref_wav: Option<PathBuf>,
    /// Rectified-flow Euler steps; irodori only (default 40).
    #[arg(long)]
    steps: Option<usize>,
    /// Sampling seed; irodori only (default 0).
    #[arg(long)]
    seed: Option<u32>,
    /// Split the text into sentences and synthesize each separately,
    /// joining with --sentence-silence. The reference implementations do
    /// this for long input; it is also the "breathless delivery"
    /// hypothesis under test (docs/benchmarks/listening-log.md).
    #[arg(long)]
    split_sentences: bool,
    /// Silence inserted between sentences when --split-sentences is set.
    #[arg(long, default_value_t = 0.4)]
    sentence_silence: f32,
    /// User term dictionary (JSON array of {term, aliases}); applied
    /// before whichever engine runs. The dictionary is yours — musculus
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
use musculus::prelude::TextProcessor as _;

/// Read the text and apply the user dictionary — engine-independent.
#[cfg(feature = "onnx")]
fn prepare_text(args: &SynthArgs) -> Result<String, String> {
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
    Ok(text)
}

/// Turn the CLI flags into an engine description, rejecting flags that
/// belong to the other engine (explicit beats silent).
#[cfg(feature = "onnx")]
fn resolve_engine(args: &SynthArgs) -> Result<musculus::factory::Engine, String> {
    use musculus::factory as f;
    f::parse_engine_name(&args.engine)?;
    match args.engine.as_str() {
        "sbv2" => {
            if args.ref_wav.is_some() || args.steps.is_some() {
                return Err("--ref-wav/--steps only apply to --engine irodori".into());
            }
            Ok(f::Engine::Sbv2 {
                dir: args
                    .dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(f::SBV2_DEFAULT_DIR)),
                voice: args.voice.clone(),
                style_id: args.style.unwrap_or(0),
                style_weight: args.style_weight.unwrap_or(1.0),
            })
        }
        #[cfg(feature = "wav")]
        "irodori" => {
            if args.voice.is_some() || args.style.is_some() || args.style_weight.is_some() {
                return Err(
                    "the irodori engine takes its voice from --ref-wav; --voice/--style/--style-weight are sbv2-only"
                        .into(),
                );
            }
            Ok(f::Engine::Irodori {
                dir: args
                    .dir
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(f::IRODORI_DEFAULT_DIR)),
                ref_wav: args
                    .ref_wav
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(f::IRODORI_DEFAULT_REF)),
                steps: args.steps.unwrap_or(f::IRODORI_DEFAULT_STEPS),
                seed: args.seed.unwrap_or(0),
            })
        }
        other => Err(format!("engine {other:?} is not available in this build")),
    }
}

/// Per-engine license reminder (the voice's terms follow the source).
#[cfg(feature = "onnx")]
fn credit_reminder(engine: &musculus::factory::Engine) {
    match engine {
        musculus::factory::Engine::Sbv2 { .. } => eprintln!(
            "voice credit reminder: each voice carries its own license terms — see \
             docs/model-licenses.md (tsukuyomi: つくよみちゃん(CV. 夢前黎), credit \
             required, https://tyc.rei-yumesaki.net/)"
        ),
        #[cfg(feature = "wav")]
        musculus::factory::Engine::Irodori { ref_wav, .. } => eprintln!(
            "voice credit reminder: Irodori clones the voice of {} — that reference's \
             license terms apply to the output (the shipped default reference is a \
             tsukuyomi/SBV2 render; see docs/model-licenses.md)",
            ref_wav.display()
        ),
    }
}

#[cfg(feature = "onnx")]
fn run_synth(args: SynthArgs) -> Result<(), String> {
    let text = prepare_text(&args)?;
    let engine = resolve_engine(&args)?;
    credit_reminder(&engine);

    let adapter = engine
        .build()
        .map_err(|e| format!("load models from {}: {e}", engine.model_dir().display()))?;

    // One segment per sentence when asked, otherwise the whole text.
    let pieces = if args.split_sentences {
        musculus::segmenter::split_sentences(&text)
    } else {
        vec![text]
    };
    if pieces.is_empty() {
        return Err("text is empty".into());
    }
    let segments: Vec<musculus::prelude::SpeechSegment> = pieces
        .into_iter()
        .map(|piece| {
            let mut segment = musculus::prelude::SpeechSegment::new(piece);
            if let Some(voice) = engine.voice_hint() {
                segment = segment.with_voice(voice);
            }
            segment
        })
        .collect();

    let t0 = std::time::Instant::now();
    let synthesis = tokio::runtime::Runtime::new()
        .map_err(|e| format!("tokio runtime: {e}"))?
        .block_on(adapter.synthesize(&segments))
        .map_err(|e| format!("synthesis: {e}"))?;

    // Concatenate chunk samples; an engine decodes at one rate. When the
    // text was split, the joins get a breath of silence.
    let chunk = if args.split_sentences {
        musculus::segmenter::stitch_with_silence(&synthesis.audio, args.sentence_silence)
    } else {
        musculus::segmenter::stitch_with_silence(&synthesis.audio, 0.0)
    }
    .ok_or_else(|| "synthesis produced no audio".to_string())?;
    let sample_rate = chunk.sample_rate;
    musculus::wav::write_wav(&args.out, &chunk)
        .map_err(|e| format!("write {}: {e}", args.out.display()))?;

    let audio_secs = synthesis.duration().as_secs_f64();
    let wall = t0.elapsed().as_secs_f64();
    eprintln!(
        "wrote {}: {:.2} s @ {} Hz ({} chunk(s){}) | engine={} wall {:.2} s RTF {:.3}",
        args.out.display(),
        audio_secs,
        sample_rate,
        synthesis.audio.len(),
        if args.split_sentences {
            format!(", split at {:.2} s silence", args.sentence_silence)
        } else {
            String::new()
        },
        engine.name(),
        wall,
        wall / audio_secs,
    );
    Ok(())
}

#[cfg(not(feature = "onnx"))]
fn run_synth(_args: SynthArgs) -> Result<(), String> {
    Err("the synth command requires the onnx feature: cargo build --features cli,onnx".into())
}
