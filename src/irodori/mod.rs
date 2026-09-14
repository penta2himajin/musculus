//! Irodori-TTS adapter — the M4 comparison candidate (ADR-0002).
//!
//! Rust (ort rc.13) port of the rectified-flow inference pipeline,
//! following the **official WebGPU runtime** (`irodori-tts-webgpu`,
//! MIT) which was verified bit-faithful against the upstream PyTorch
//! runtime (corr = 1.000000). The port mirrors `runtime/pipeline.mjs`:
//! all forward graphs are ONNX sessions; control flow (text
//! normalization, tokenization, duration → length, the RF Euler loop
//! with 3-branch CFG, loudness) lives here.
//!
//! Zero-shot voice cloning: the adapter instance carries one reference
//! voice (a 48 kHz mono WAV), like SBV2 adapters carry one .sbv2 voice.
//! The natural reference for the comparison is the SBV2 baseline's own
//! output, so both engines are compared on the same voice.
//!
//! ```text
//! text ─tokenize(llm-jp)─▶ text_encoder ─┐
//! ref.wav(48k) ─dacvae_encoder─▶ speaker_encoder ─┤
//!                                    duration ─▶ seqLen
//! seeded noise ─▶ RF Euler ×40 (dit, batch-3 CFG in t ∈ [0.5, 1])
//!                     └▶ latent ─▶ dacvae_decoder ─▶ 48 kHz
//! ```

use std::path::Path;
use std::sync::{LazyLock, Mutex};

use async_trait::async_trait;
use ort::session::Session;
use ort::value::Value;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

use crate::traits::{TtsAdapter, TtsError};
use crate::types::AudioChunk;
use crate::types::{SpeechSegment, Synthesis};

pub const SAMPLE_RATE: u32 = 48_000;
pub const HOP: usize = 1920;
pub const LATENT_DIM: usize = 32;
const BOS: i64 = 1;

// ---------------------------------------------------------------------------
// Text normalization (port of irodori_tts/text_normalization.py)
// ---------------------------------------------------------------------------

static STRIP_MARKS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[;▼♀♂《》≪≫①②③④⑤⑥]").expect("strip pattern is valid"));
static DASHES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[˗‐-―⁃−⎯⏤─━⸺⸻]").expect("dash pattern is valid"));
static WAVES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("[～〜]").expect("wave pattern is valid"));
static ELLIPSIS_RUNS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("…{3,}").expect("ellipsis pattern is valid"));
static MULTIDOTS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new("\\.{2,}").expect("dots pattern is valid"));

/// Irodori's own text normalization — separate from musculus's
/// ja_normalizer on purpose: the comparison engines must each run
/// their native preprocessing, or the comparison measures musculus's
/// normalizer through Irodori's mouth.
pub fn normalize_text(text: &str) -> String {
    let mut t = text
        .replace(['\t', '　'], "")
        .replace("[n]", "")
        .replace('？', "?")
        .replace('！', "!")
        .replace(['♥'], "♡")
        .replace(['●', '◯', '〇'], "○");
    t = STRIP_MARKS.replace_all(&t, "").to_string();
    t = DASHES.replace_all(&t, "").to_string();
    t = WAVES.replace_all(&t, "ー").to_string();
    t = ELLIPSIS_RUNS.replace_all(&t, "……").to_string();
    t = strip_outer_brackets(&t);
    t = t.nfkc().collect();
    // "..." and ".." collapse to "…" (after NFKC, matching the JS order).
    MULTIDOTS.replace_all(&t, "…").to_string()
}

/// Strip balanced outer brackets (「」, 『』, （）, 【】, ()).
fn strip_outer_brackets(text: &str) -> String {
    let pairs = [
        ('「', '」'),
        ('『', '』'),
        ('（', '）'),
        ('【', '】'),
        ('(', ')'),
    ];
    let chars: Vec<char> = text.chars().collect();
    let mut chars = chars;
    loop {
        if chars.len() < 2 {
            break;
        }
        let Some((open, close)) = pairs.iter().find(|(o, _)| *o == chars[0]) else {
            break;
        };
        if chars[chars.len() - 1] != *close {
            break;
        }
        // The whole span must be wrapped: first char opens, last closes,
        // and no intermediate reset to depth 0.
        let mut depth = 0usize;
        let mut balanced = true;
        for (i, c) in chars.iter().enumerate() {
            if *c == *open {
                depth += 1;
            } else if *c == *close {
                depth -= 1;
            }
            if depth == 0 && i < chars.len() - 1 {
                balanced = false;
                break;
            }
        }
        if !balanced || depth != 0 {
            break;
        }
        chars = chars[1..chars.len() - 1].to_vec();
    }
    chars.into_iter().collect()
}

// ---------------------------------------------------------------------------
// Seeded noise (mulberry32 + Box–Muller, port of the JS runtime)
// ---------------------------------------------------------------------------

