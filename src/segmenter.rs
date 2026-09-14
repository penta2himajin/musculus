//! Sentence segmentation and audio stitching — the small piece of the
//! pipeline that sits between text processing and synthesis.
//!
//! euhadra's VAD decides where one utterance ends and the next begins;
//! on the synthesis side the mirror decision is where to break a long
//! text so each sentence is synthesized with its own prosody and the
//! joins get a breath of silence. The reference SBV2 implementations do
//! this for long input; musculus makes it an explicit, measurable choice
//! (the "breathless delivery" hypothesis, docs/benchmarks/listening-log.md).

use crate::types::AudioChunk;

/// Sentence-final punctuation that ends a segment (kept with the
/// sentence it closes).
const TERMINATORS: [char; 6] = ['。', '！', '？', '!', '?', '.'];

/// Split `text` into sentence-ish segments, keeping the terminating
/// punctuation attached and dropping whitespace-only pieces.
///
/// Newlines always break. Consecutive terminators stay together
/// (「……。」 is one break, not three).
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' {
            let piece = current.trim();
            if !piece.is_empty() {
                out.push(piece.to_string());
            }
            current.clear();
            continue;
        }
        current.push(c);
        if TERMINATORS.contains(&c) {
            // Absorb any run of terminators and closing quotes.
            while let Some(&next) = chars.peek() {
                if TERMINATORS.contains(&next)
                    || next == '」'
                    || next == '』'
                    || next == '）'
                    || next == ')'
                {
                    current.push(chars.next().expect("peeked"));
                } else {
                    break;
                }
            }
            let piece = current.trim();
            if !piece.is_empty() {
                out.push(piece.to_string());
            }
            current.clear();
        }
    }
    let piece = current.trim();
    if !piece.is_empty() {
        out.push(piece.to_string());
    }
    out
}

/// Group sentences so that each group is at most `max_chars` characters
/// (measured in codepoints), always keeping at least one sentence per
/// group.
///
/// This is the middle ground the split A/B asked for
/// (ab-test-sbv2-split/score-sheet.md): synthesizing everything in one
/// pass makes long text sound breathless, while splitting every sentence
/// breaks the flow across short ones. Grouping keeps short sentences
/// together and cuts only when a group has grown long. `max_chars == 0`
/// returns one group per sentence (the old split behaviour).
pub fn group_sentences(text: &str, max_chars: usize) -> Vec<String> {
    let sentences = split_sentences(text);
    if max_chars == 0 {
        return sentences;
    }
    let mut groups: Vec<String> = Vec::new();
    let mut current = String::new();
    for sentence in sentences {
        if current.is_empty() {
            current = sentence;
            continue;
        }
        let would_be = current.chars().count() + sentence.chars().count();
        if would_be <= max_chars {
            current.push_str(&sentence);
        } else {
            groups.push(std::mem::take(&mut current));
            current = sentence;
        }
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// Concatenate chunks, inserting `silence_secs` of silence between
/// them. The sample rate comes from the first chunk; chunks are expected
/// to share it (one engine decodes at one rate).
pub fn stitch_with_silence(chunks: &[AudioChunk], silence_secs: f32) -> Option<AudioChunk> {
    let sample_rate = chunks.first()?.sample_rate;
    let silence_len = (silence_secs.max(0.0) * sample_rate as f32) as usize;
    let mut samples = Vec::new();
    for (i, chunk) in chunks.iter().enumerate() {
        if i > 0 && silence_len > 0 {
            samples.extend(std::iter::repeat(0.0f32).take(silence_len));
        }
        samples.extend_from_slice(&chunk.samples);
    }
    Some(AudioChunk {
        samples,
        sample_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_on_japanese_and_ascii_terminators() {
        let parts = split_sentences("こんにちは。今日はいい天気ですね！明日は?");
        assert_eq!(
            parts,
            vec!["こんにちは。", "今日はいい天気ですね！", "明日は?"]
        );
    }

    #[test]
    fn keeps_trailing_quotes_and_terminator_runs_together() {
        // A terminator run absorbs following terminators and closing
        // quotes; text after that run starts the next piece.
        let parts = split_sentences("「そうですか。」なるほど。");
        assert_eq!(parts, vec!["「そうですか。」", "なるほど。"]);
        let parts = split_sentences("本当ですか……。そうですか。");
        assert_eq!(parts, vec!["本当ですか……。", "そうですか。"]);
    }

    #[test]
    fn newlines_break_without_punctuation() {
        let parts = split_sentences("一行目\n二行目\n\n三行目");
        assert_eq!(parts, vec!["一行目", "二行目", "三行目"]);
    }

    #[test]
    fn a_single_sentence_stays_one_segment() {
        assert_eq!(split_sentences("こんにちは"), vec!["こんにちは"]);
        assert!(split_sentences("   \n  ").is_empty());
    }

    #[test]
    fn grouping_keeps_short_sentences_together() {
        // 8 + 13 = 21 codepoints, under the limit -> one group.
        let groups = group_sentences("こんにちは。今日はとても良い天気ですね。", 60);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0], "こんにちは。今日はとても良い天気ですね。");
    }

    #[test]
    fn grouping_cuts_when_a_group_grows_long() {
        // The same text is 19 + 39 = 58 codepoints: a 40-character budget
        // cuts it in two, a 60-character budget keeps it as one group.
        let text = "その森には、古い言い伝えがありました。月が最も高く昇る夜、静かに耳を澄ませば、風の歌声が聞こえるというのです。";
        let cut = group_sentences(text, 40);
        assert_eq!(cut.len(), 2);
        assert!(cut[0].ends_with('。') && cut[1].ends_with('。'));
        let kept = group_sentences(text, 60);
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn grouping_zero_is_one_group_per_sentence() {
        let text = "あ。い。う。";
        assert_eq!(group_sentences(text, 0).len(), 3);
    }

    #[test]
    fn a_single_long_sentence_still_yields_one_group() {
        let long = "あ".repeat(500) + "。";
        assert_eq!(group_sentences(&long, 60).len(), 1);
    }

    #[test]
    fn stitching_inserts_silence_between_chunks() {
        let a = AudioChunk {
            samples: vec![1.0; 100],
            sample_rate: 1000,
        };
        let b = AudioChunk {
            samples: vec![2.0; 50],
            sample_rate: 1000,
        };
        let stitched = stitch_with_silence(&[a, b], 0.1).unwrap();
        // 100 + 100 (0.1 s at 1 kHz) + 50
        assert_eq!(stitched.samples.len(), 250);
        assert!(stitched.samples[100..200].iter().all(|&v| v == 0.0));
        assert_eq!(stitched.samples[0], 1.0);
        assert_eq!(stitched.samples[200], 2.0);
    }

    #[test]
    fn stitching_no_chunks_is_none() {
        assert!(stitch_with_silence(&[], 0.5).is_none());
    }
}
