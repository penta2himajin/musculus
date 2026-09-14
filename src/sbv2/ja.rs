//! Japanese grapheme-to-phoneme conversion for the SBV2 frontend.
//!
//! PROVENANCE (docs/model-licenses.md): this module reimplements the
//! prosody-driven g2p algorithm used by litagin02's Style-Bert-VITS2
//! `g2p.py` and, before it, VOICEVOX's engine — an algorithm, not code.
//! sbv2_core's Rust rendition (`jtalk.rs`) carries an LGPL-3.0 header
//! and is **not** copied: every function here is written against the
//! documented behaviour, on musculus's own types, error handling and
//! naming. The data it consumes (mora tables, symbol inventory) comes
//! from the MIT sbv2_core crate.
//!
//! The pipeline per text unit:
//!
//! 1. `num2word` — numerals → readable words via jpreprocess/NJD
//! 2. `normalize_text` — punctuation unification (see normalize.rs)
//! 3. `run_frontend` — NJD node parse, then prosody labels
//! 4. `g2p` — phones + accent tones + word2phone counts for BERT

use std::sync::{Arc, LazyLock};

use jpreprocess::{kind, DefaultTokenizer, JPreprocess, SystemDictionaryConfig};
use regex::Regex;

use crate::sbv2::mora::{
    CONSONANTS, MORA_KATA_TO_MORA_PHONEMES, MORA_PHONEMES_TO_MORA_KATA, VOWELS,
};
use crate::sbv2::normalize::replace_punctuation;
use crate::sbv2::symbols::PUNCTUATIONS;

/// Why the Japanese frontend failed.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum JaError {
    /// The dictionary or NJD pipeline rejected the input.
    #[error("jpreprocess failed: {0}")]
    Jpreprocess(String),

    /// The frontend produced something inconsistent with the trained
    /// phoneme inventory — e.g. a non-katakana reading.
    #[error("invalid frontend output: {0}")]
    ValueError(String),
}

type JPreprocessType = JPreprocess<DefaultTokenizer>;

/// `(phones, tones, word2ph)` — the g2p triple.
type G2pOutput = (Vec<String>, Vec<i32>, Vec<i32>);

/// The Japanese text frontend: jpreprocess dictionary + parse entry
/// points. Cheap to share across syntheses behind an `Arc`.
pub struct JaFrontend {
    jpreprocess: Arc<JPreprocessType>,
}

impl JaFrontend {
    /// Build the frontend with the bundled NAIST-JDIC dictionary
    /// (BSD-licensed — see docs/model-licenses.md §1; the AGPL
    /// dictionary feature of the reference implementation is never
    /// enabled here).
    pub fn new() -> Result<Self, JaError> {
        let sdic = SystemDictionaryConfig::Bundled(kind::JPreprocessDictionaryKind::NaistJdic)
            .load()
            .map_err(|e| JaError::Jpreprocess(e.to_string()))?;
        let jpreprocess = JPreprocess::with_dictionaries(sdic, None);
        Ok(Self {
            jpreprocess: Arc::new(jpreprocess),
        })
    }

    /// Rewrite numerals (and a few other NJD-handled forms) into their
    /// readable word forms before normalization.
    pub fn num2word(&self, text: &str) -> Result<String, JaError> {
        let mut njd = self
            .jpreprocess
            .text_to_njd(text)
            .map_err(|e| JaError::Jpreprocess(e.to_string()))?;
        njd.preprocess();
        Ok(njd
            .nodes
            .iter()
            .map(|node| node.get_string().to_string())
            .collect())
    }

    /// Parse normalized text into a [`JaProcess`] for g2p.
    pub fn process_text(&self, text: &str) -> Result<JaProcess, JaError> {
        let parsed = self
            .jpreprocess
            .run_frontend(text)
            .map_err(|e| JaError::Jpreprocess(e.to_string()))?;
        Ok(JaProcess {
            jpreprocess: Arc::clone(&self.jpreprocess),
            parsed,
        })
    }
}

/// One parsed text unit, ready for [`JaProcess::g2p`].
pub struct JaProcess {
    #[allow(dead_code)] // held for future interactive tone overrides
    jpreprocess: Arc<JPreprocessType>,
    parsed: Vec<String>,
}

// ---------------------------------------------------------------------------
// Katakana → phonemes
// ---------------------------------------------------------------------------

