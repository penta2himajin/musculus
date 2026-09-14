//! User-owned accent overrides — the prosody counterpart of the term
//! dictionary.
//!
//! The frontend's accent estimation is documented to miss context
//! dependent cases (numeral compounds: `1,200` comes out `HLLHH` where a
//! standard reading is `LLLHH`; see docs/benchmarks/accent/ja-report.md).
//! The literature also documents genuine variation between speakers, so
//! there is no single "correct" pattern to hard-code: the table lets the
//! consumer own the choice, exactly as it owns vocabulary (ADR-0006).
//!
//! An entry names a katakana phrase and the H/L value of each of its
//! morae. Matching is on the mora stream the decode actually receives,
//! longest entry first, so a long phrase wins over a shorter one inside
//! it (an entry for 「センニヒャク」 must not shadow one for
//! 「ニセンニヒャクエン」).

use serde::Deserialize;

use crate::sbv2::ja::{self, MoraSpan};

/// One override: a katakana phrase and one tone character (`H`/`L`) per
/// mora.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AccentEntry {
    /// Katakana mora sequence, e.g. `センニヒャク`.
    pub kana: String,
    /// One `H` or `L` per mora, e.g. `LLLHH`.
    pub tones: String,
}

/// A validated set of accent overrides.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccentTable {
    /// (mora kana, tone per mora), longest kana first.
    entries: Vec<(Vec<String>, Vec<i32>)>,
}

impl AccentTable {
    /// Validate entries and sort them longest-first.
    pub fn new(entries: Vec<AccentEntry>) -> Result<Self, String> {
        let mut parsed: Vec<(Vec<String>, Vec<i32>)> = Vec::with_capacity(entries.len());
        for entry in entries {
            let moras = ja::split_kana_morae(&entry.kana);
            let tones: Vec<i32> = entry
                .tones
                .chars()
                .map(|c| match c {
                    'H' | 'h' => Ok(1),
                    'L' | 'l' => Ok(0),
                    other => Err(format!(
                        "accent entry {:?}: expected H or L, found {other:?}",
                        entry.kana
                    )),
                })
                .collect::<Result<_, _>>()?;
            if moras.is_empty() {
                return Err(format!("accent entry {:?}: no morae", entry.kana));
            }
            if tones.len() != moras.len() {
                return Err(format!(
                    "accent entry {:?}: {} tone characters for {} morae",
                    entry.kana,
                    tones.len(),
                    moras.len()
                ));
            }
            parsed.push((moras, tones));
        }
        parsed.sort_by_key(|entry| std::cmp::Reverse(entry.0.len()));
        Ok(Self { entries: parsed })
    }

    /// Parse a JSON array of entries.
    pub fn from_json_slice(bytes: &[u8]) -> Result<Self, String> {
        let entries: Vec<AccentEntry> =
            serde_json::from_slice(bytes).map_err(|e| format!("accent table: {e}"))?;
        Self::new(entries)
    }

    /// Read a JSON table from disk.
    pub fn from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Self::from_json_slice(&bytes)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Rewrite the tones of every matching mora run.
    ///
    /// Returns a fresh tone array of the same length as `tones`; phones
    /// outside a match keep their value. Matching is longest-first, so a
    /// longer phrase shadows a shorter one contained in it.
    pub fn apply(&self, phones: &[String], tones: &[i32]) -> Vec<i32> {
        let mut out = tones.to_vec();
        if self.entries.is_empty() {
            return out;
        }
        let Ok(spans) = ja::kana_tone_spans(phones, tones) else {
            return out;
        };
        // Punctuation must not be covered by a match: build the list of
        // matchable positions (index into `spans`).
        let matchable: Vec<usize> = spans
            .iter()
            .enumerate()
            .filter(|(_, span)| !ja::is_punctuation(&span.kana))
            .map(|(i, _)| i)
            .collect();
        let mut cursor = 0usize;
        while cursor < matchable.len() {
            let mut matched = false;
            for (kana, entry_tones) in &self.entries {
                let len = kana.len();
                if cursor + len > matchable.len() {
                    continue;
                }
                let candidate: Vec<&str> = matchable[cursor..cursor + len]
                    .iter()
                    .map(|&i| spans[i].kana.as_str())
                    .collect();
                if candidate
                    .iter()
                    .zip(kana.iter())
                    .all(|(a, b)| *a == b.as_str())
                {
                    for (offset, tone) in entry_tones.iter().enumerate() {
                        write_span_tone(&spans[matchable[cursor + offset]], *tone, &mut out);
                    }
                    cursor += len;
                    matched = true;
                    break;
                }
            }
            if !matched {
                cursor += 1;
            }
        }
        out
    }
}

