//! ONNX session loading and inference for the SBV2 pipeline.
//!
//! Our own code over `ort` rc.13, following the session idioms euhadra
//! already validated (values owned, feeds by name, outputs positional).
//! CPU execution provider only for M1; acceleration EPs are a later
//! decision measured against the RTF baseline.

use std::sync::Mutex;

use ndarray::{Array1, Array2, Array3, Axis, Ix2, Ix3};
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Value;

use crate::traits::TtsError;

fn load_error(what: &str, e: impl std::fmt::Display) -> TtsError {
    TtsError::ModelLoad(format!("{what}: {e}"))
}

fn inference_error(what: &str, e: impl std::fmt::Display) -> TtsError {
    TtsError::Inference(format!("{what}: {e}"))
}

/// Build a CPU-session from in-memory ONNX bytes.
///
/// Sessions are `Send` but not `Sync` under ort rc.13, so callers hold
/// them in `Mutex`es — same posture as euhadra's adapters.
pub fn load_session(what: &str, bytes: &[u8]) -> Result<Mutex<Session>, TtsError> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let session = Session::builder()
        .map_err(|e| load_error(what, e))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| load_error(what, e))?
        .with_intra_threads(threads)
        .map_err(|e| load_error(what, e))?
        .commit_from_memory(bytes)
        .map_err(|e| load_error(what, e))?;
    Ok(Mutex::new(session))
}

/// Run the deberta front-end model: token ids + attention mask →
/// per-character contextual features `[seq, hidden]`.
pub fn predict_bert(
    session: &Mutex<Session>,
    token_ids: &[i64],
    attention_masks: &[i64],
) -> Result<Array2<f32>, TtsError> {
    let input_ids = Array2::from_shape_vec((1, token_ids.len()), token_ids.to_vec())
        .map_err(|e| inference_error("bert input_ids reshape", e))?;
    let mask = Array2::from_shape_vec((1, attention_masks.len()), attention_masks.to_vec())
        .map_err(|e| inference_error("bert attention_mask reshape", e))?;

    let ids_v = Value::from_array(input_ids).map_err(|e| inference_error("bert input_ids", e))?;
    let mask_v = Value::from_array(mask).map_err(|e| inference_error("bert attention_mask", e))?;

    let mut session = session
        .lock()
        .map_err(|e| inference_error("bert session lock", e))?;
    let outputs = session
        .run(vec![
            ("input_ids", ids_v.into_dyn()),
            ("attention_mask", mask_v.into_dyn()),
        ])
        .map_err(|e| inference_error("bert run", e))?;
    let output = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| inference_error("bert extract output", e))?
        .to_owned()
        .into_dimensionality::<Ix2>()
        .map_err(|e| inference_error("bert output rank", e))?;
    Ok(output)
}

/// One synthesis request for the VITS2 decode model.
pub struct Vits2Input<'a> {
    /// Interspersed phoneme ids `[1 + 2n + 1]`.
    pub phones: &'a [i64],
    /// Interspersed tones, same length as `phones`.
    pub tones: &'a [i64],
    /// Language ids, same length as `phones` (all 1 for Japanese).
    pub lang_ids: &'a [i64],
    /// Phone-level BERT features `[hidden, phones]`.
    pub bert: Array2<f32>,
    /// Style vector (one row of the style table).
    pub style_vector: Vec<f32>,
    pub speaker_id: i64,
    pub sdp_ratio: f32,
    pub length_scale: f32,
    pub noise_scale: f32,
    pub noise_scale_w: f32,
}