static KATAKANA_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\u{30A0}-\u{30FF}]+").expect("katakana pattern is valid"));
static LONG_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\w)(ー*)").expect("long-vowel pattern is valid"));

/// Katakana moras sorted longest-first, so `キャ` wins over `カ` + `ャ`.
static MORA_PATTERN: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut keys: Vec<String> = MORA_KATA_TO_MORA_PHONEMES.keys().cloned().collect();
    keys.sort_by_key(|k| std::cmp::Reverse(k.chars().count()));
    keys
});

/// Convert one katakana word (or punctuation run) to phonemes.
pub fn kata_to_phoneme_list(text: String) -> Result<Vec<String>, JaError> {
    let chars: std::collections::HashSet<String> = text.chars().map(|c| c.to_string()).collect();
    if chars.iter().all(|c| PUNCTUATIONS.contains(&c.as_str())) {
        return Ok(text.chars().map(|c| c.to_string()).collect());
    }
    if !KATAKANA_PATTERN.is_match(&text) {
        return Err(JaError::ValueError(format!(
            "input must be katakana only: {text}"
        )));
    }

    let mut replaced = text;
    for mora in MORA_PATTERN.iter() {
        let Some((consonant, vowel)) = MORA_KATA_TO_MORA_PHONEMES.get(mora) else {
            continue;
        };
        replaced = match consonant {
            None => replaced.replace(mora, &format!(" {vowel}")),
            Some(consonant) => replaced.replace(mora, &format!(" {consonant} {vowel}")),
        };
    }

    // 長音記号 (ー) repeats the preceding vowel: 「ラーメン」 → ra a me N.
    replaced = LONG_PATTERN
        .replace_all(&replaced, |caps: &regex::Captures| {
            let first = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let repeat = caps.get(2).map_or("", |m| m.as_str()).chars().count();
            let mut result = first.clone();
            for _ in 0..repeat {
                result.push(' ');
                result.push_str(&first);
            }
            result
        })
        .to_string();

    Ok(replaced.trim().split(' ').map(str::to_string).collect())
}

/// Resolve lingering 長音記号 against the preceding mora's vowel.
///
/// Deviation from the reference, documented: a leading `ー` has no
/// previous mora to prolong, and the reference would index out of
/// bounds there. We leave it in place; it later surfaces as an unknown
/// symbol with a clear message instead of a panic.
fn handle_long(mut sep_phonemes: Vec<Vec<String>>) -> Vec<Vec<String>> {
    for i in 0..sep_phonemes.len() {
        if sep_phonemes[i].is_empty() {
            continue;
        }
        if sep_phonemes[i][0] == "ー" && i != 0 {
            if let Some(prev_last) = sep_phonemes[i - 1].last() {
                if VOWELS.contains(&prev_last.as_str()) {
                    sep_phonemes[i][0] = prev_last.clone();
                }
            }
        }
        for e in 1..sep_phonemes[i].len() {
            if sep_phonemes[i][e] == "ー" {
                if let Some(prev_last) = sep_phonemes[i][e - 1].chars().last() {
                    sep_phonemes[i][e] = prev_last.to_string();
                }
            }
        }
    }
    sep_phonemes
}

/// Spread `n_phone` phonemes across `n_word` characters as evenly as
/// possible, greedy-min fill (BERT feature repetition counts).
fn distribute_phone(n_phone: i32, n_word: i32) -> Vec<i32> {
    let mut phones_per_word = vec![0; n_word.max(0) as usize];
    for _ in 0..n_phone.max(0) {
        let (min_index, _) = phones_per_word
            .iter()
            .enumerate()
            .min_by_key(|&(_, &count)| count)
            .expect("non-empty word buffer");
        phones_per_word[min_index] += 1;
    }
    phones_per_word
}

impl JaProcess {
    /// NJD nodes → (surface text units, katakana readings).
    ///
    /// Node fields arrive as comma-joined strings: field 0 is the
    /// surface, field 9 the reading (katakana with accent marks).
    pub fn text_to_seq_kata(&self) -> Result<(Vec<String>, Vec<String>), JaError> {
        let mut seq_text = Vec::with_capacity(self.parsed.len());
        let mut seq_kata = Vec::with_capacity(self.parsed.len());

        for parts in &self.parsed {
            let (string, pron) = parse_to_string_and_pron(parts.clone());
            let mut yomi = pron.replace('’', "");
            let word = replace_punctuation(&string);
            if yomi.is_empty() {
                return Err(JaError::ValueError(format!("empty reading for: {word}")));
            }
            if yomi == "、" {
                let all_punctuation = word
                    .chars()
                    .all(|c| PUNCTUATIONS.contains(&c.to_string().as_str()));
                yomi = if all_punctuation {
                    word.clone()
                } else {
                    "'".repeat(word.chars().count())
                };
            } else if yomi == "？" {
                if word != "?" {
                    return Err(JaError::ValueError(format!(
                        "reading `？` must come from `?`, got: {word}"
                    )));
                }
                yomi = "?".to_string();
            }
            seq_text.push(word);
            seq_kata.push(yomi);
        }
        Ok((seq_text, seq_kata))
    }

