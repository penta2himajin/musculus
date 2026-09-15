//! SBV2 (Style-Bert-VITS2 JP-Extra) `TtsAdapter` — ort-native, per
//! ADR-0002/0003. The ja frontend (`ja.rs`) is musculus's own g2p; the
//! container, style tables and symbol inventory are the MIT-licensed
//! sbv2_core data (see docs/model-licenses.md).

pub mod bundle;
pub mod ja;
pub mod ja_norm;
pub mod mora;
pub mod normalize;
pub mod symbols;
pub mod synth;

use crate::accent::AccentTable;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use ort::session::Session;
use tokenizers::Tokenizer;

use crate::traits::{SpeechNormalizer as _, TtsAdapter, TtsError};
use crate::types::{AudioChunk, SpeechSegment, Synthesis};

use bundle::{parse_sbv2file, StyleVectors};
use ja::JaFrontend;
use ja_norm::JaNormalizer;
use synth::{load_session, synthesize_vits2, Vits2Input};

/// Sample rate the released SBV2 models decode at (reference: the
/// WAV spec in sbv2_core `array_to_vec`).
pub const SAMPLE_RATE: u32 = 44100;

/// One loaded voice: a VITS2 session plus its style table.
struct Voice {
    vits2: Mutex<Session>,
    style_vectors: StyleVectors,
    /// Declared graph inputs — conversions expose sdp/length/noise
    /// inputs in varying combinations (see synth.rs).
    input_names: Vec<String>,
}

/// The SBV2 synthesis adapter.
///
/// Load a directory laid out by `scripts/setup_sbv2.sh`: every
/// `*.sbv2` in the directory becomes a voice (its file stem is the
/// voice id), sharing one deberta front-end + tokenizer. Sessions are
/// `Send`-only under ort, hence the mutexes.
///
/// Synthesis runs synchronously inside the async call (ort inference
/// has no async API); CLI and batch workloads are the intended callers
/// for 0.x, the same posture as euhadra's ONNX ASR adapters.
pub struct Sbv2Adapter {
    frontend: JaFrontend,
    /// musculus-owned normalization (dates, symbols, time) — runs
    /// before the frontend, whose inventory cannot read the symbols.
    normalizer: JaNormalizer,
    tokenizer: Tokenizer,
    bert: Mutex<Session>,
    voices: BTreeMap<String, Voice>,
    /// Neutral by default; style ids are per-voice (0 = neutral mean).
    style_id: i32,
    style_weight: f32,
    sdp_ratio: f32,
    length_scale: f32,
    /// User-owned accent overrides (ADR-0006).
    accent: AccentTable,
    /// Apply musculus's deliberate accent deviations (on by default;
    /// validated by ear — docs/accent-resources.md).
    accent_deviations: bool,
}

impl Sbv2Adapter {
    /// Load every `*.sbv2` voice in `dir`, plus `tokenizer.json` and
    /// `deberta.onnx`.
    pub fn load_dir<P: AsRef<Path>>(dir: P) -> Result<Self, TtsError> {
        Self::load_dir_with_user_dictionary(dir, None)
    }

    /// Load a model directory, optionally with a user dictionary that
    /// overrides accent assignment (a standard-accent dictionary generated
    /// offline; docs/accent-resources.md).
    pub fn load_dir_with_user_dictionary<P: AsRef<Path>>(
        dir: P,
        user_dictionary: Option<&Path>,
    ) -> Result<Self, TtsError> {
        let dir = dir.as_ref();
        let tokenizer_bytes = std::fs::read(dir.join("tokenizer.json"))
            .map_err(|e| TtsError::ModelLoad(format!("tokenizer.json: {e}")))?;
        let tokenizer = Tokenizer::from_bytes(&tokenizer_bytes)
            .map_err(|e| TtsError::ModelLoad(format!("tokenizer: {e}")))?;
        let bert_bytes = std::fs::read(dir.join("deberta.onnx"))
            .map_err(|e| TtsError::ModelLoad(format!("deberta.onnx: {e}")))?;
        let bert = load_session("deberta", &bert_bytes)?;

        let mut voices = BTreeMap::new();
        let entries = std::fs::read_dir(dir)
            .map_err(|e| TtsError::ModelLoad(format!("read {}: {e}", dir.display())))?;
        for entry in entries {
            let path = entry
                .map_err(|e| TtsError::ModelLoad(format!("read dir entry: {e}")))?
                .path();
            let is_sbv2 = path.extension().is_some_and(|ext| ext == "sbv2");
            if !is_sbv2 {
                continue;
            }
            let ident = path.file_stem().map_or_else(
                || "unknown".to_string(),
                |s| s.to_string_lossy().to_string(),
            );
            let bytes = std::fs::read(&path)
                .map_err(|e| TtsError::ModelLoad(format!("{}: {e}", path.display())))?;
            let parsed = parse_sbv2file(&bytes)?;
            let vits2 = load_session(&ident, &parsed.model_onnx)?;
            let input_names: Vec<String> = {
                let session = vits2
                    .lock()
                    .map_err(|e| TtsError::ModelLoad(format!("{ident} session lock: {e}")))?;
                session
                    .inputs()
                    .iter()
                    .map(|i| i.name().to_string())
                    .collect()
            };
            voices.insert(
                ident,
                Voice {
                    vits2,
                    style_vectors: parsed.style_vectors,
                    input_names,
                },
            );
        }
        if voices.is_empty() {
            return Err(TtsError::ModelLoad(format!(
                "no .sbv2 voices found in {} (run scripts/setup_sbv2.sh)",
                dir.display()
            )));
        }

        Ok(Self {
            frontend: match user_dictionary {
                Some(path) => JaFrontend::with_user_dictionary(path),
                None => JaFrontend::new(),
            }
            .map_err(|e| TtsError::ModelLoad(format!("ja frontend: {e}")))?,
            normalizer: JaNormalizer::new(),
            tokenizer,
            bert,
            voices,
            style_id: 0,
            style_weight: 1.0,
            sdp_ratio: 0.0,
            length_scale: 1.0,
            accent: AccentTable::default(),
            accent_deviations: true,
        })
    }

