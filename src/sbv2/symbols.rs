//! Phoneme symbol tables for the SBV2 (JP-Extra) frontend.
//!
//! DATA PROVENANCE: the symbol inventory and its ordering are copied
//! from sbv2_core (`crates/sbv2_core/src/norm.rs`, MIT) because the
//! VITS2 decode model was trained against exactly this symbol→id
//! mapping — the model's output is undefined for any other ordering.
//! This is model-internal data, not algorithmic code from the AGPL
//! lineage; see docs/model-licenses.md.

/// Punctuation symbols that stay in the phoneme stream with tone 0.
pub const PUNCTUATIONS: [&str; 7] = ["!", "?", "…", ",", ".", "'", "-"];

const PAD: &str = "_";

/// Japanese phoneme inventory (JP-Extra).
pub const JP_SYMBOLS: [&str; 42] = [
    "N", "a", "a:", "b", "by", "ch", "d", "dy", "e", "e:", "f", "g", "gy", "h", "hy", "i", "i:",
    "j", "k", "ky", "m", "my", "n", "ny", "o", "o:", "p", "py", "q", "r", "ry", "s", "sh", "t",
    "ts", "ty", "u", "u:", "w", "y", "z", "zy",
];

/// Chinese phoneme inventory (present in the shared table so the id
/// ordering matches the multi-lingual SBV2 lineage, even though the
/// JP-Extra models never emit these).
const ZH_SYMBOLS: [&str; 65] = [
    "E", "En", "a", "ai", "an", "ang", "ao", "b", "c", "ch", "d", "e", "ei", "en", "eng", "er",
    "f", "g", "h", "i", "i0", "ia", "ian", "iang", "iao", "ie", "in", "ing", "iong", "ir", "iu",
    "j", "k", "l", "m", "n", "o", "ong", "ou", "p", "q", "r", "s", "sh", "t", "u", "ua", "uai",
    "uan", "uang", "ui", "un", "uo", "v", "van", "ve", "vn", "w", "x", "y", "z", "zh", "AA", "EE",
    "OO",
];

/// English phoneme inventory (same reason as `ZH_SYMBOLS`).
const EN_SYMBOLS: [&str; 39] = [
    "aa", "ae", "ah", "ao", "aw", "ay", "b", "ch", "d", "dh", "eh", "er", "ey", "f", "g", "hh",
    "ih", "iy", "jh", "k", "l", "m", "n", "ng", "ow", "oy", "p", "r", "s", "sh", "t", "th", "uh",
    "uw", "V", "w", "y", "z", "zh",
];

/// Normal symbols: deduplicated, sorted union of the three inventories.
fn normal_symbols() -> Vec<&'static str> {
    let mut set: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for s in ZH_SYMBOLS
        .iter()
        .chain(JP_SYMBOLS.iter())
        .chain(EN_SYMBOLS.iter())
    {
        set.insert(s);
    }
    set.into_iter().collect()
}

/// The full symbol table the model was trained against:
/// `PAD`, then normal symbols (sorted), then punctuation symbols.
pub fn symbols() -> Vec<&'static str> {
    let mut symbols = vec![PAD];
    symbols.extend(normal_symbols());
    symbols.extend(PUNCTUATIONS.iter().copied());
    symbols.extend(["SP", "UNK"]);
    symbols
}

/// Look up a symbol's model id.
pub fn symbol_to_id(symbol: &str) -> Option<i64> {
    symbols()
        .iter()
        .position(|s| *s == symbol)
        .map(|i| i as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pad_is_id_zero() {
        assert_eq!(symbol_to_id("_"), Some(0));
    }

    #[test]
    fn japanese_phonemes_are_in_the_table() {
        for s in ["k", "o", "N", "n", "i", "ch", "w", "a", "a:", "q"] {
            assert!(symbol_to_id(s).is_some(), "{s} missing");
        }
    }

    #[test]
    fn punctuation_symbols_follow_normal_symbols() {
        let table = symbols();
        let punctuation_first = symbol_to_id("!").unwrap();
        // Every normal (non-punctuation) symbol must sit before "!":
        // JP inventory must therefore be below the punctuation block.
        for s in JP_SYMBOLS {
            assert!(symbol_to_id(s).unwrap() < punctuation_first, "{s}");
        }
        assert_eq!(symbol_to_id("SP"), Some(table.len() as i64 - 2));
        assert_eq!(symbol_to_id("UNK"), Some(table.len() as i64 - 1));
    }

    #[test]
    fn unknown_symbols_return_none() {
        assert_eq!(symbol_to_id("X"), None);
        assert_eq!(symbol_to_id(""), None);
    }
}
