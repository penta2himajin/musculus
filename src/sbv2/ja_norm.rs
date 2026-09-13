//! The built-in Japanese normalization stage — musculus-owned rules
//! that rewrite written text into forms the frontend reads correctly.
//!
//! This is the Tier-1 mirror of euhadra's text filters: deterministic,
//! rule-based, L3-measured. It runs **before** the frontend (and
//! before NJD's own numeral expansion), because its rules need to see
//! raw digits and symbols:
//!
//! - **date compounds**: 「2026年2月14日」 reads as とし/つき through the
//!   frontend — rewritten to 「2026ねん2がつ14日」 (日 is left alone;
//!   the frontend already reads じゅうよっか correctly).
//! - **currency/percent/unit/temperature symbols**: 「¥1,200」→
//!   「1,200円」, 「5%」→「5パーセント」, 「50m」→「50メートル」,
//!   「100℃」→「100度」 — symbols are outside the trained inventory
//!   and get stripped (measured: `5%` degraded a whole sentence).
//! - **colon time**: 「10:30」→「10時30分」 — the colon becomes a pause
//!   otherwise. Known limitation: aspect ratios like 「16:9」 are
//!   misread as time (v1 trade-off, documented).
//! - **minus sign**: a bare leading 「-5」→「マイナス5」 — ranges like
//!   「10-20」 are untouched (the sign only counts at string start or
//!   after whitespace/brackets).
//!
//! One combined scan, so `Correction` spans index the final output
//! text (no cross-rule shifting). The user dictionary runs before
//! this stage and is a separate layer.

use std::sync::LazyLock;

use regex::Regex;

use crate::traits::{NormalizerError, SpeechNormalizer};
use crate::types::{Correction, NormalizedText};

static DATE_SYMBOL_TIME_UNITS: LazyLock<Regex> = LazyLock::new(|| {
    // Alternation order matters: more specific patterns first.
    //  currency | percent | time | temperature | year | month | minus | units
    let pattern = "([¥$€£])(\\d[\\d,]*(?:\\.\\d+)?)".to_owned()
        + "|(\\d[\\d,]*(?:\\.\\d+)?)[％%]"
        + "|(\\d{1,2}):(\\d{2})"
        + "|(\\d[\\d,]*(?:\\.\\d+)?)(?:℃|°C)"
        + "|(\\d{1,4})年"
        + "|(\\d{1,2})月"
        + "|(^|[\\s(（「\\[])(-|−)(\\d+)"
        + "|(\\d[\\d,]*(?:\\.\\d+)?)(km|cm|mm|kg|ml|l|L|m|g)";
    Regex::new(&pattern).expect("normalizer pattern is valid")
});

/// The unit suffix → its Japanese reading (longest-first in the regex
/// alternation above, so 「mm」 wins over 「m」).
fn unit_reading(unit: &str) -> &'static str {
    match unit {
        "km" => "キロメートル",
        "cm" => "センチメートル",
        "mm" => "ミリメートル",
        "kg" => "キログラム",
        "ml" => "ミリリットル",
        "l" | "L" => "リットル",
        "m" => "メートル",
        "g" => "グラム",
        _ => "",
    }
}

fn currency_reading(symbol: &str) -> &'static str {
    match symbol {
        "¥" => "円",
        "$" => "ドル",
        "€" => "ユーロ",
        "£" => "ポンド",
        _ => "",
    }
}

/// The built-in ja normalizer.
#[derive(Debug, Clone, Default)]
pub struct JaNormalizer;