    /// Voice ids available in this adapter, sorted.
    pub fn voice_names(&self) -> Vec<String> {
        self.voices.keys().cloned().collect()
    }

    /// Builder: enable or disable musculus's deliberate accent deviations
    /// from the reference frontend (docs/accent-resources.md). On by
    /// default; disable for a reference-faithful reading.
    pub fn with_accent_deviations(mut self, enabled: bool) -> Self {
        self.accent_deviations = enabled;
        self
    }

    /// Builder: apply user-owned accent overrides to the H/L feature
    /// before it is encoded (the frontend's accent estimation is
    /// documented to miss numeral compounds; ADR-0006).
    pub fn with_accent_table(mut self, accent: AccentTable) -> Self {
        self.accent = accent;
        self
    }

    /// Builder: decode with a specific style id.
    pub fn with_style_id(mut self, style_id: i32) -> Self {
        self.style_id = style_id;
        self
    }

    /// Builder: blend weight between the neutral mean (0) and the raw
    /// style (1).
    pub fn with_style_weight(mut self, style_weight: f32) -> Self {
        self.style_weight = style_weight;
        self
    }

    /// Builder: stochastic duration predictor mixing (0 = off).
    pub fn with_sdp_ratio(mut self, sdp_ratio: f32) -> Self {
        self.sdp_ratio = sdp_ratio;
        self
    }

    /// Builder: speech speed. >1 speaks slower, <1 faster.
    pub fn with_length_scale(mut self, length_scale: f32) -> Self {
        self.length_scale = length_scale;
        self
    }

    fn default_voice(&self) -> Option<&String> {
        self.voices.keys().next()
    }
}

// ---------------------------------------------------------------------------
// Text → model inputs
// ---------------------------------------------------------------------------

/// Model inputs for one segment, ready for the VITS2 decode pass.
struct ParsedPhones {
    phones: Vec<i64>,
    tones: Vec<i64>,
    lang_ids: Vec<i64>,
    /// Phone-level BERT features, `[hidden, phones]`.
    bert: ndarray::Array2<f32>,
}

/// `(phone ids, tone ids, language ids)` for one segment.
type SequenceIds = (Vec<i64>, Vec<i64>, Vec<i64>);

/// `sep` between every element: `[a, b]` → `[0, a, 0, b, 0]`.
fn intersperse<T: Copy>(slice: &[T], sep: T) -> Vec<T> {
    let mut result = vec![sep; slice.len() * 2 + 1];
    for (i, value) in slice.iter().enumerate() {
        result[2 * i + 1] = *value;
    }
    result
}