    /// Phones + tones from prosody labels, punctuation excluded.
    ///
    /// Accent marks: `^` utterance start, `$`/`?` end (interrogative),
    /// `_` pause, `#` accent-phrase boundary, `[`/`]` accent step.
    fn g2phone_tone_wo_punct(&self) -> Result<Vec<(String, i32)>, JaError> {
        let prosodies = self.g2p_prosody()?;

        let mut results: Vec<(String, i32)> = Vec::new();
        let mut current_phrase: Vec<(String, i32)> = Vec::new();
        let mut current_tone = 0;

        for (i, letter) in prosodies.iter().enumerate() {
            if letter == "^" {
                if i != 0 {
                    return Err(JaError::ValueError("`^` must open the utterance".into()));
                }
            } else if matches!(letter.as_str(), "$" | "?" | "_" | "#") {
                results.extend(Self::fix_phone_tone(std::mem::take(&mut current_phrase))?);
                if matches!(letter.as_str(), "$" | "?") && i != prosodies.len() - 1 {
                    return Err(JaError::ValueError(
                        "utterance-end marker must be last".into(),
                    ));
                }
                current_tone = 0;
            } else if letter == "[" {
                current_tone += 1;
            } else if letter == "]" {
                current_tone -= 1;
            } else {
                let new_letter = if letter == "cl" {
                    "q".to_string()
                } else {
                    letter.clone()
                };
                current_phrase.push((new_letter, current_tone));
            }
        }
        Ok(results)
    }

    /// NJD labels → the OpenJTalk-style prosody token stream.
    fn g2p_prosody(&self) -> Result<Vec<String>, JaError> {
        let labels = self.jpreprocess.make_label(self.parsed.clone());

        let mut phones: Vec<String> = Vec::new();
        for (i, label) in labels.iter().enumerate() {
            let mut p3 = label
                .phoneme
                .c
                .clone()
                .ok_or_else(|| JaError::ValueError(format!("label {i} lacks phoneme")))?;
            if "AIUEO".contains(&p3) {
                p3 = p3.to_lowercase();
            }
            if p3 == "sil" {
                if i == 0 {
                    phones.push("^".to_string());
                } else if i == labels.len() - 1 {
                    let interrogative = label
                        .accent_phrase_prev
                        .as_ref()
                        .is_some_and(|ap| ap.is_interrogative);
                    phones.push(if interrogative { "$" } else { "?" }.to_string());
                } else {
                    return Err(JaError::ValueError(format!(
                        "unexpected sil at position {i}"
                    )));
                }
                continue;
            } else if p3 == "pau" {
                phones.push("_".to_string());
                continue;
            }
            phones.push(p3.clone());

            let a1 = label
                .mora
                .as_ref()
                .map_or(-50, |m| m.relative_accent_position as i32);
            let a2 = label
                .mora
                .as_ref()
                .map_or(-50, |m| m.position_forward as i32);
            let a3 = label
                .mora
                .as_ref()
                .map_or(-50, |m| m.position_backward as i32);
            let f1 = label
                .accent_phrase_curr
                .as_ref()
                .map_or(-50, |ap| ap.mora_count as i32);
            let a2_next = labels
                .get(i + 1)
                .and_then(|next| next.mora.as_ref())
                .map_or(-50, |m| m.position_forward as i32);

            if a3 == 1 && a2_next == 1 && "aeiouAEIOUNcl".contains(&p3) {
                phones.push("#".to_string());
            } else if a1 == 0 && a2_next == a2 + 1 && a2 != f1 {
                phones.push("]".to_string());
            } else if a2 == 1 && a2_next == 2 {
                phones.push("[".to_string());
            }
        }
        Ok(phones)
    }

