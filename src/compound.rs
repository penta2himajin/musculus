//! Compound-accent rules — musculus's deliberate deviation from the
//! reference frontend.
//!
//! The literature classifies Japanese compound nouns by the mora count of
//! the second element (NHK Broadcasting Culture Research Institute, the
//! 2016 dictionary revision notes) and adds the 窪薗・山本 (1999) subrules
//! for the 3–4 mora case:
//!
//! | N2 | pattern | nucleus |
//! |---|---|---|
//! | ≤ 2 morae | 前部末型 (most common) | N1's **last mora** |
//! | 3–4 morae | 後部一型 (most common) | N2's **first mora** when N2 is 頭高 or 平板/尾高 |
//! | 3–4 morae, N2 accented inside | N2 keeps its own nucleus | N2's nucleus |
//! | ≥ 5 morae | 後部保存型 | N2 keeps its own accent; N1 is deaccented |
//!
//! A nucleus that would land on a special mora (ン, ッ, a long vowel) shifts
//! one mora left, which the same literature states for the 1–2 mora case.
//!
//! Why this lives here rather than in the frontend: jpreprocess matches
//! OpenJTalk exactly (measured), and OpenJTalk's chain rules only partly
//! reproduce these patterns — the listener's own ear reproduced all three
//! test words (docs/accent-resources.md). The rule is applied to the tones
//! after g2p, before the user's override table, which therefore still wins.
//!
//! Numerals are excluded: they follow their own (left-dominant, separable
//! minor phrase) behaviour and are handled by the override table.

/// One token as the rules see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// First POS component ("名詞", "助詞", …).
    pub pos: String,
    /// True when the token is a numeral (名詞,数) — excluded from the rules.
    pub numeral: bool,
    /// Mora count of the token's reading.
    pub morae: usize,
    /// Dictionary accent position: 0 = heiban, N = nucleus on mora N
    /// (N may exceed the mora count, meaning "the fall is after the token").
    pub accent: i32,
    /// Per-mora flag: the mora is a special mora (ン, ッ, long vowel).
    pub special: Vec<bool>,
}

/// Where the nucleus of a compound sits, 1-based within the compound.
/// `0` means no fall inside the word (heiban / plateau).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nucleus {
    Heiban,
    At(usize),
}

/// The nucleus the rules predict for `N1 (already accumulated) + N2`.
pub fn compound_nucleus(n1_morae: usize, n2: &Token) -> Nucleus {
    let m2 = n2.morae;
    match m2 {
        0 => Nucleus::Heiban,
        // 前部末型: the nucleus moves onto N1's last mora.
        1..=2 => Nucleus::At(n1_morae),
        // 後部一型: N2's first mora, unless N2 carries its own nucleus
        // inside the word (頭高 / 中高), in which case that nucleus stands.
        3..=4 => {
            let accent = n2.accent;
            let keeps_own = accent >= 2 && (accent as usize) < m2;
            if keeps_own {
                Nucleus::At(n1_morae + accent as usize)
            } else {
                Nucleus::At(n1_morae + 1)
            }
        }
        // 後部保存型: N2 keeps its accent; a heiban N2 makes the whole flat.
        _ => {
            if n2.accent == 0 {
                Nucleus::Heiban
            } else {
                Nucleus::At(n1_morae + n2.accent as usize)
            }
        }
    }
}

/// Shift a nucleus off a special mora (it cannot carry the accent).
pub fn shift_off_special(nucleus: Nucleus, special: &[bool]) -> Nucleus {
    match nucleus {
        Nucleus::Heiban => Nucleus::Heiban,
        Nucleus::At(mut k) => {
            while k > 1 && special.get(k - 1).copied().unwrap_or(false) {
                k -= 1;
            }
            Nucleus::At(k)
        }
    }
}

/// The H/L pattern of a word of `morae` morae with nucleus `nucleus`.
pub fn pattern(morae: usize, nucleus: Nucleus) -> Vec<i32> {
    if morae == 0 {
        return Vec::new();
    }
    let mut out = vec![0; morae]; // L
    match nucleus {
        Nucleus::Heiban => out[1..].fill(1),
        Nucleus::At(1) => {
            out[1..].fill(0);
            out[0] = 1;
        }
        Nucleus::At(k) => {
            let k = k.min(morae);
            out[1..k].fill(1);
            out[0] = 0;
        }
    }
    out
}

/// One compound the rules rewrote, for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    /// Index of the first mora of the compound.
    pub start: usize,
    /// Number of morae in the compound.
    pub morae: usize,
    /// The nucleus the rules chose.
    pub nucleus: Nucleus,
}

