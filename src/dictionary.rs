//! User-owned term dictionary — the mirror of euhadra's
//! `TermDictionary`.
//!
//! Some words come out wrong no matter how good the synthesis model
//! is: a coined spelling nobody trained on (say "musculus" and a ja
//! frontend spells the letters out — docs/benchmarks/listening-log.md).
//! No acoustic model will produce a reading nobody trained on; only
//! the user can say what they meant. musculus owns the behaviour, not
//! the dictionary: entries arrive from the consuming application, and
//! the rewrite happens as a pipeline stage so its ordering is picked
//! by the pipeline, not by hand (it runs before the frontend, which
//! then sees already-read forms).
//!
//! Matching is one pass, longest alias first, and replaced text is
//! never rescanned. A match always replaces — there is no confidence
//! score that might quietly decline — and every substitution is
//! reported as a `Correction` with a codepoint span so callers can
//! show it or undo it.

use serde::Deserialize;

use crate::traits::{NormalizerError, TextProcessor};
use crate::types::{Correction, NormalizedText};

/// One entry: what a phrase should be read as (`term`), and the
/// written forms that trigger it (`aliases`).
#[derive(Debug, Clone, Deserialize)]
pub struct TermEntry {
    /// The replacement — already in speakable form (kana for ja).
    pub term: String,
    /// The written forms that should be rewritten to `term`.
    pub aliases: Vec<String>,
}

/// Matching policy. The Japanese defaults mirror euhadra's
/// `MatchPolicy::for_language(Japanese)` table: substring scope,
/// case-insensitive, fullwidth-folded, kana-folded. The folds follow
/// the information-preserving rule: hiragana and katakana spell the
/// same word and fold together; nothing else is folded (a fold that
/// loses information is not offered — 「タイプライタ」 and
/// 「タイプライター」 can be different words).
#[derive(Debug, Clone, Copy, Default)]
pub struct MatchPolicy;

impl MatchPolicy {
    /// The Japanese policy (the only one in 0.x).
    pub fn for_japanese() -> Self {
        Self
    }

    /// Fold a string for matching. Length-preserving by design, so a
    /// fold of the haystack at position `i` compares against a fold of
    /// the alias with the same char count.
    fn fold(&self, s: &str) -> String {
        s.chars()
            .map(|c| {
                let code = c as u32;
                let code = match code {
                    // Fullwidth ASCII → ASCII (U+FF01..U+FF5E).
                    0xFF01..=0xFF5E => code - 0xFEE0,
                    // Fullwidth ideographic space → space.
                    0x3000 => 0x20,
                    // Katakana → hiragana (kana fold).
                    0x30A1..=0x30F6 => code - 0x60,
                    _ => code,
                };
                let folded = char::from_u32(code).unwrap_or(c);
                folded.to_ascii_lowercase()
            })
            .collect()
    }
}

/// A validated, ready-to-apply dictionary.
#[derive(Debug, Clone)]
pub struct TermDictionary {
    /// `(folded alias, entry index, term)`, longest folded alias first.
    aliases: Vec<(String, usize, String)>,
    policy: MatchPolicy,
}

impl TermDictionary {
    /// Validate and build. Reports **every** problem at once, keyed by
    /// entry and alias index, so a settings UI can highlight all
    /// offending rows instead of one error per save.
    pub fn new(
        entries: impl IntoIterator<Item = TermEntry>,
        policy: MatchPolicy,
    ) -> Result<Self, NormalizerError> {
        let entries: Vec<TermEntry> = entries.into_iter().collect();
        let mut problems: Vec<String> = Vec::new();
        let mut aliases: Vec<(String, usize, String)> = Vec::new();
        let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for (entry_index, entry) in entries.iter().enumerate() {
            if entry.term.is_empty() {
                problems.push(format!("entry[{entry_index}]: term is empty"));
            }
            if entry.aliases.is_empty() {
                problems.push(format!("entry[{entry_index}]: no aliases"));
            }
            for (alias_index, alias) in entry.aliases.iter().enumerate() {
                if alias.is_empty() {
                    problems.push(format!(
                        "entry[{entry_index}] alias[{alias_index}]: alias is empty"
                    ));
                    continue;
                }
                if alias == &entry.term {
                    problems.push(format!(
                        "entry[{entry_index}] alias[{alias_index}] ({alias}): alias equals term — a no-op rewrite"
                    ));
                    continue;
                }
                let folded = policy.fold(alias);
                if let Some(&previous) = seen.get(&folded) {
                    problems.push(format!(
                        "entry[{entry_index}] alias[{alias_index}] ({alias}): duplicate alias — already provided by entry[{previous}]"
                    ));
                    continue;
                }
                seen.insert(folded.clone(), entry_index);
                aliases.push((folded, entry_index, entry.term.clone()));
            }
        }

        if !problems.is_empty() {
            return Err(NormalizerError::Config(format!(
                "dictionary has {} problem(s): {}",
                problems.len(),
                problems.join("; ")
            )));
        }
        aliases.sort_by_key(|(folded, _, _)| std::cmp::Reverse(folded.chars().count()));
        Ok(Self { aliases, policy })
    }

    /// Every alias that would trigger a rewrite, folded, longest first.
    pub fn alias_count(&self) -> usize {
        self.aliases.len()
    }
}