fn mulberry32(seed: u32) -> impl FnMut() -> f64 {
    let mut a = seed;
    move || {
        // Exact port of the JS mulberry32 (u32 wrap-around semantics).
        a = a.wrapping_add(0x6d2b_79f5);
        let t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        let t = t.wrapping_add(t ^ (t >> 7)) ^ t;
        (t ^ (t >> 14)) as f64 / 4294967296.0
    }
}

fn gaussian_noise(n: usize, seed: u32) -> Vec<f32> {
    let mut rng = mulberry32(seed);
    (0..n)
        .map(|_| {
            let u1 = rng().max(1e-12);
            let u2 = rng();
            (f64::sqrt(-2.0 * f64::ln(u1)) * (2.0 * std::f64::consts::PI * u2).cos()) as f32
        })
        .collect()
}

// ---------------------------------------------------------------------------
// ITU-R BS.1770 loudness (fp64, K-weighting) + peak limit
// ---------------------------------------------------------------------------

struct Biquad {
    b: [f64; 3],
    a: [f64; 3],
}

static K_WEIGHT_48K: LazyLock<Vec<Biquad>> = LazyLock::new(|| {
    vec![
        // high-shelf
        Biquad {
            b: [1.5351828863637502, -2.691804030199196, 1.198426263333146],
            a: [1.0, -1.6906995865986896, 0.7325047060963897],
        },
        // high-pass
        Biquad {
            b: [0.9950442970178917, -1.9900885940357833, 0.9950442970178917],
            a: [1.0, -1.990076284018423, 0.9901009040531438],
        },
    ]
});

fn lfilter(x: &[f64], filter: &Biquad) -> Vec<f64> {
    let (b, a) = (&filter.b, &filter.a);
    let mut y = vec![0.0; x.len()];
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for (n, &xn) in x.iter().enumerate() {
        let yn = b[0] * xn + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2;
        y[n] = yn;
        x2 = x1;
        x1 = xn;
        y2 = y1;
        y1 = yn;
    }
    y
}

/// ITU-R BS.1770 integrated loudness in LUFS (f64 K-weighting).
///
/// Public because the A/B prep tool verifies that a presented pair is
/// actually loudness-matched (peak limiting can leave a peaky file
/// short of the target).
pub fn integrated_loudness(wav: &[f32], rate: u32) -> Option<f64> {
    let wav: Vec<f64> = wav.iter().map(|&v| v as f64).collect();
    integrated_loudness_f64(&wav, rate)
}

fn integrated_loudness_f64(wav: &[f64], rate: u32) -> Option<f64> {
    let mut d = wav.to_vec();
    for filter in K_WEIGHT_48K.iter() {
        d = lfilter(&d, filter);
    }
    let kernel = (0.4 * rate as f64).round() as usize;
    let stride = (0.4 * rate as f64 * 0.25).round() as usize;
    if d.len() < kernel {
        return None;
    }
    let nf = (d.len() - kernel).div_ceil(stride) + 1;
    let mut z = vec![0.0f64; nf];
    let mut l = vec![0.0f64; nf];
    for (j, slot) in z.iter_mut().enumerate() {
        let mut sum = 0.0;
        let offset = j * stride;
        for i in 0..kernel {
            let idx = offset + i;
            if idx < d.len() {
                sum += d[idx] * d[idx];
            }
        }
        *slot = sum / kernel as f64;
        l[j] = -0.691 + 10.0 * (*slot).log10();
    }
    let abs_keep: Vec<usize> = (0..nf).filter(|&j| l[j] > -70.0).collect();
    if abs_keep.is_empty() {
        return None;
    }
    let z_abs_mean = abs_keep.iter().map(|&j| z[j]).sum::<f64>() / abs_keep.len() as f64;
    let gamma_r = -0.691 + 10.0 * z_abs_mean.log10() - 10.0;
    let rel_keep: Vec<usize> = abs_keep.into_iter().filter(|&j| l[j] > gamma_r).collect();
    if rel_keep.is_empty() {
        return None;
    }
    let z_mean = rel_keep.iter().map(|&j| z[j]).sum::<f64>() / rel_keep.len() as f64;
    Some(-0.691 + 10.0 * z_mean.log10())
}

/// Normalize to `target_db` LUFS, then peak-limit to |x| ≤ 1.
pub fn lufs_normalize(wav: &[f32], rate: u32, target_db: f64) -> Vec<f32> {
    let mut out: Vec<f32> = wav.to_vec();
    if let Some(lufs) = integrated_loudness(wav, rate) {
        if lufs.is_finite() {
            let gain = 10f64.powf((target_db - lufs) / 20.0);
            for v in out.iter_mut() {
                *v *= gain as f32;
            }
        }
    }
    let peak = out.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    if peak > 1.0 {
        let g = 1.0 / peak;
        for v in out.iter_mut() {
            *v *= g;
        }
    }
    out
}