    /// Human-readable dump of the prosody labels driving the tones.
    ///
    /// Diagnostic for the accent work: shows where each accent phrase
    /// starts/ends and the accent positions, i.e. *why* a tone came out
    /// the way it did (docs/benchmarks/accent/ja-report.md).
    pub fn label_dump(&self) -> Result<Vec<String>, JaError> {
        let labels = self.jpreprocess.make_label(self.parsed.clone());
        Ok(labels
            .iter()
            .map(|label| {
                let phoneme = label.phoneme.c.clone().unwrap_or_else(|| "?".into());
                let (a1, a2, a3) = label
                    .mora
                    .as_ref()
                    .map(|m| {
                        (
                            m.relative_accent_position as i32,
                            m.position_forward as i32,
                            m.position_backward as i32,
                        )
                    })
                    .unwrap_or((-50, -50, -50));
                let f1 = label
                    .accent_phrase_curr
                    .as_ref()
                    .map(|a| a.mora_count as i32)
                    .unwrap_or(-50);
                let f2 = label
                    .accent_phrase_curr
                    .as_ref()
                    .map(|a| a.accent_position as i32)
                    .unwrap_or(-50);
                format!("{phoneme:>4} a1={a1:>3} a2={a2:>3} a3={a3:>3} f1={f1:>2} f2={f2:>2}")
            })
            .collect())
    }

    /// Full g2p: (phones, tones, word2ph).
    ///
    /// `phones`/`tones` carry boundary pads; `word2ph` (one entry per
    /// text character plus the two pads) says how many BERT features
    /// each character contributes.
    pub fn g2p(&self) -> Result<G2pOutput, JaError> {
        let phone_tone_list_wo_punct = self.g2phone_tone_wo_punct()?;
        let (seq_text, seq_kata) = self.text_to_seq_kata()?;

        let sep_phonemes = handle_long(
            seq_kata
                .iter()
                .map(|kata| kata_to_phoneme_list(kata.clone()))
                .collect::<Result<Vec<_>, _>>()?,
        );
        let phone_w_punct: Vec<String> = sep_phonemes
            .iter()
            .flat_map(|phones| phones.iter().cloned())
            .collect();

        let phone_tone_list = Self::align_tones(phone_w_punct, phone_tone_list_wo_punct)?;

        let mut word2ph = Vec::new();
        for (word, phonemes) in seq_text.iter().zip(sep_phonemes.iter()) {
            word2ph.extend(distribute_phone(
                phonemes.len() as i32,
                word.chars().count() as i32,
            ));
        }

        let mut new_phone_tone_list = vec![("_".to_string(), 0)];
        new_phone_tone_list.extend(phone_tone_list);
        new_phone_tone_list.push(("_".to_string(), 0));

        let mut new_word2ph = vec![1];
        new_word2ph.extend(word2ph);
        new_word2ph.push(1);

        let phones: Vec<String> = new_phone_tone_list.iter().map(|(p, _)| p.clone()).collect();
        let tones: Vec<i32> = new_phone_tone_list.iter().map(|(_, t)| *t).collect();
        Ok((phones, tones, new_word2ph))
    }

    /// Accept only {0} or {0,1} tone sets; {-1,0} folds to {0,1}.
    fn fix_phone_tone(phone_tone_list: Vec<(String, i32)>) -> Result<Vec<(String, i32)>, JaError> {
        let mut tones: std::collections::HashSet<i32> =
            phone_tone_list.iter().map(|(_, tone)| *tone).collect();
        match tones.len() {
            1 => {
                if tones.remove(&0) {
                    Ok(phone_tone_list)
                } else {
                    Err(JaError::ValueError(format!("invalid tone set: {tones:?}")))
                }
            }
            2 => {
                if tones.contains(&-1) && tones.contains(&0) {
                    Ok(phone_tone_list
                        .into_iter()
                        .map(|(letter, tone)| (letter, if tone == -1 { 0 } else { 1 }))
                        .collect())
                } else if tones.contains(&0) && tones.contains(&1) {
                    Ok(phone_tone_list)
                } else {
                    Err(JaError::ValueError(format!("invalid tone set: {tones:?}")))
                }
            }
            _ => Err(JaError::ValueError(format!(
                "too many distinct tones: {tones:?}"
            ))),
        }
    }