/// Run the VITS2 decode model → audio `[1, 1, samples]`.
///
/// Feeds adapt to the model's declared inputs: conversions expose
/// `sdp_ratio`/`length_scale` (older) plus `noise_scale`/`noise_scale_w`
/// (newer) in varying combinations, so anything not in `input_names` is
/// skipped and its constant applied inside the graph.
///
/// The default decoding constants `noise_scale = 0.677`,
/// `noise_scale_w = 0.8`, `sdp_ratio = 0.0` are the Rust reference's
/// (`sbv2_core::easy_synthesize`); upstream Style-Bert-VITS2 ships
/// `0.6 / 0.8 / 0.2` instead, and each is fed only when the model declares
/// the input (otherwise the value lives inside the graph).
pub fn synthesize_vits2(
    session: &Mutex<Session>,
    input: &Vits2Input<'_>,
    input_names: &[String],
) -> Result<Array3<f32>, TtsError> {
    let phones_len = input.phones.len();
    let x_tst = Array2::from_shape_vec((1, phones_len), input.phones.to_vec())
        .map_err(|e| inference_error("vits2 x_tst reshape", e))?;
    let tones = Array2::from_shape_vec((1, phones_len), input.tones.to_vec())
        .map_err(|e| inference_error("vits2 tones reshape", e))?;
    let language = Array2::from_shape_vec((1, phones_len), input.lang_ids.to_vec())
        .map_err(|e| inference_error("vits2 language reshape", e))?;
    let x_tst_lengths = Array1::from_vec(vec![phones_len as i64]);
    let sid = Array1::from_vec(vec![input.speaker_id]);
    let style_vec =
        Array2::from_shape_vec((1, input.style_vector.len()), input.style_vector.clone())
            .map_err(|e| inference_error("vits2 style_vec reshape", e))?;
    // bert arrives as [hidden, phones]; the model wants [1, hidden, phones].
    let bert = input
        .bert
        .clone()
        .insert_axis(Axis(0))
        .as_standard_layout()
        .into_owned();

    // Required inputs: their absence is a conversion we cannot drive.
    let declared = |name: &str| input_names.iter().any(|n| n == name);
    for name in [
        "x_tst",
        "x_tst_lengths",
        "tones",
        "language",
        "bert",
        "style_vec",
    ] {
        if !declared(name) {
            return Err(TtsError::ModelLoad(format!(
                "vits2 model lacks required input {name:?} (declared: {input_names:?})"
            )));
        }
    }

    let mut feeds: Vec<(&'static str, _)> = Vec::new();
    if declared("x_tst") {
        feeds.push((
            "x_tst",
            Value::from_array(x_tst)
                .map_err(|e| inference_error("vits2 x_tst", e))?
                .into_dyn(),
        ));
    }
    if declared("x_tst_lengths") {
        feeds.push((
            "x_tst_lengths",
            Value::from_array(x_tst_lengths)
                .map_err(|e| inference_error("vits2 lengths", e))?
                .into_dyn(),
        ));
    }
    if declared("sid") {
        feeds.push((
            "sid",
            Value::from_array(sid)
                .map_err(|e| inference_error("vits2 sid", e))?
                .into_dyn(),
        ));
    }
    if declared("tones") {
        feeds.push((
            "tones",
            Value::from_array(tones)
                .map_err(|e| inference_error("vits2 tones", e))?
                .into_dyn(),
        ));
    }
    if declared("language") {
        feeds.push((
            "language",
            Value::from_array(language)
                .map_err(|e| inference_error("vits2 language", e))?
                .into_dyn(),
        ));
    }
    if declared("bert") {
        feeds.push((
            "bert",
            Value::from_array(bert)
                .map_err(|e| inference_error("vits2 bert", e))?
                .into_dyn(),
        ));
    }
    if declared("style_vec") {
        feeds.push((
            "style_vec",
            Value::from_array(style_vec)
                .map_err(|e| inference_error("vits2 style_vec", e))?
                .into_dyn(),
        ));
    }
    if declared("sdp_ratio") {
        feeds.push((
            "sdp_ratio",
            Value::from_array(Array1::from_vec(vec![input.sdp_ratio]))
                .map_err(|e| inference_error("vits2 sdp_ratio", e))?
                .into_dyn(),
        ));
    }
    if declared("length_scale") {
        feeds.push((
            "length_scale",
            Value::from_array(Array1::from_vec(vec![input.length_scale]))
                .map_err(|e| inference_error("vits2 length_scale", e))?
                .into_dyn(),
        ));
    }
    if declared("noise_scale") {
        feeds.push((
            "noise_scale",
            Value::from_array(Array1::from_vec(vec![input.noise_scale]))
                .map_err(|e| inference_error("vits2 noise_scale", e))?
                .into_dyn(),
        ));
    }
    if declared("noise_scale_w") {
        feeds.push((
            "noise_scale_w",
            Value::from_array(Array1::from_vec(vec![input.noise_scale_w]))
                .map_err(|e| inference_error("vits2 noise_scale_w", e))?
                .into_dyn(),
        ));
    }

    let mut session = session
        .lock()
        .map_err(|e| inference_error("vits2 session lock", e))?;
    let outputs = session
        .run(feeds)
        .map_err(|e| inference_error("vits2 run", e))?;
    let audio = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| inference_error("vits2 extract output", e))?
        .to_owned()
        .into_dimensionality::<Ix3>()
        .map_err(|e| inference_error("vits2 output rank", e))?;
    Ok(audio)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_load_of_garbage_is_a_model_load_error() {
        let err = load_session("garbage", b"not onnx").unwrap_err();
        assert!(matches!(err, TtsError::ModelLoad(_)));
    }
}
