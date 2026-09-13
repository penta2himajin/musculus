//! WER/CER metrics for the evaluation harness.
//!
//! The strict flavour is ported from euhadra's `src/eval/metrics.rs`
//! (MIT, sibling project) with its two-flavour rationale intact: the
//! round-trip runners need surface-form tolerance the ablation
//! harnesses must not have. musculus keeps the two it uses and stays
//! lean; the full euhadra table lives upstream.

/// Character Error Rate, strict: whitespace stripped, **no** other
/// normalisation (euhadra port). `f64::NAN` when the reference is
/// empty.
pub fn cer(reference: &str, hypothesis: &str) -> f64 {
    let r: Vec<char> = reference.chars().filter(|c| !c.is_whitespace()).collect();
    let h: Vec<char> = hypothesis.chars().filter(|c| !c.is_whitespace()).collect();
    if r.is_empty() {
        return f64::NAN;
    }
    levenshtein(&r, &h) as f64 / r.len() as f64
}

/// CER over case-folded, punctuation-stripped text — the round-trip
/// flavour. A synthesis input and its ASR transcription legitimately
/// differ in punctuation and case; those are orthography, not
/// intelligibility, so the round-trip metric strips them on both sides
/// before comparing. (Kanji/kana homophone spelling differences are
/// NOT folded here — see the reading-level metric for a
/// homophone-fair view.)
pub fn cer_normalized(reference: &str, hypothesis: &str) -> f64 {
    let fold = |s: &str| -> Vec<char> {
        s.chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_ascii_lowercase())
            .filter(|c| !is_punct_like(*c))
            .collect()
    };
    let r = fold(reference);
    if r.is_empty() {
        return f64::NAN;
    }
    let h = fold(hypothesis);
    levenshtein(&r, &h) as f64 / r.len() as f64
}

/// Characters treated as orthography noise by [`cer_normalized`].
fn is_punct_like(c: char) -> bool {
    matches!(
        c,
        '.' | ','
            | '!'
            | '?'
            | '。'
            | '、'
            | '！'
            | '？'
            | '，'
            | '：'
            | '；'
            | '…'
            | 'ー'
            | '-'
            | '・'
            | '\''
            | '"'
            | '「'
            | '」'
            | '('
            | ')'
            | '（'
            | '）'
    )
}

/// Levenshtein edit distance over sequences — used for characters and
/// for phoneme sequences alike.
pub fn levenshtein<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0; b.len() + 1];
    for (i, a_item) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, b_item) in b.iter().enumerate() {
            let cost = usize::from(a_item != b_item);
            current[j + 1] = (previous[j] + cost)
                .min(current[j] + 1)
                .min(previous[j + 1] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_cer_counts_one_deletion_in_four() {
        assert_eq!(
            cer("きょうは", "きょうわ"), /* う vs わ substitution */
            0.25
        );
    }

    #[test]
    fn strict_cer_is_undefined_on_empty_reference() {
        assert!(cer("", "なにか").is_nan());
    }

    #[test]
    fn normalized_cer_folds_punctuation_and_case() {
        // Orthography noise only: punctuation, case.
        assert_eq!(cer_normalized("Hello, World!", "hello world"), 0.0);
        assert_eq!(cer_normalized("こんにちは。", "こんにちは"), 0.0);
        // Real differences still count.
        assert_eq!(cer_normalized("あいう", "あえう"), 1.0 / 3.0);
    }

    #[test]
    fn normalized_cer_is_undefined_on_empty_reference() {
        assert!(cer_normalized("。", "なにか").is_nan());
    }

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein(&[1, 2, 3], &[1, 2, 3]), 0);
        assert_eq!(levenshtein(&[1, 2, 3], &[1, 3]), 1);
        assert_eq!(levenshtein::<char>(&[], &['a']), 1);
        assert_eq!(
            levenshtein(
                &["s".to_string(), "a".to_string()],
                &["s".to_string(), "u".to_string()]
            ),
            1
        );
    }
}