/// Resample mono audio to `target` rate via rubato FFT chunks.
///
/// Shared by the reference-voice path and the A/B prep tool so both
/// engines' audio can be compared at one rate.
pub fn resample_mono(samples: &[f32], input_rate: u32, target: u32) -> Result<Vec<f32>, String> {
    use rubato::Resampler as _;
    if input_rate == target {
        return Ok(samples.to_vec());
    }
    let min_chunk = gcd(input_rate as usize, target as usize);
    let chunk_in = min_chunk * ((441 / min_chunk).max(1));
    let mut resampler =
        rubato::FftFixedIn::<f32>::new(input_rate as usize, target as usize, chunk_in, 4, 1)
            .map_err(|e| format!("rubato: {e}"))?;
    let mut out =
        Vec::with_capacity(samples.len() * target as usize / input_rate as usize + target as usize);
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
    Ok(out)
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Decode defaults of the released exports (pipeline.mjs).
const NUM_STEPS: usize = 40;
const CFG_TEXT: f32 = 3.0;
const CFG_SPK: f32 = 5.0;
const CFG_MIN_T: f32 = 0.5;
const CFG_MAX_T: f32 = 1.0;
const INIT_SCALE: f32 = 0.999;
const REF_TARGET_LUFS: f64 = -16.0;

/// One loaded Irodori voice: sessions + tokenizer + encoded reference.
///
/// The reference WAV is decoded once at construction (resampled to
/// 48 kHz mono, LUFS-normalized to −16 LUFS), so repeated syntheses
/// share the speaker state — the same "adapter instance is one voice"
/// shape as the SBV2 adapters.
pub struct IrodoriAdapter {
    text: Mutex<Session>,
    duration: Mutex<Session>,
    dit: Mutex<Session>,
    dac: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    speaker_ready: Mutex<SpkState>,
    seed: u32,
    num_steps: usize,
    /// Sessions that accepted the CoreML EP (empty on CPU runs).
    coreml_sessions: Vec<String>,
}

#[derive(Clone)]
struct SpkState {
    /// `speaker_state` flattened, shape [1, tspk, dim].
    state: Vec<f32>,
    tspk: usize,
    dim: usize,
    /// `speaker_mask` as booleans (the JS feeds Uint8Array-as-bool).
    mask: Vec<bool>,
}

fn inference_error(what: &str, e: impl std::fmt::Display) -> TtsError {
    TtsError::Inference(format!("{what}: {e}"))
}

/// Which execution provider the ONNX sessions should prefer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionProvider {
    /// CPU only — the default, available in every build.
    #[default]
    Cpu,
    /// Apple CoreML (GPU/NE) with CPU fallback. Requires the `coreml`
    /// feature; without it the sessions still build, CPU-only.
    CoreMl(CoreMlOptions),
}

/// CoreML compute-unit selection (mirrors ORT's option).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoreMlUnits {
    /// Let ORT choose among CPU/GPU/ANE.
    #[default]
    All,
    /// Apple Silicon Neural Engine (with CPU).
    NeuralEngine,
    /// Apple GPU (with CPU).
    Gpu,
}

/// CoreML model format (mirrors ORT's option).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoreMlFormat {
    /// MLProgram — newer, broader operator support.
    #[default]
    MlProgram,
    /// NeuralNetwork — older, better compatibility.
    NeuralNetwork,
}

/// CoreML options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CoreMlOptions {
    pub units: CoreMlUnits,
    pub format: CoreMlFormat,
}

/// Load an Irodori export from its path.
///
/// The exports use external-data tensors (`.onnx.data`), which ort
/// validates relative to the model file's directory — so these load
/// from the file, not from bytes (unlike the SBV2 exports, whose
/// weights are embedded).
fn load_session_by_name(
    dir: &Path,
    name: &str,
    ep: ExecutionProvider,
) -> Result<Mutex<Session>, TtsError> {
    let path = dir.join("onnx").join(format!("{name}.onnx"));
    let mut builder = ort::session::Session::builder()
        .map_err(|e| TtsError::ModelLoad(e.to_string()))?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|e| TtsError::ModelLoad(e.to_string()))?
        .with_intra_threads(
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
        )
        .map_err(|e| TtsError::ModelLoad(e.to_string()))?;
    if let ExecutionProvider::CoreMl(options) = ep {
        builder = with_coreml(builder, options)?;
    }
    let session = builder
        .commit_from_file(&path)
        .map_err(|e| TtsError::ModelLoad(format!("{}: {e}", path.display())))?;
    Ok(Mutex::new(session))
}

