//! Text normalization for the Japanese TTS frontend.
//!
//! Our own implementation (docs/model-licenses.md): the behaviour it
//! reproduces — punctuation unification to a small spoken-form set,
//! dash-variant folding, and stripping everything that is neither
//! kana/kanji/latin nor punctuation — is what the models were trained
//! on, so the *mappings* follow the reference; the code does not.

use std::sync::LazyLock;

use crate::sbv2::symbols::PUNCTUATIONS;

static REPLACE_MAP: LazyLock<&[(&str, &str)]> = LazyLock::new(|| {
    &[
        // Japanese punctuation → ASCII spoken-form punctuation.
        ("：", ","),
        ("；", ","),
        ("，", ","),
        ("。", "."),
        ("！", "!"),
        ("？", "?"),
        ("．", "."),
        ("…", "..."),
        ("···", "..."),
        ("・・・", "..."),
        ("·", ","),
        ("・", ","),
        ("、", ","),
        ("$", "."),
        // Quotation marks and brackets → apostrophes (brief pauses).
        ("“", "'"),
        ("”", "'"),
        ("\"", "'"),
        ("‘", "'"),
        ("’", "'"),
        ("（", "'"),
        ("）", "'"),
        ("(", "'"),
        (")", "'"),
        ("《", "'"),
        ("》", "'"),
        ("【", "'"),
        ("】", "'"),
        ("[", "'"),
        ("]", "'"),
        ("「", "'"),
        ("」", "'"),
        // Dash/dash-variant folding after NFKC-style reasoning: every
        // horizontal-bar codepoint the reference handles folds to U+002D.
        ("\u{02d7}", "\u{002d}"), // ˗ modifier minus
        ("\u{2010}", "\u{002d}"), // ‐ hyphen
        ("\u{2012}", "\u{002d}"), // ‒ figure dash
        ("\u{2013}", "\u{002d}"), // – en dash
        ("\u{2014}", "\u{002d}"), // — em dash
        ("\u{2015}", "\u{002d}"), // ― horizontal bar
        ("\u{2043}", "\u{002d}"), // ⁃ hyphen bullet
        ("\u{2212}", "\u{002d}"), // − minus sign
        ("\u{23af}", "\u{002d}"), // ⎯ line extension
        ("\u{23e4}", "\u{002d}"), // ⏤ straightness
        ("\u{2500}", "\u{002d}"), // ─ box drawing light
        ("\u{2501}", "\u{002d}"), // ━ box drawing heavy
        ("\u{2e3a}", "\u{002d}"), // ⸺ two-em dash
        ("\u{2e3b}", "\u{002d}"), // ⸻ three-em dash
    ]
});

/// Everything stripped before synthesis: anything outside kana, kanji,
/// latin (ASCII + fullwidth), Greek (BERT-side legacy) and punctuation.
static PUNCTUATION_CLEANUP_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    let pattern = r"[^\u{3040}-\u{309F}\u{30A0}-\u{30FF}\u{4E00}-\u{9FFF}\u{3400}-\u{4DBF}\u{3005}"
        .to_owned()
        + r"\u{0041}-\u{005A}\u{0061}-\u{007A}"
        + r"\u{FF21}-\u{FF3A}\u{FF41}-\u{FF5A}"
        + r"\u{0370}-\u{03FF}\u{1F00}-\u{1FFF}"
        + &PUNCTUATIONS.join("")
        + "]+";
    regex::Regex::new(&pattern).expect("cleanup pattern is valid")
});

/// Normalize one line of Japanese text for synthesis.
///
/// Wave characters (~) become the long-vowel mark ー, punctuation is
/// unified, and every character outside the trained inventory is
/// dropped. Newlines are handled by the caller (segment splitting).
pub fn normalize_text(text: &str) -> String {
    let text = text.replace(['~', '～', '〜'], "ー");
    replace_punctuation(&text)
}

/// Apply the punctuation unification map and strip untrained characters.
pub fn replace_punctuation(text: &str) -> String {
    let mut replaced = text.to_string();
    for (from, to) in REPLACE_MAP.iter() {
        replaced = replaced.replace(from, to);
    }
    PUNCTUATION_CLEANUP_PATTERN
        .replace_all(&replaced, "")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_punctuation_becomes_spoken_form() {
        assert_eq!(normalize_text("こんにちは。"), "こんにちは.");
        assert_eq!(normalize_text("あ、いーう"), "あ,いーう");
        assert_eq!(normalize_text("そうだね？"), "そうだね?");
    }

    #[test]
    fn wave_characters_become_long_vowel_mark() {
        assert_eq!(normalize_text("ラーメン〜"), "ラーメンー");
        assert_eq!(normalize_text("ぐあ~"), "ぐあー");
    }

    #[test]
    fn brackets_and_quotes_become_apostrophes() {
        assert_eq!(normalize_text("「はい」"), "'はい'");
        assert_eq!(normalize_text("(メモ)"), "'メモ'");
    }

    #[test]
    fn untrained_characters_are_stripped() {
        // Emoji and digits are not in the trained inventory; they must
        // disappear rather than corrupt the phoneme stream.
        assert_eq!(normalize_text("こんにちは🐱"), "こんにちは");
        assert_eq!(normalize_text("3.14"), ".");
        // Whitespace is stripped too.
        assert_eq!(normalize_text("あ い"), "あい");
    }

    #[test]
    fn dash_variants_fold_to_ascii_hyphen() {
        // U+2010 hyphen and U+2014 em dash fold to U+002D. (U+2011,
        // the non-breaking hyphen, is NOT in the trained map and is
        // stripped by the cleanup pass — reference-faithful behaviour.)
        // Digits are stripped here too; they never reach this pass in
        // the real pipeline because num2word rewrites them first.
        assert_eq!(normalize_text("Wi\u{2010}Fi"), "Wi-Fi");
        assert_eq!(normalize_text("あ\u{2014}い"), "あ-い");
        assert_eq!(normalize_text("1\u{2014}2"), "-");
    }
}
