//! Mora tables for the Japanese frontend.
//!
//! DATA PROVENANCE: `mora_list.json` is copied from sbv2_core
//! (`crates/sbv2_core/src/mora_list.json`, MIT) — a mora inventory the
//! models were trained with, functionally the same data VOICEVOX's
//! engine carries. Data tables, not algorithmic code; see
//! docs/model-licenses.md.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Mora {
    pub mora: String,
    pub consonant: Option<String>,
    pub vowel: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MoraFile {
    minimum: Vec<Mora>,
    additional: Vec<Mora>,
}

static MORA_LIST: LazyLock<MoraFile> = LazyLock::new(|| {
    serde_json::from_str(include_str!("mora_list.json")).expect("mora_list.json is valid")
});

/// Mora phoneme sequence (e.g. `ko`) → katakana mora (e.g. `コ`),
/// over the minimum list.
pub static MORA_PHONEMES_TO_MORA_KATA: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    MORA_LIST
        .minimum
        .iter()
        .map(|m| {
            (
                format!("{}{}", m.consonant.clone().unwrap_or_default(), m.vowel),
                m.mora.clone(),
            )
        })
        .collect()
});

/// Katakana mora (e.g. `コ`) → (optional consonant, vowel), over the
/// minimum + additional lists.
pub static MORA_KATA_TO_MORA_PHONEMES: LazyLock<HashMap<String, (Option<String>, String)>> =
    LazyLock::new(|| {
        MORA_LIST
            .minimum
            .iter()
            .chain(MORA_LIST.additional.iter())
            .map(|m| (m.mora.clone(), (m.consonant.clone(), m.vowel.clone())))
            .collect()
    });

/// Every consonant in the inventory.
pub static CONSONANTS: LazyLock<Vec<String>> = LazyLock::new(|| {
    MORA_KATA_TO_MORA_PHONEMES
        .values()
        .filter_map(|(consonant, _)| consonant.clone())
        .collect()
});

pub const VOWELS: [&str; 6] = ["a", "i", "u", "e", "o", "N"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_moras_round_trip() {
        // コ = k + o, ア = a (the table's keys are katakana)
        assert_eq!(
            MORA_KATA_TO_MORA_PHONEMES.get("コ"),
            Some(&(Some("k".to_string()), "o".to_string()))
        );
        assert_eq!(
            MORA_KATA_TO_MORA_PHONEMES.get("ア"),
            Some(&(None, "a".to_string()))
        );
        assert_eq!(
            MORA_PHONEMES_TO_MORA_KATA.get("ko"),
            Some(&"コ".to_string())
        );
    }

    #[test]
    fn small_tsu_maps_to_the_glottal_stop() {
        // ッ (促音) is in the minimum list and resolves to `q`.
        assert_eq!(
            MORA_KATA_TO_MORA_PHONEMES.get("ッ"),
            Some(&(None, "q".to_string()))
        );
        assert_eq!(MORA_PHONEMES_TO_MORA_KATA.get("q"), Some(&"ッ".to_string()));
    }

    #[test]
    fn consonant_inventory_contains_expected_phonemes() {
        for c in ["k", "ch", "ts", "gy", "sh"] {
            assert!(CONSONANTS.contains(&c.to_string()), "{c} missing");
        }
        assert!(!CONSONANTS.contains(&"a".to_string()));
    }
}