/// Append the CoreML EP (then CPU) to a session builder.
///
/// CPU is appended after CoreML on purpose: ORT assigns each node to
/// the first provider that can run it, so nodes CoreML cannot handle
/// fall back to CPU instead of failing session creation.
#[cfg(feature = "coreml")]
fn with_coreml(
    builder: ort::session::builder::SessionBuilder,
    options: CoreMlOptions,
) -> Result<ort::session::builder::SessionBuilder, TtsError> {
    use ort::ep::coreml::{ComputeUnits, ModelFormat};
    let units = match options.units {
        CoreMlUnits::All => ComputeUnits::All,
        CoreMlUnits::NeuralEngine => ComputeUnits::CPUAndNeuralEngine,
        CoreMlUnits::Gpu => ComputeUnits::CPUAndGPU,
    };
    let format = match options.format {
        CoreMlFormat::MlProgram => ModelFormat::MLProgram,
        CoreMlFormat::NeuralNetwork => ModelFormat::NeuralNetwork,
    };
    let mut coreml = ort::ep::CoreML::default()
        .with_model_format(format)
        .with_compute_units(units);
    // CoreML caches its compiled model under ~/Library/Caches by
    // default. In sandboxed/CI environments that path can be
    // unwritable, in which case CoreML degrades or fails; this knob
    // points the cache somewhere writable for measurement.
    if let Ok(dir) = std::env::var("MUSCULUS_COREML_CACHE_DIR") {
        coreml = coreml.with_model_cache_dir(dir);
    }
    let coreml = coreml.build();
    let cpu = ort::ep::CPU::default().build();
    builder
        .with_execution_providers([coreml, cpu])
        .map_err(|e| TtsError::ModelLoad(format!("coreml execution provider: {e}")))
}

/// Without the `coreml` feature the request cannot be honoured.
#[cfg(not(feature = "coreml"))]
fn with_coreml(
    _builder: ort::session::builder::SessionBuilder,
    _options: CoreMlOptions,
) -> Result<ort::session::builder::SessionBuilder, TtsError> {
    Err(TtsError::Config(
        "CoreML requested but musculus was built without the `coreml` feature".into(),
    ))
}

/// Load one session, falling back to CPU when CoreML rejects the graph.
///
/// CoreML's graph partitioning fails outright on some graphs (measured:
/// `dacvae_encoder` trips an axis-range check inside ORT), which would
/// abort session creation. The hot loop is the DiT, so the sensible
/// posture is: give every session a chance at CoreML, keep the ones
/// that accept it, and run the rest on CPU.
fn load_session_tolerant(
    dir: &Path,
    name: &str,
    ep: ExecutionProvider,
    on_coreml: &mut Vec<String>,
) -> Result<Mutex<Session>, TtsError> {
    match load_session_by_name(dir, name, ep) {
        Ok(session) => {
            if matches!(ep, ExecutionProvider::CoreMl(_)) {
                on_coreml.push(name.to_string());
            }
            Ok(session)
        }
        Err(err) if matches!(ep, ExecutionProvider::CoreMl(_)) => {
            eprintln!("[irodori] {name}: CoreML EP rejected the graph ({err}); using CPU for this session");
            Ok(load_session_by_name(dir, name, ExecutionProvider::Cpu)?)
        }
        Err(err) => Err(err),
    }
}

/// Encode the reference voice: codec encoder (wav → latent), then
/// speaker encoder (latent + mask → speaker state/mask).
fn prepare_speaker_state(
    encoder: &Mutex<Session>,
    speaker_session: &Mutex<Session>,
    padded: &[f32],
) -> Result<SpkState, TtsError> {
    let input = ndarray::Array3::from_shape_vec((1, 1, padded.len()), padded.to_vec())
        .map_err(|e| inference_error("ref reshape", e))?;
    let latent_value = Value::from_array(input).map_err(|e| inference_error("ref input", e))?;
    let latent = {
        let mut session = encoder.lock().expect("encoder lock");
        let outputs = session
            .run(vec![("wav", latent_value.into_dyn())])
            .map_err(|e| inference_error("dacvae_encoder run", e))?;
        outputs[0]
            .try_extract_array::<f32>()
            .map_err(|e| inference_error("latent extract", e))?
            .to_owned()
    };
    if latent.shape().len() != 3 || latent.shape()[2] != LATENT_DIM {
        return Err(TtsError::ModelLoad(format!(
            "unexpected latent shape {:?}",
            latent.shape()
        )));
    }
    let t_ref = latent.shape()[1];
    let latent_flat: Vec<f32> = latent.iter().copied().collect();
    let latent_arr = ndarray::Array3::from_shape_vec((1, t_ref, LATENT_DIM), latent_flat)
        .map_err(|e| inference_error("latent reshape", e))?;
    let mask_arr =
        ndarray::Array2::from_shape_vec((1, t_ref), vec![true; t_ref]).expect("mask shape");

    let latent_value =
        Value::from_array(latent_arr).map_err(|e| inference_error("ref_latent", e))?;
    let mask_value = Value::from_array(mask_arr).map_err(|e| inference_error("ref_mask", e))?;
    let mut session = speaker_session.lock().expect("speaker lock");
    let outputs = session
        .run(vec![
            ("ref_latent", latent_value.into_dyn()),
            ("ref_mask", mask_value.into_dyn()),
        ])
        .map_err(|e| inference_error("speaker run", e))?;
    let state = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| inference_error("speaker extract", e))?
        .to_owned();
    let mask_out = outputs[1]
        .try_extract_array::<bool>()
        .map_err(|e| inference_error("speaker mask extract", e))?
        .to_owned();
    Ok(SpkState {
        state: state.iter().copied().collect(),
        tspk: state.shape()[1],
        dim: state.shape()[2],
        mask: mask_out.iter().copied().collect(),
    })
}