/// Apply the rules to a token stream and the matching mora tones.
///
/// `morae` describes each token's mora count and special-mora flags in
/// order; `tones` holds one H/L value per mora (the mora view of the phone
/// stream). Returns the rewrites so callers can report them.
pub fn apply(tokens: &[Token], tones: &mut [i32]) -> Vec<Rewrite> {
    let mut rewrites = Vec::new();
    let mut index = Vec::with_capacity(tokens.len());
    let mut cursor = 0usize;
    for token in tokens {
        index.push(cursor);
        cursor += token.morae;
    }
    if cursor != tones.len() {
        // The alignment is a precondition: refuse rather than guess.
        return rewrites;
    }

    let mut i = 0usize;
    while i + 1 < tokens.len() {
        let head = &tokens[i];
        let next = &tokens[i + 1];
        if head.pos != "名詞" || next.pos != "名詞" || head.numeral || next.numeral {
            i += 1;
            continue;
        }
        // Accumulate a compound: the group grows while nouns keep coming.
        let start_token = i;
        let mut group_morae = head.morae;
        let group_start = index[i];
        let mut j = i + 1;
        let mut nucleus = compound_nucleus(group_morae, &tokens[j]);
        while j + 1 < tokens.len()
            && tokens[j].pos == "名詞"
            && !tokens[j].numeral
            && tokens[j + 1].pos == "名詞"
            && !tokens[j + 1].numeral
        {
            group_morae += tokens[j].morae;
            j += 1;
            nucleus = compound_nucleus(group_morae, &tokens[j]);
        }
        let total = group_morae + tokens[j].morae;
        let mut special = Vec::with_capacity(total);
        for token in &tokens[start_token..=j] {
            special.extend_from_slice(&token.special);
        }
        let nucleus = shift_off_special(nucleus, &special);
        let pattern = pattern(total, nucleus);
        tones[group_start..group_start + total].copy_from_slice(&pattern);
        rewrites.push(Rewrite {
            start: group_start,
            morae: total,
            nucleus,
        });
        i = j + 1;
    }
    rewrites
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noun(morae: usize, accent: i32) -> Token {
        Token {
            pos: "名詞".into(),
            numeral: false,
            morae,
            accent,
            special: vec![false; morae],
        }
    }

    #[test]
    fn short_second_element_takes_the_nucleus_on_n1s_last_mora() {
        // 個人情報保護 = (個人 3 + 情報 4) + 保護 2.
        let tokens = [noun(3, 1), noun(4, 0), noun(2, 1)];
        // 情報 is a 4-mora heiban N2: 後部一型 puts the nucleus on its first
        // mora, i.e. mora 4 of the group.
        assert_eq!(compound_nucleus(3, &tokens[1]), Nucleus::At(4));
        // Now 個人情報 (7 morae) is the head and 保護 (2 morae) is short:
        // 前部末型 puts the nucleus on the head's last mora, 7 = ホ.
        assert_eq!(compound_nucleus(7, &tokens[2]), Nucleus::At(7));
    }

    #[test]
    fn three_to_four_mora_heiban_second_element_takes_its_first_mora() {
        // 教師なし学習: N2 = 学習 (4, heiban) -> nucleus on ガ.
        assert_eq!(compound_nucleus(5, &noun(4, 0)), Nucleus::At(6));
        // 尾高 counts as heiban for this rule.
        assert_eq!(compound_nucleus(5, &noun(4, 4)), Nucleus::At(6));
        // 中高 keeps its own nucleus.
        assert_eq!(compound_nucleus(5, &noun(4, 3)), Nucleus::At(8));
    }

    #[test]
    fn five_mora_second_element_is_preserved_and_heiban_goes_flat() {
        // 地球温暖化: N2 = 温暖化 (5, heiban) -> whole word flat.
        assert_eq!(compound_nucleus(3, &noun(5, 0)), Nucleus::Heiban);
        // An accented N2 keeps its nucleus.
        assert_eq!(compound_nucleus(3, &noun(5, 2)), Nucleus::At(5));
    }

    #[test]
    fn a_special_mora_shifts_the_nucleus_left() {
        // ン cannot carry the accent: 個人 + 新聞 (N2 = 2 morae) would put the
        // nucleus on ン, so it moves to ジ.
        let mut head = noun(2, 1);
        head.special = vec![false, true];
        let special = [false, true, false, false];
        assert_eq!(shift_off_special(Nucleus::At(2), &special), Nucleus::At(1));
    }

    #[test]
    fn pattern_matches_the_three_shapes() {
        assert_eq!(pattern(4, Nucleus::Heiban), vec![0, 1, 1, 1]);
        assert_eq!(pattern(4, Nucleus::At(1)), vec![1, 0, 0, 0]);
        assert_eq!(pattern(4, Nucleus::At(3)), vec![0, 1, 1, 0]);
    }

    #[test]
    fn apply_rewrites_a_two_token_compound() {
        let tokens = [noun(3, 0), noun(4, 0)];
        let mut tones = vec![0; 7];
        let rewrites = apply(&tokens, &mut tones);
        assert_eq!(rewrites.len(), 1);
        assert_eq!(rewrites[0].nucleus, Nucleus::At(4)); // N2's first mora
        assert_eq!(tones, vec![0, 1, 1, 1, 0, 0, 0]);
    }

    #[test]
    fn apply_skips_numerals_and_non_nouns() {
        let mut numeral = noun(2, 1);
        numeral.numeral = true;
        let tokens = [numeral, noun(4, 0)];
        let mut tones = vec![0; 6];
        assert!(apply(&tokens, &mut tones).is_empty());
        let tokens = [
            noun(2, 0),
            Token {
                pos: "助詞".into(),
                numeral: false,
                morae: 1,
                accent: 0,
                special: vec![false],
            },
        ];
        assert!(apply(&tokens, &mut tones).is_empty());
    }

    #[test]
    fn apply_refuses_misaligned_input() {
        let tokens = [noun(3, 0), noun(4, 0)];
        let mut tones = vec![0; 5];
        assert!(apply(&tokens, &mut tones).is_empty());
    }
}