impl JaNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl SpeechNormalizer for JaNormalizer {
    fn normalize(&self, input: &str) -> Result<NormalizedText, NormalizerError> {
        let regex = &*DATE_SYMBOL_TIME_UNITS;
        let mut output = String::with_capacity(input.len());
        let mut corrections: Vec<Correction> = Vec::new();
        let mut last_end = 0usize;

        for cap in regex.captures_iter(input) {
            let mat = cap.get(0).expect("captures always include group 0");
            let span = mat.range();
            // Copy the unmatched gap verbatim.
            output.push_str(&input[last_end..span.start]);
            let matched = mat.as_str();

            // Boundary guard for bare unit letters: 「50mph」 must not
            // become 50メートルph. The regex cannot look ahead, so the
            // char after the match decides.
            let next_is_latin = input[span.end..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric());
            let replacement: Option<String> = if cap.get(13).is_some() && next_is_latin {
                None // unit letter glued to more Latin — leave verbatim
            } else if let Some(prefix) = cap.get(9) {
                // minus sign: prefix + マイナス + digits
                let sign = cap.get(10).map_or("", |m| m.as_str());
                let digits = cap.get(11).map_or("", |m| m.as_str());
                let _ = sign;
                Some(format!("{}マイナス{}", prefix.as_str(), digits))
            } else if let Some(symbol) = cap.get(1) {
                let number = cap.get(2).map_or("", |m| m.as_str());
                Some(format!("{number}{}", currency_reading(symbol.as_str())))
            } else if cap.get(3).is_some() {
                let number = cap.get(3).map_or("", |m| m.as_str());
                Some(format!("{number}パーセント"))
            } else if let (Some(h), Some(m)) = (cap.get(4), cap.get(5)) {
                Some(format!("{}時{}分", h.as_str(), m.as_str()))
            } else if let Some(number) = cap.get(6) {
                Some(format!("{}度", number.as_str()))
            } else if let Some(year) = cap.get(7) {
                Some(format!("{}ねん", year.as_str()))
            } else if let Some(month) = cap.get(8) {
                Some(format!("{}がつ", month.as_str()))
            } else if let Some(unit) = cap.get(13) {
                let number = mat
                    .as_str()
                    .trim_end_matches(|c: char| c.is_ascii_alphabetic());
                Some(format!("{number}{}", unit_reading(unit.as_str())))
            } else {
                None
            };

            match replacement {
                Some(new) => {
                    let span_start = output.chars().count();
                    output.push_str(&new);
                    corrections.push(Correction {
                        span: span_start..output.chars().count(),
                        from: matched.to_string(),
                        to: new,
                    });
                }
                None => output.push_str(matched),
            }
            last_end = span.end;
        }
        output.push_str(&input[last_end..]);

        Ok(NormalizedText {
            text: output,
            corrections,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalize(input: &str) -> NormalizedText {
        JaNormalizer::new().normalize(input).unwrap()
    }

    #[test]
    fn date_compound_reads_year_and_month() {
        let out = normalize("2026年2月14日に、東京で会議があります。");
        assert_eq!(out.text, "2026ねん2がつ14日に、東京で会議があります。");
        // 日 is untouched — the frontend reads じゅうよっか correctly.
        assert!(out.text.contains("14日"));
        assert_eq!(out.corrections.len(), 2);
        assert_eq!(out.corrections[0].from, "2026年");
        assert_eq!(out.corrections[0].to, "2026ねん");
    }

    #[test]
    fn standalone_year_and_month() {
        assert_eq!(normalize("2026年").text, "2026ねん");
        assert_eq!(normalize("2月14日").text, "2がつ14日");
    }

    #[test]
    fn kagetu_is_not_a_month() {
        // 「3カ月」 is a duration, not March — the 月 has no digit
        // directly before it.
        assert_eq!(normalize("3カ月間").text, "3カ月間");
        assert_eq!(normalize("1ヶ月").text, "1ヶ月");
    }

    #[test]
    fn currency_symbols_get_unit_suffixes() {
        assert_eq!(normalize("¥1,200").text, "1,200円");
        assert_eq!(normalize("$3.5").text, "3.5ドル");
        assert_eq!(normalize("€20").text, "20ユーロ");
        assert_eq!(normalize("£10").text, "10ポンド");
        // An explicit 円 is not doubled.
        assert_eq!(normalize("1,200円").text, "1,200円");
    }

    #[test]
    fn percent_gets_spoken_form() {
        assert_eq!(normalize("5%").text, "5パーセント");
        assert_eq!(
            normalize("成功率は5%上がりました").text,
            "成功率は5パーセント上がりました"
        );
    }

    #[test]
    fn unit_suffixes_expand() {
        assert_eq!(normalize("50m").text, "50メートル");
        assert_eq!(normalize("3km").text, "3キロメートル");
        assert_eq!(normalize("5mm").text, "5ミリメートル");
        assert_eq!(normalize("2kg").text, "2キログラム");
        assert_eq!(normalize("500ml").text, "500ミリリットル");
    }

    #[test]
    fn unit_letter_glued_to_latin_is_left_alone() {
        // Boundary guard: identifiers like 50mph stay verbatim.
        assert_eq!(normalize("50mph").text, "50mph");
    }

    #[test]
    fn temperature_reads_degrees() {
        assert_eq!(normalize("100℃").text, "100度");
        assert_eq!(normalize("36.5°C").text, "36.5度");
    }

    #[test]
    fn colon_time_reads_hour_and_minute() {
        assert_eq!(normalize("10:30").text, "10時30分");
        assert_eq!(normalize("9:05に集合").text, "9時05分に集合");
    }

    #[test]
    fn minus_sign_only_at_boundary() {
        assert_eq!(normalize("-5").text, "マイナス5");
        assert_eq!(normalize("気温は -5度です").text, "気温は マイナス5度です");
        // A range hyphen keeps its plain reading.
        assert_eq!(normalize("10-20人").text, "10-20人");
    }

    #[test]
    fn no_matches_pass_through() {
        let out = normalize("こんにちは、今日はいい天気ですね。");
        assert_eq!(out.text, "こんにちは、今日はいい天気ですね。");
        assert!(out.corrections.is_empty());
    }

    #[test]
    fn correction_spans_index_the_output() {
        let out = normalize("駅まで50mほど、5%です");
        let chars: Vec<char> = out.text.chars().collect();
        for correction in &out.corrections {
            let sliced: String = chars[correction.span.clone()].iter().collect();
            assert_eq!(sliced, correction.to);
        }
        assert_eq!(out.text, "駅まで50メートルほど、5パーセントです");
    }
}