/// Tokenize (BOS + ids, add_special_tokens off) and run the text
/// encoder → (text_state flat, S, dim).
fn encode_text(
    text_session: &Mutex<Session>,
    tokenizer: &tokenizers::Tokenizer,
    text: &str,
) -> Result<(Vec<f32>, usize, usize), TtsError> {
    let normalized = normalize_text(text);
    let encoding = tokenizer
        .encode(normalized.as_str(), false)
        .map_err(|e| inference_error("tokenize", e))?;
    let mut ids: Vec<i64> = vec![BOS];
    ids.extend(encoding.get_ids().iter().map(|&x| x as i64));
    let s = ids.len();
    let ids_arr = ndarray::Array2::from_shape_vec((1, s), ids)
        .map_err(|e| inference_error("ids reshape", e))?;
    let mask_arr = mask_from_bool(vec![true; s]);
    let ids_value = Value::from_array(ids_arr).map_err(|e| inference_error("input_ids", e))?;
    let mask_value = Value::from_array(mask_arr).map_err(|e| inference_error("text mask", e))?;
    let mut session = text_session.lock().expect("text lock");
    let outputs = session
        .run(vec![
            ("input_ids", ids_value.into_dyn()),
            ("mask", mask_value.into_dyn()),
        ])
        .map_err(|e| inference_error("text_encoder run", e))?;
    let state = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| inference_error("text extract", e))?
        .to_owned();
    let dim = state.shape()[2];
    Ok((state.iter().copied().collect(), s, dim))
}

fn mask_from_bool(mask: Vec<bool>) -> ndarray::Array2<bool> {
    ndarray::Array2::from_shape_vec((1, mask.len()), mask).expect("mask shape")
}

/// Duration predictor → clamped latent-frame count.
fn predict_duration(
    duration_session: &Mutex<Session>,
    text_state: &[f32],
    s: usize,
    dim: usize,
    spk: &SpkState,
) -> Result<usize, TtsError> {
    let text_arr = ndarray::Array3::from_shape_vec((1, s, dim), text_state.to_vec())
        .map_err(|e| inference_error("duration text reshape", e))?;
    let text_mask = mask_from_bool(vec![true; s]);
    let spk_arr = ndarray::Array3::from_shape_vec((1, spk.tspk, spk.dim), spk.state.clone())
        .map_err(|e| inference_error("duration spk reshape", e))?;
    let spk_mask = ndarray::Array2::from_shape_vec((1, spk.tspk), spk.mask.clone())
        .map_err(|e| inference_error("duration spk mask reshape", e))?;
    let aux = ndarray::Array2::from_shape_vec((1, 14), vec![0.0f32; 14])
        .map_err(|e| inference_error("aux reshape", e))?;
    let has_speaker = ndarray::Array1::from_vec(vec![true]);

    let feeds = vec![
        (
            "text_state",
            Value::from_array(text_arr)
                .map_err(|e| inference_error("text_state", e))?
                .into_dyn(),
        ),
        (
            "text_mask",
            Value::from_array(text_mask)
                .map_err(|e| inference_error("text_mask", e))?
                .into_dyn(),
        ),
        (
            "aux",
            Value::from_array(aux)
                .map_err(|e| inference_error("aux", e))?
                .into_dyn(),
        ),
        (
            "speaker_state",
            Value::from_array(spk_arr)
                .map_err(|e| inference_error("speaker_state", e))?
                .into_dyn(),
        ),
        (
            "speaker_mask",
            Value::from_array(spk_mask)
                .map_err(|e| inference_error("speaker_mask", e))?
                .into_dyn(),
        ),
        (
            "has_speaker",
            Value::from_array(has_speaker)
                .map_err(|e| inference_error("has_speaker", e))?
                .into_dyn(),
        ),
    ];
    let mut session = duration_session.lock().expect("duration lock");
    let outputs = session
        .run(feeds)
        .map_err(|e| inference_error("duration run", e))?;
    let log_frames = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| inference_error("log_frames extract", e))?
        .iter()
        .next()
        .copied()
        .unwrap_or(0.0);
    let pred_frames = (log_frames.exp_m1()) as f64;
    let min_f = (0.5 * SAMPLE_RATE as f64 / HOP as f64).ceil() as i64;
    let max_f = (30.0 * SAMPLE_RATE as f64 / HOP as f64).floor() as i64;
    let frames = (pred_frames.round() as i64).clamp(min_f, max_f);
    Ok(frames as usize)
}