impl TextProcessor for TermDictionary {
    fn process(&self, input: &str) -> Result<NormalizedText, NormalizerError> {
        let folded_input = self.policy.fold(input);
        let mut output = String::with_capacity(input.len());
        let mut corrections: Vec<Correction> = Vec::new();
        let mut char_index = 0usize;
        let mut input_chars = input.chars().peekable();

        while let Some(&ch) = input_chars.peek() {
            let mut matched = false;
            for (alias_folded, _entry_index, term) in &self.aliases {
                let alias_chars = alias_folded.chars().count();
                if alias_chars == 0 {
                    continue;
                }
                let window: String = folded_input
                    .chars()
                    .skip(char_index)
                    .take(alias_chars)
                    .collect();
                if window == *alias_folded {
                    let from: String = input_chars.clone().take(alias_chars).collect();
                    let span_start = output.chars().count();
                    output.push_str(term);
                    corrections.push(Correction {
                        span: span_start..output.chars().count(),
                        from,
                        to: term.clone(),
                    });
                    for _ in 0..alias_chars {
                        input_chars.next();
                    }
                    char_index += alias_chars;
                    matched = true;
                    break;
                }
            }
            if !matched {
                output.push(ch);
                input_chars.next();
                char_index += 1;
            }
        }

        Ok(NormalizedText {
            text: output,
            corrections,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dictionary(entries: Vec<TermEntry>) -> TermDictionary {
        TermDictionary::new(entries, MatchPolicy::for_japanese()).unwrap()
    }

    fn entry(term: &str, aliases: &[&str]) -> TermEntry {
        TermEntry {
            term: term.to_string(),
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn rewrites_the_coined_word() {
        let dict = dictionary(vec![entry("ムスクルス", &["musculus"])]);
        let out = dict.process("musculus の世界へようこそ").unwrap();
        assert_eq!(out.text, "ムスクルス の世界へようこそ");
        assert_eq!(out.corrections.len(), 1);
        assert_eq!(out.corrections[0].from, "musculus");
        assert_eq!(out.corrections[0].to, "ムスクルス");
        // span indexes codepoints into the rewritten text
        let chars: Vec<char> = out.text.chars().collect();
        let sliced: String = chars[out.corrections[0].span.clone()].iter().collect();
        assert_eq!(sliced, "ムスクルス");
    }

    #[test]
    fn case_and_fullwidth_and_kana_folds() {
        let dict = dictionary(vec![entry("ムスクルス", &["musculus"])]);
        for input in ["MUSCULUS", "Musculus", "ＭＵＳＣＵＬＵＳ"] {
            let out = dict.process(input).unwrap();
            assert_eq!(out.text, "ムスクルス", "{input}");
        }
        // kana fold: a katakana alias matches hiragana text and back
        let kana_dict = dictionary(vec![entry("マウス", &["みうす"])]);
        let out = kana_dict.process("ミウスです").unwrap();
        assert_eq!(out.text, "マウスです");
    }

    #[test]
    fn longest_alias_wins() {
        let dict = dictionary(vec![entry("エー", &["A"]), entry("エービー", &["AB"])]);
        let out = dict.process("AB").unwrap();
        assert_eq!(out.text, "エービー");
        assert_eq!(out.corrections.len(), 1);
    }

    #[test]
    fn replaced_text_is_never_rescanned() {
        // term contains the alias as a substring; one pass must not
        // feed the replacement back through the dictionary.
        let dict = dictionary(vec![entry("BA", &["A"])]);
        let out = dict.process("A A").unwrap();
        assert_eq!(out.text, "BA BA");
        assert_eq!(out.corrections.len(), 2);
    }

    #[test]
    fn empty_input_passes_through() {
        let dict = dictionary(vec![entry("エー", &["A"])]);
        let out = dict.process("").unwrap();
        assert_eq!(out.text, "");
        assert!(out.corrections.is_empty());
    }

    #[test]
    fn validation_reports_every_problem_at_once() {
        let err = TermDictionary::new(
            vec![
                entry("", &["x"]),        // empty term
                entry("t1", &[]),         // no aliases
                entry("t2", &["", "t2"]), // empty alias + alias equals term
                entry("t3", &["A"]),      // duplicate of t4
                entry("t4", &["a"]),      // duplicate of t3
            ],
            MatchPolicy::for_japanese(),
        )
        .unwrap_err();
        let message = err.to_string();
        assert!(matches!(err, NormalizerError::Config(_)));
        for expected in [
            "entry[0]: term is empty",
            "entry[1]: no aliases",
            "entry[2] alias[0]: alias is empty",
            "entry[2] alias[1] (t2): alias equals term",
            "entry[4] alias[0] (a): duplicate alias — already provided by entry[3]",
        ] {
            assert!(
                message.contains(expected),
                "missing: {expected}\nin: {message}"
            );
        }
    }

    #[test]
    fn correction_spans_stay_consistent_after_earlier_rewrites() {
        let dict = dictionary(vec![entry("エー", &["A"]), entry("ビー", &["B"])]);
        let out = dict.process("AとB").unwrap();
        assert_eq!(out.text, "エーとビー");
        assert_eq!(out.corrections.len(), 2);
        let chars: Vec<char> = out.text.chars().collect();
        let first: String = chars[out.corrections[0].span.clone()].iter().collect();
        let second: String = chars[out.corrections[1].span.clone()].iter().collect();
        assert_eq!(first, "エー");
        assert_eq!(second, "ビー");
    }
}