    /// Match the tone stream against the punctuated phone stream.
    fn align_tones(
        phone_with_punct: Vec<String>,
        phone_tone_list: Vec<(String, i32)>,
    ) -> Result<Vec<(String, i32)>, JaError> {
        let mut result: Vec<(String, i32)> = Vec::new();
        let mut tone_index = 0;
        for phone in phone_with_punct {
            if tone_index >= phone_tone_list.len() {
                result.push((phone, 0));
            } else if phone == phone_tone_list[tone_index].0 {
                result.push((phone, phone_tone_list[tone_index].1));
                tone_index += 1;
            } else if PUNCTUATIONS.contains(&phone.as_str()) {
                result.push((phone, 0));
            } else {
                return Err(JaError::ValueError(format!(
                    "mismatched phoneme while aligning tones: {phone}"
                )));
            }
        }
        Ok(result)
    }
}

/// NJD node string → (surface, reading): fields 0 and 9.
fn parse_to_string_and_pron(parts: String) -> (String, String) {
    let mut fields = parts.split(',');
    let string = fields.next().unwrap_or_default().to_string();
    let pron = fields.nth(8).unwrap_or_default().to_string();
    (string, pron)
}

/// The padded phone stream (from [`JaProcess::g2p`]) → katakana
/// reading, punctuation kept, boundary pads dropped.
///
/// Consumed by the L3 evaluation to compare a synthesis frontend's
/// reading against annotated gold readings (docs/evaluation.md §3).
pub fn phones_to_kana(phones: &[String]) -> Result<String, JaError> {
    let mut results: Vec<String> = Vec::new();
    let mut current_mora = String::new();
    let end = phones.len().saturating_sub(1);
    for phone in &phones[1..end] {
        if phone == "_" {
            continue;
        }
        if PUNCTUATIONS.contains(&phone.as_str()) {
            results.push(phone.clone());
            continue;
        }
        if CONSONANTS.contains(phone) {
            current_mora = phone.clone();
        } else {
            current_mora.push_str(phone);
            let kana = MORA_PHONEMES_TO_MORA_KATA
                .get(&current_mora)
                .ok_or_else(|| {
                    JaError::ValueError(format!("phoneme pair is not a mora: {current_mora:?}"))
                })?;
            results.push(kana.clone());
            current_mora.clear();
        }
    }
    Ok(results.concat())
}

/// The padded phone stream plus its tones → `(mora, tone)` pairs, with
/// boundary pads dropped and punctuation kept at tone 0.
///
/// This is the accent view of a reading: `tone` is the H/L feature the
/// VITS2 decode receives (1 = high), one entry per mora. Used by the L3
/// accent report, which exists because the phoneme-level gate cannot see
/// a wrong pitch pattern (docs/evaluation.md §3.4).
pub fn kana_tone(phones: &[String], tones: &[i32]) -> Result<Vec<(String, i32)>, JaError> {
    let end = phones.len().min(tones.len()).saturating_sub(1);
    let mut results: Vec<(String, i32)> = Vec::new();
    let mut current_mora = String::new();
    for i in 1..end {
        let phone = &phones[i];
        let tone = tones[i];
        if phone == "_" {
            continue;
        }
        if PUNCTUATIONS.contains(&phone.as_str()) {
            results.push((phone.clone(), 0));
            continue;
        }
        if CONSONANTS.contains(phone) {
            // The mora's tone comes from its vowel; the consonant/vowel
            // pair shares one tone in this scheme.
            current_mora = phone.clone();
        } else {
            current_mora.push_str(phone);
            let kana = MORA_PHONEMES_TO_MORA_KATA
                .get(&current_mora)
                .ok_or_else(|| {
                    JaError::ValueError(format!("phoneme pair is not a mora: {current_mora:?}"))
                })?
                .clone();
            results.push((kana, tone.clamp(0, 1)));
            current_mora.clear();
        }
    }
    Ok(results)
}

/// Render `(mora, tone)` pairs as `(kana, "H/L")` for reports.
pub fn tone_string(pairs: &[(String, i32)]) -> String {
    pairs
        .iter()
        .map(|(_, tone)| if *tone == 0 { 'L' } else { 'H' })
        .collect()
}