/// Rectified-flow Euler + independent CFG (text + speaker), exact
/// port of the JS loop. Returns the final latent flattened [S, 32].
struct RfLoop<'a> {
    dit_session: &'a Mutex<Session>,
    text_state: &'a [f32],
    s_text: usize,
    dim_text: usize,
    spk: &'a SpkState,
    seq_len: usize,
    seed: u32,
    num_steps: usize,
}

fn rf_loop(args: RfLoop<'_>) -> Result<Vec<f32>, TtsError> {
    let RfLoop {
        dit_session,
        text_state,
        s_text,
        dim_text,
        spk,
        seq_len,
        seed,
        num_steps,
    } = args;
    let sd = seq_len * LATENT_DIM;
    let mut xt = gaussian_noise(sd, seed);

    let zeros_t = vec![0.0f32; s_text * dim_text];
    let zeros_s = vec![0.0f32; spk.tspk * spk.dim];
    let cat3 = |a: &[f32], b: &[f32], c: &[f32], n: usize| -> Vec<f32> {
        let mut o = vec![0.0f32; 3 * n];
        o[..n].copy_from_slice(a);
        o[n..2 * n].copy_from_slice(b);
        o[2 * n..].copy_from_slice(c);
        o
    };
    // batch-3 layout: [cond, text-uncond, speaker-uncond]
    let text_b = cat3(text_state, &zeros_t, text_state, s_text * dim_text);
    let text_mb: Vec<bool> = [vec![true; s_text], vec![false; s_text], vec![true; s_text]].concat();
    let spk_b = cat3(&spk.state, &spk.state, &zeros_s, spk.tspk * spk.dim);
    let spk_mb: Vec<bool> = [spk.mask.clone(), spk.mask.clone(), vec![false; spk.tspk]].concat();

    let t_sched: Vec<f32> = (0..=num_steps)
        .map(|i| (1.0 - i as f32 / num_steps as f32) * INIT_SCALE)
        .collect();

    for i in 0..num_steps {
        let t = t_sched[i];
        let dt = t_sched[i + 1] - t;
        let v: Vec<f32> = if (CFG_MIN_T..=CFG_MAX_T).contains(&t) {
            let xc = cat3(&xt, &xt, &xt, sd);
            let x_arr = ndarray::Array3::from_shape_vec((3, seq_len, LATENT_DIM), xc)
                .map_err(|e| inference_error("x_t reshape", e))?;
            let t_arr = ndarray::Array1::from_vec(vec![t, t, t]);
            let text_arr = ndarray::Array3::from_shape_vec((3, s_text, dim_text), text_b.clone())
                .map_err(|e| inference_error("text reshape", e))?;
            let text_mask_arr = ndarray::Array2::from_shape_vec((3, s_text), text_mb.clone())
                .map_err(|e| inference_error("text mask reshape", e))?;
            let spk_arr = ndarray::Array3::from_shape_vec((3, spk.tspk, spk.dim), spk_b.clone())
                .map_err(|e| inference_error("spk reshape", e))?;
            let spk_mask_arr = ndarray::Array2::from_shape_vec((3, spk.tspk), spk_mb.clone())
                .map_err(|e| inference_error("spk mask reshape", e))?;
            let feeds = vec![
                (
                    "x_t",
                    Value::from_array(x_arr)
                        .map_err(|e| inference_error("x_t", e))?
                        .into_dyn(),
                ),
                (
                    "t",
                    Value::from_array(t_arr)
                        .map_err(|e| inference_error("t", e))?
                        .into_dyn(),
                ),
                (
                    "text_state",
                    Value::from_array(text_arr)
                        .map_err(|e| inference_error("text_state", e))?
                        .into_dyn(),
                ),
                (
                    "text_mask",
                    Value::from_array(text_mask_arr)
                        .map_err(|e| inference_error("text_mask", e))?
                        .into_dyn(),
                ),
                (
                    "speaker_state",
                    Value::from_array(spk_arr)
                        .map_err(|e| inference_error("speaker_state", e))?
                        .into_dyn(),
                ),
                (
                    "speaker_mask",
                    Value::from_array(spk_mask_arr)
                        .map_err(|e| inference_error("speaker_mask", e))?
                        .into_dyn(),
                ),
            ];
            let mut session = dit_session.lock().expect("dit lock");
            let outputs = session
                .run(feeds)
                .map_err(|e| inference_error("dit run", e))?;
            let v3 = outputs[0]
                .try_extract_array::<f32>()
                .map_err(|e| inference_error("v extract", e))?
                .iter()
                .copied()
                .collect::<Vec<f32>>();
            let mut combined = vec![0.0f32; sd];
            for j in 0..sd {
                let vc = v3[j];
                combined[j] = vc + CFG_TEXT * (vc - v3[sd + j]) + CFG_SPK * (vc - v3[2 * sd + j]);
            }
            combined
        } else {
            let x_arr = ndarray::Array3::from_shape_vec((1, seq_len, LATENT_DIM), xt.clone())
                .map_err(|e| inference_error("x_t reshape", e))?;
            let t_arr = ndarray::Array1::from_vec(vec![t]);
            let text_arr =
                ndarray::Array3::from_shape_vec((1, s_text, dim_text), text_state.to_vec())
                    .map_err(|e| inference_error("text reshape", e))?;
            let text_mask_arr = mask_from_bool(vec![true; s_text]);
            let spk_arr =
                ndarray::Array3::from_shape_vec((1, spk.tspk, spk.dim), spk.state.clone())
                    .map_err(|e| inference_error("spk reshape", e))?;
            let spk_mask_arr = ndarray::Array2::from_shape_vec((1, spk.tspk), spk.mask.clone())
                .map_err(|e| inference_error("spk mask reshape", e))?;
            let feeds = vec![
                (
                    "x_t",
                    Value::from_array(x_arr)
                        .map_err(|e| inference_error("x_t", e))?
                        .into_dyn(),
                ),
                (
                    "t",
                    Value::from_array(t_arr)
                        .map_err(|e| inference_error("t", e))?
                        .into_dyn(),
                ),
                (
                    "text_state",
                    Value::from_array(text_arr)
                        .map_err(|e| inference_error("text_state", e))?
                        .into_dyn(),
                ),
                (
                    "text_mask",
                    Value::from_array(text_mask_arr)
                        .map_err(|e| inference_error("text_mask", e))?
                        .into_dyn(),
                ),
                (
                    "speaker_state",
                    Value::from_array(spk_arr)
                        .map_err(|e| inference_error("speaker_state", e))?
                        .into_dyn(),
                ),
                (
                    "speaker_mask",
                    Value::from_array(spk_mask_arr)
                        .map_err(|e| inference_error("speaker_mask", e))?
                        .into_dyn(),
                ),
            ];
            let mut session = dit_session.lock().expect("dit lock");
            let outputs = session
                .run(feeds)
                .map_err(|e| inference_error("dit run", e))?;
            outputs[0]
                .try_extract_array::<f32>()
                .map_err(|e| inference_error("v extract", e))?
                .iter()
                .copied()
                .collect()
        };
        for j in 0..sd {
            xt[j] += v[j] * dt;
        }
    }
    Ok(xt)
}