/// Phoneme strings → model ids. Tones shift by 6 into the model's
/// range; language ids are all 1 (Japanese).
fn to_sequence(phones: &[String], tones: &[i32]) -> Result<SequenceIds, TtsError> {
    let phone_ids = phones
        .iter()
        .map(|p| {
            symbols::symbol_to_id(p).ok_or_else(|| {
                TtsError::Inference(format!("phoneme not in trained inventory: {p:?}"))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let tone_ids = tones.iter().map(|t| (*t + 6) as i64).collect();
    let lang_ids = vec![1; phone_ids.len()];
    Ok((phone_ids, tone_ids, lang_ids))
}

/// BERT tokenization: CLS + per-character tokens + SEP, char-by-char.
fn tokenize_text(tokenizer: &Tokenizer, text: &str) -> Result<(Vec<i64>, Vec<i64>), TtsError> {
    let mut token_ids = vec![1];
    let mut attention_masks = vec![1];
    for ch in text.chars() {
        let encoding = tokenizer
            .encode(ch.to_string(), false)
            .map_err(|e| TtsError::Inference(format!("tokenize {ch:?}: {e}")))?;
        token_ids.extend(encoding.get_ids().iter().map(|&x| x as i64));
        attention_masks.extend(encoding.get_attention_mask().iter().map(|&x| x as i64));
    }
    token_ids.push(2);
    attention_masks.push(1);
    Ok((token_ids, attention_masks))
}

impl Sbv2Adapter {
    fn parse_text(&self, text: &str) -> Result<ParsedPhones, TtsError> {
        // musculus-owned stage first: the frontend's inventory cannot
        // read ¥/%/℃ or date compounds; rewrite before it sees them.
        let normalized = self
            .normalizer
            .normalize(text)
            .map_err(|e| TtsError::Inference(e.to_string()))?
            .text;
        let read = self
            .frontend
            .num2word(&normalized)
            .map_err(|e| TtsError::Inference(e.to_string()))?;
        let normalized = normalize::normalize_text(&read);
        let mut process = self
            .frontend
            .process_text(&normalized)
            .map_err(|e| TtsError::Inference(e.to_string()))?;
        // musculus's deliberate accent deviations, on top of the faithful
        // reference frontend — opt-in while the encodings are still being
        // validated by ear (docs/accent-resources.md).
        if self.accent_deviations {
            process
                .apply_accent_deviations()
                .map_err(|e| TtsError::Inference(e.to_string()))?;
        }
        let (phones, tones, mut word2ph) = process
            .g2p()
            .map_err(|e| TtsError::Inference(e.to_string()))?;
        // User-owned accent overrides replace the frontend's H/L where
        // they match, before anything is encoded.
        let tones = self.accent.apply(&phones, &tones);

        let (phone_ids, tone_ids, lang_ids) = to_sequence(&phones, &tones)?;
        let phones = intersperse(&phone_ids, 0);
        let tones = intersperse(&tone_ids, 0);
        let lang_ids = intersperse(&lang_ids, 0);
        for count in word2ph.iter_mut() {
            *count *= 2;
        }
        word2ph[0] += 1;

        let (seq_text, _) = process
            .text_to_seq_kata()
            .map_err(|e| TtsError::Inference(e.to_string()))?;
        let bert_text = seq_text.concat();
        let (token_ids, attention_masks) = tokenize_text(&self.tokenizer, &bert_text)?;
        let bert_content = synth::predict_bert(&self.bert, &token_ids, &attention_masks)?;

        if word2ph.len() != bert_text.chars().count() + 2 {
            return Err(TtsError::Inference(format!(
                "word2ph {} does not match BERT characters {}",
                word2ph.len(),
                bert_text.chars().count()
            )));
        }

        // Repeat each character's BERT row word2ph[i] times so the
        // feature stream aligns with the (interspersed) phoneme stream.
        let hidden = bert_content.ncols();
        let total: usize = word2ph.iter().map(|&c| c as usize).sum();
        if total != phones.len() {
            return Err(TtsError::Inference(format!(
                "bert repetition total {total} != phone stream length {}",
                phones.len()
            )));
        }
        let mut phone_level = ndarray::Array2::<f32>::zeros((total, hidden));
        let mut offset = 0;
        for (row, &reps) in bert_content.rows().into_iter().zip(word2ph.iter()) {
            for _ in 0..reps {
                phone_level.slice_mut(ndarray::s![offset, ..]).assign(&row);
                offset += 1;
            }
        }
        Ok(ParsedPhones {
            phones,
            tones,
            lang_ids,
            bert: phone_level.t().to_owned(),
        })
    }
}

// ---------------------------------------------------------------------------
// TtsAdapter
// ---------------------------------------------------------------------------

#[async_trait]
impl TtsAdapter for Sbv2Adapter {
    async fn synthesize(&self, segments: &[SpeechSegment]) -> Result<Synthesis, TtsError> {
        if segments.is_empty() || segments.iter().all(|s| s.text.is_empty()) {
            return Err(TtsError::NoText);
        }

        let mut chunks = Vec::new();
        for segment in segments {
            if segment.text.is_empty() {
                continue;
            }
            let ident = match &segment.voice {
                Some(name) if !self.voices.contains_key(name) => {
                    return Err(TtsError::Unsupported(format!(
                        "voice {name:?} not loaded (available: {:?})",
                        self.voice_names()
                    )));
                }
                Some(name) => name,
                None => self
                    .default_voice()
                    .expect("non-empty voice map checked at load"),
            };
            let voice = &self.voices[ident];
            let style_vector = voice
                .style_vectors
                .vector(self.style_id, self.style_weight)?;

            let parsed = self.parse_text(&segment.text)?;
            let audio = synthesize_vits2(
                &voice.vits2,
                &Vits2Input {
                    phones: &parsed.phones,
                    tones: &parsed.tones,
                    lang_ids: &parsed.lang_ids,
                    bert: parsed.bert,
                    style_vector,
                    speaker_id: 0,
                    sdp_ratio: self.sdp_ratio,
                    length_scale: self.length_scale,
                },
                &voice.input_names,
            )?;
            let samples = audio.slice(ndarray::s![0, 0, ..]).to_vec();
            chunks.push(AudioChunk {
                samples,
                sample_rate: SAMPLE_RATE,
            });
        }

        if chunks.is_empty() {
            return Err(TtsError::NoText);
        }
        Ok(Synthesis { audio: chunks })
    }
}