/// Fold hiragana to katakana, character-wise (kana fold, euhadra's
/// MatchPolicy table: hiragana and katakana spell the same word).
pub fn hiragana_to_katakana(text: &str) -> String {
    text.chars()
        .map(|c| {
            let code = c as u32;
            if (0x3041..=0x3096).contains(&code) {
                char::from_u32(code + 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g2p_konnichiwa() {
        let frontend = JaFrontend::new().unwrap();
        let process = frontend.process_text("こんにちは").unwrap();
        let (phones, tones, word2ph) = process.g2p().unwrap();

        // こんにちは: k o N n i ch i w a between boundary pads.
        assert_eq!(phones.first(), Some(&"_".to_string()));
        assert_eq!(phones.last(), Some(&"_".to_string()));
        for expected in ["k", "o", "N", "n", "i", "ch", "w", "a"] {
            assert!(
                phones.contains(&expected.to_string()),
                "{expected} missing from {phones:?}"
            );
        }
        // word2ph covers the 5 characters plus the two pads, and its
        // counts sum to the padded phone stream length.
        assert_eq!(word2ph.len(), 5 + 2);
        assert_eq!(word2ph.first(), Some(&1));
        assert_eq!(word2ph.last(), Some(&1));
        assert_eq!(word2ph.iter().sum::<i32>() as usize, phones.len());
        // Accent tones stay in {0, 1} after the fix-up pass.
        assert!(tones.iter().all(|t| *t == 0 || *t == 1));
    }

    #[test]
    fn g2p_sentence_with_punctuation() {
        let frontend = JaFrontend::new().unwrap();
        let process = frontend.process_text("猫が座った。").unwrap();
        let (phones, tones, word2ph) = process.g2p().unwrap();
        // The 。 becomes "." with tone 0 and is padded by silence.
        assert!(phones.contains(&".".to_string()));
        // 猫が座った = neko ga suwatQta.
        for expected in ["n", "e", "k", "o", "g", "a", "s", "u", "w", "q", "t"] {
            assert!(
                phones.contains(&expected.to_string()),
                "{expected} missing from {phones:?}"
            );
        }
        assert_eq!(word2ph.iter().sum::<i32>() as usize, phones.len());
        assert!(tones.iter().all(|t| *t == 0 || *t == 1));
    }

    #[test]
    fn num2word_rewrites_ascii_digits() {
        let frontend = JaFrontend::new().unwrap();
        let rewritten = frontend.num2word("2026年2月").unwrap();
        assert!(
            !rewritten.chars().any(|c| c.is_ascii_digit()),
            "digits must be rewritten: {rewritten}"
        );
    }

    #[test]
    fn kata_to_phoneme_list_handles_long_vowel_mark() {
        // ラーメン → ra a me N (ー repeats the previous vowel).
        let phonemes = kata_to_phoneme_list("ラーメン".to_string()).unwrap();
        assert_eq!(phonemes, vec!["r", "a", "a", "m", "e", "N"]);
    }

    #[test]
    fn kata_to_phoneme_list_passes_punctuation_through() {
        let phonemes = kata_to_phoneme_list(".,!".to_string()).unwrap();
        assert_eq!(phonemes, vec![".", ",", "!"]);
    }

    #[test]
    fn kata_to_phoneme_list_rejects_non_katakana() {
        let err = kata_to_phoneme_list("abcあ".to_string()).unwrap_err();
        assert!(matches!(err, JaError::ValueError(_)));
    }
}

#[cfg(test)]
mod phones_to_kana_tests {
    use super::*;

    #[test]
    fn konnichiwa_phones_to_kana() {
        // from the g2p test's observed stream
        let kana = phones_to_kana(&[
            "_".to_string(),
            "k".into(),
            "o".into(),
            "N".into(),
            "n".into(),
            "i".into(),
            "ch".into(),
            "i".into(),
            "w".into(),
            "a".into(),
            "_".to_string(),
        ])
        .unwrap();
        // Pronunciation-based: the particle は is pronounced わ.
        assert_eq!(kana, "コンニチワ");
    }

    #[test]
    fn punctuation_is_kept_pads_dropped() {
        let kana = phones_to_kana(&[
            "_".into(),
            "n".into(),
            "e".into(),
            "k".into(),
            "o".into(),
            ".".into(),
            "g".into(),
            "a".into(),
            "_".into(),
        ])
        .unwrap();
        assert_eq!(kana, "ネコ.ガ");
    }

    #[test]
    fn hiragana_folds_to_katakana() {
        assert_eq!(hiragana_to_katakana("むすくるす"), "ムスクルス");
        assert_eq!(hiragana_to_katakana("あーい"), "アーイ");
        assert_eq!(hiragana_to_katakana("ABC"), "ABC");
    }
}