/// Decode the latent ([S, 32] flat) → 48 kHz waveform.
fn decode(
    dac_session: &Mutex<Session>,
    latent: &[f32],
    seq_len: usize,
) -> Result<Vec<f32>, TtsError> {
    // z[1, 32, S]: z[d * S + s] = latent[s * D + d]
    let mut z = vec![0.0f32; LATENT_DIM * seq_len];
    for (s_pos, slot) in latent.chunks(LATENT_DIM).enumerate() {
        for (d, &value) in slot.iter().enumerate() {
            z[d * seq_len + s_pos] = value;
        }
    }
    let z_arr = ndarray::Array3::from_shape_vec((1, LATENT_DIM, seq_len), z)
        .map_err(|e| inference_error("z reshape", e))?;
    let z_value = Value::from_array(z_arr).map_err(|e| inference_error("z", e))?;
    let mut session = dac_session.lock().expect("dac lock");
    let outputs = session
        .run(vec![("z", z_value.into_dyn())])
        .map_err(|e| inference_error("dac run", e))?;
    Ok(outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| inference_error("audio extract", e))?
        .iter()
        .copied()
        .collect())
}

impl IrodoriAdapter {
    /// Load sessions + tokenizer; encode the reference voice once.
    ///
    /// The reference WAV may be any mono WAV (44.1/48 kHz); it is
    /// resampled to 48 kHz and LUFS-normalized to −16 LUFS before
    /// encoding.
    pub fn load(models_dir: impl AsRef<Path>, ref_wav: impl AsRef<Path>) -> Result<Self, TtsError> {
        Self::load_with_ep(models_dir, ref_wav, ExecutionProvider::Cpu)
    }