/// Write one mora's tone across all the phones it spans.
fn write_span_tone(span: &MoraSpan, tone: i32, out: &mut [i32]) {
    for index in span.start..span.end.min(out.len()) {
        out[index] = tone;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The padded phone stream for 「センニヒャク」 as the frontend emits
    /// it (pads at both ends, one entry per phone).
    fn sen_ni_hyaku() -> (Vec<String>, Vec<i32>) {
        let phones: Vec<String> = ["_", "s", "e", "N", "n", "i", "hy", "a", "k", "u", "_"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let tones = vec![0, 1, 1, 0, 0, 0, 1, 1, 1, 1, 0];
        (phones, tones)
    }

    #[test]
    fn entry_mora_count_must_match_the_kana() {
        let ok = AccentTable::new(vec![AccentEntry {
            kana: "センニヒャク".into(),
            tones: "LLLHH".into(),
        }]);
        assert!(ok.is_ok());
        let bad = AccentTable::new(vec![AccentEntry {
            kana: "センニヒャク".into(),
            tones: "LLL".into(),
        }]);
        assert!(bad.is_err());
    }

    #[test]
    fn only_h_and_l_are_accepted() {
        let bad = AccentTable::new(vec![AccentEntry {
            kana: "セン".into(),
            tones: "HX".into(),
        }]);
        assert!(bad.unwrap_err().contains("expected H or L"));
    }

    #[test]
    fn apply_rewrites_the_matching_morae_only() {
        let table = AccentTable::new(vec![AccentEntry {
            kana: "センニヒャク".into(),
            tones: "LLLHH".into(),
        }])
        .unwrap();
        let (phones, tones) = sen_ni_hyaku();
        let out = table.apply(&phones, &tones);
        // Every phone of every mora carries its mora's tone: セ=L, ン=L,
        // ニ=L, ヒャ=H, ク=H.
        assert_eq!(
            ja::kana_tone(&phones, &out).unwrap(),
            vec![
                ("セ".to_string(), 0),
                ("ン".to_string(), 0),
                ("ニ".to_string(), 0),
                ("ヒャ".to_string(), 1),
                ("ク".to_string(), 1),
            ]
        );
    }

    #[test]
    fn longest_entry_wins_inside_a_longer_phrase() {
        // The 5-mora entry must not apply inside the 8-mora phrase.
        let table = AccentTable::new(vec![
            AccentEntry {
                kana: "センニヒャク".into(),
                tones: "LLLHH".into(),
            },
            AccentEntry {
                kana: "ニセンニヒャクエン".into(),
                tones: "HHLLHHHL".into(),
            },
        ])
        .unwrap();
        let phones: Vec<String> = [
            "_", "n", "i", "s", "e", "N", "n", "i", "hy", "a", "k", "u", "e", "N", "_",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let tones = vec![0; phones.len()];
        let out = table.apply(&phones, &tones);
        let moras: Vec<(String, i32)> = ja::kana_tone(&phones, &out).unwrap();
        let pattern: String = moras
            .iter()
            .map(|(_, t)| if *t == 0 { 'L' } else { 'H' })
            .collect();
        assert_eq!(pattern, "HHLLHHHL");
    }

    #[test]
    fn no_entries_is_a_no_op() {
        let table = AccentTable::default();
        let (phones, tones) = sen_ni_hyaku();
        assert_eq!(table.apply(&phones, &tones), tones);
        assert!(table.is_empty());
    }

    #[test]
    fn json_round_trip() {
        let json = r#"[{"kana": "セン", "tones": "HL"}]"#;
        let table = AccentTable::from_json_slice(json.as_bytes()).unwrap();
        assert_eq!(table.len(), 1);
    }
}