    /// Load with an explicit execution provider (see [`ExecutionProvider`]).
    pub fn load_with_ep(
        models_dir: impl AsRef<Path>,
        ref_wav: impl AsRef<Path>,
        ep: ExecutionProvider,
    ) -> Result<Self, TtsError> {
        let dir = models_dir.as_ref();
        let tokenizer = tokenizers::Tokenizer::from_file(
            dir.join("tokenizer")
                .join("llmjp_tok")
                .join("tokenizer.json"),
        )
        .map_err(|e| TtsError::ModelLoad(format!("tokenizer: {e}")))?;

        let chunk = crate::wav::read_wav(ref_wav)
            .map_err(|e| TtsError::ModelLoad(format!("ref wav: {e}")))?;
        let samples = if chunk.sample_rate == SAMPLE_RATE {
            chunk.samples
        } else {
            resample_mono(&chunk.samples, chunk.sample_rate, SAMPLE_RATE)
                .map_err(|e| TtsError::ModelLoad(format!("resample ref: {e}")))?
        };
        let normalized = lufs_normalize(&samples, SAMPLE_RATE, REF_TARGET_LUFS);
        let padded_len = normalized.len().div_ceil(HOP) * HOP;
        let mut padded = vec![0.0f32; padded_len];
        padded[..normalized.len()].copy_from_slice(&normalized);

        // The closure records which sessions accepted CoreML; keep it
        // (and its borrow) inside this scope so the vector can move into
        // the adapter afterwards.
        let (text, duration, dit, dac, speaker_state, coreml_sessions) = {
            let mut coreml_sessions: Vec<String> = Vec::new();
            let mut load = |name: &str| -> Result<Mutex<Session>, TtsError> {
                load_session_tolerant(dir, name, ep, &mut coreml_sessions)
            };
            let enc = load("dacvae_encoder")?;
            let speaker_session = load("speaker_encoder")?;
            let speaker_state = prepare_speaker_state(&enc, &speaker_session, &padded)?;
            let text = load("text_encoder")?;
            let duration = load("duration")?;
            let dit = load("dit")?;
            let dac = load("dacvae_decoder")?;
            (text, duration, dit, dac, speaker_state, coreml_sessions)
        };
        Ok(Self {
            text,
            duration,
            dit,
            dac,
            tokenizer,
            speaker_ready: Mutex::new(speaker_state),
            seed: 0,
            num_steps: NUM_STEPS,
            coreml_sessions,
        })
    }

    /// Names of the sessions that accepted the CoreML EP.
    pub fn coreml_sessions(&self) -> &[String] {
        &self.coreml_sessions
    }

    /// Builder: sampling seed.
    pub fn with_seed(mut self, seed: u32) -> Self {
        self.seed = seed;
        self
    }

    /// Builder: Euler steps (default 40).
    pub fn with_num_steps(mut self, num_steps: usize) -> Self {
        self.num_steps = num_steps;
        self
    }

    /// Full synthesis: normalize → tokenize → encode → duration → RF
    /// loop → decode → 48 kHz chunk.
    fn synthesize_one(&self, text: &str, _voice: Option<&str>) -> Result<AudioChunk, TtsError> {
        let (text_state, s_text, dim_text) = encode_text(&self.text, &self.tokenizer, text)?;
        let spk = self.speaker_ready.lock().expect("spk lock").clone();
        let seq_len = predict_duration(&self.duration, &text_state, s_text, dim_text, &spk)?;
        let latent = rf_loop(RfLoop {
            dit_session: &self.dit,
            text_state: &text_state,
            s_text,
            dim_text,
            spk: &spk,
            seq_len,
            seed: self.seed,
            num_steps: self.num_steps,
        })?;
        let samples = decode(&self.dac, &latent, seq_len)?;
        Ok(AudioChunk {
            samples,
            sample_rate: SAMPLE_RATE,
        })
    }
}

#[async_trait]
impl TtsAdapter for IrodoriAdapter {
    async fn synthesize(&self, segments: &[SpeechSegment]) -> Result<Synthesis, TtsError> {
        if segments.is_empty() || segments.iter().all(|s| s.text.is_empty()) {
            return Err(TtsError::NoText);
        }
        let mut chunks = Vec::new();
        for segment in segments {
            if segment.text.is_empty() {
                continue;
            }
            // M4 spike: the adapter instance is one reference voice;
            // the voice id is accepted and ignored (documented).
            chunks.push(self.synthesize_one(&segment.text, segment.voice.as_deref())?);
        }
        if chunks.is_empty() {
            return Err(TtsError::NoText);
        }
        Ok(Synthesis { audio: chunks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_text_strips_and_folds() {
        // Dashes are removed entirely (unlike the SBV2 frontend, which
        // folds them to a hyphen); waves become the long-vowel mark.
        assert_eq!(normalize_text("Wi‑Fi"), "WiFi");
        assert_eq!(normalize_text("ラーメン〜"), "ラーメンー");
        assert_eq!(normalize_text("？"), "?");
    }

    #[test]
    fn brackets_are_peeled() {
        assert_eq!(normalize_text("「はい」"), "はい");
        assert_eq!(normalize_text("（メモ）"), "メモ");
        // Non-wrapping brackets stay.
        assert_eq!(normalize_text("「はい」「いいえ」"), "「はい」「いいえ」");
    }

    #[test]
    fn noise_is_deterministic_and_bounded() {
        let a = gaussian_noise(1000, 42);
        let b = gaussian_noise(1000, 42);
        assert_eq!(a, b);
        let peak = a.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!(peak < 4.5, "Box–Muller outlier beyond expectation: {peak}");
    }

    #[test]
    fn lufs_peak_limits_loud_input() {
        let loud = vec![0.9f32; SAMPLE_RATE as usize]; // long, loud, flat
        let out = lufs_normalize(&loud, SAMPLE_RATE, -16.0);
        let peak = out.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!(peak <= 1.0 + 1e-3, "peak must be limited, got {peak}");
    }
}
