# Accent and prosody resources for later use

Reference inventory for the accent workstream (ADR-0006). Everything here
is recorded so a later session can pick it up without re-surveying the
field. **musculus bundles none of it** — dictionaries and corpora are
fetched by a setup script or supplied by the consumer.

## What we can use

| Resource | Licence | How it would be used | Status |
|---|---|---|---|
| [tdmelodic](https://github.com/PKSHATechnology-Research/tdmelodic) — Japanese accent dictionary generator (PKSHATechnology-Research) | **BSD-3-Clause** | Generates a large-vocabulary accent dictionary from UniDic + NEologd with a neural estimator ([Tachibana & Katayama, ICASSP 2020](https://doi.org/10.1109/ICASSP40776.2020.9054081)). This is the intended source of the standard-accent layer: emit word entries (surface, reading, accent type, mora count) and feed them to the frontend so the *labels* are right | not started; needs Python/torch and a dictionary build |
| [tdmelodic_openjtalk](https://github.com/sarulab-speech/tdmelodic_openjtalk) | **no licence file** (GitHub reports none) → all rights reserved by default | Would have turned tdmelodic output into an OpenJTalk dictionary, but **we must not use this code**. The step it performs is a data-format conversion we can implement ourselves from tdmelodic's BSD-3 output | **rejected on licence grounds**; write our own converter instead |
| [UniDic](https://clrd.ninjal.ac.jp/unidic/) | modern editions **GPLv2 / LGPLv2.1 / modified BSD (triple)** — commercial use free | Vocabulary + accent source for tdmelodic | licence confirmed on the CLRD page |
| [UniDic non-core data](https://teru-oka-1933.github.io/unidic_non_core/) | **Apache-2.0** (changed from MIT in 2019-08) | Additional information keyed by UniDic short-unit IDs. The page checked publishes katakana-abbreviation and compound-splitting lists; the accent annotation from KAKEN 19K13173 was not on that page — locate it before relying on it | partially verified |
| [koniwa (声庭)](https://github.com/koniwa/koniwa) | annotations **CC0**, programs **Apache-2.0**, audio CC BY / public domain | Openly licensed **prosody-annotated Japanese speech**: gold for the L3 accent gate and a verification corpus for the accent layer, instead of hand-authoring everything | not started; confirm the annotation schema carries accent (mora H/L or nucleus position) |
| [KAKEN 19K13173](https://kaken.nii.ac.jp/ja/grant/KAKENHI-PROJECT-19K13173/) — accent information for UniDic | research report is public | Describes the crowdsourced accent annotation method (audio comparison tasks, gold filtering, Bayesian worker/question estimation, weighted majority) | useful as method reference |

## What we must not use

| Resource | Why |
|---|---|
| [OJAD](https://www.gavo.t.u-tokyo.ac.jp/ojad/pages/notes) | Limited to education/academic use; the site explicitly asks companies not to use it for commercial R&D |
| 『NHK日本語発音アクセント新辞典』 | A copyrighted work. **Reference use only**: its *published principles* (the NHK Broadcasting Culture Research Institute articles) may inform our rules, but the dictionary data must never be copied into musculus or its tests. A consumer who owns the dictionary may enter the forms they need into their own accent override table |
| [NHK「数詞＋助数詞」の発音とアクセント](https://www.nhk.or.jp/bunken/research/kotoba/pdf/20160901_5.pdf) / [数詞の発音とアクセント](https://www.nhk.or.jp/bunken/research/kotoba/20180501_4.html) | These are research articles (quotable, citable). We use them for **rules and trends**, never as a data dump |

## Rules already extracted from the literature

Recorded in full in `docs/benchmarks/accent/ja-report.md`; the short form:

1. Short compounds with 千 are penultimate-accented (千 = /séN/, 二千 = /ni-séN/).
2. A -sen or larger number combines with a following number as a **separable minor phrase**, and the first component can lose its nucleus (千二百 → セン flat).
3. Number + 円: the NHK 2016 dictionary simplifies toward a fall **before** 円; the traditional 円 accent (エン[1]) remains a documented variant.
4. An accentual phrase carries **at most one accent**, and a boundary is inserted between two words when at least one is underlyingly accented (three nuclei in 二千二百円 follow from this).
5. Initial lowering is suppressed when the phrase-initial mora is accented or the initial syllable is heavy, and its degree is smaller at minor-phrase edges than at major-phrase edges.

## Worked example already in the repo

`examples/accent-overrides.json` carries the two listener-confirmed forms
and `examples/accent_report.rs --accent <file>` verifies them:

```
[ok] 1,200        tones LLLHH    (frontend alone: HLLHH)
[ok] 二千二百円   tones HHLLHHHL (frontend alone: LHLLHHLL)
```

## Next actions in order (settled 2026-09-14)

The order below follows from what each step de-risks. The tdmelodic route
is the highest-ceiling answer but its quality on **numeral compounds** is
unproven (UniDic stores accents per word while numerals are compositional),
and its setup is a one-time Docker/Python + UniDic cost. So we test it
empirically before committing to rules that might duplicate it.

1. **Build the verification base** (cheap, unblocks judgement):
   - keep the listener-confirmed forms in `tests/evaluation/annotations/ja_accent.jsonl` (2 gated today)
   - check koniwa's annotation schema and, if it carries accent, extract L3 gold from it (annotations are CC0)
   - add the "at most one nucleus per accentual phrase" invariant as a test
2. **Test tdmelodic empirically** (the decisive question):
   - stand up the tool (Docker or Python + UniDic download), generate a dictionary
   - run it on exactly our problem cases — 1,200 / 二千二百円 / 千五百 / 先週と比べると — and compare with the listener's forms and the gold set
   - check whether jpreprocess can load an externally built dictionary; if not, feed the generated data through our own `AccentTable`/a table provider
   - write the OpenJTalk-format conversion ourselves (tdmelodic_openjtalk is off-limits)
3. **Implement the winner**:
   - if tdmelodic fixes the cases → the standard-accent layer becomes the default, with the override table kept for speaker preference
   - if it does not → implement the numeral rules (nucleus loss in -sen + number, then the 円 variant), each validated against the gold set and by listening
4. **Extend the override table opportunistically** — it is the only mechanism
   that captures a *speaker's* choice, so it stays regardless of 2/3.

---

## Mechanism map: how the accent actually gets decided (2026-09-14)

Investigated end to end, because it decides where a fix can live. All of it
is **Rust-only** — no Python or torch is needed for this route.

### 1. The dictionary entries carry the accent

NAIST-jdic is downloaded at build time by `jpreprocess-naist-jdic`'s
build.rs (an 18 MB tarball, 52 MB CSV) from
`https://github.com/jpreprocess/naist-jdic/archive/refs/tags/v0.1.3.tar.gz`.
Its CSV columns are `surface,left,right,cost,pos…,orig,read,pron,accent/mora,chain,…`
and the numeral entries are:

```
千,1355,1355,4255,名詞,数,*,*,*,*,千,セン,セン,1/2,C3
百,1355,1355,2709,名詞,数,*,*,*,*,百,ヒャク,ヒャク,2/2,C3
```

So **千 = accent 1 of 2 morae (セ＼ン) and 百 = accent 2**, and there is
**no entry for 千二百 or 二千二百** — compounds are composed by NJD. The
dictionary data therefore already matches the listener's "千円 is セH ンL".

### 2. Compounds are composed by accent chain rules

`jpreprocess-core/src/accent_rule.rs` parses a `chain` string into a rule
set (`F1–F5`, `C1–C5`, `P1/P2/P6/P14`, optional `pos%` selector, optional
`@add` offset). `jpreprocess-njd/src/open_jtalk/accent_type.rs` applies it:

```rust
let rule = node.get_chain_rule().get_rule(prev.get_pos())?;
let accent = match rule.accent_type {
    C1 => mora_size + node_acc,
    C2 => mora_size + 1,
    C3 => mora_size,          // <- 千/百 carry C3
    C4 => 0,
    C5 => top_node_acc,
    …
};
```

**C3 = "accent = mora_size"**: for a 2-mora 千 that means accent 2, i.e.
セH ン**H** — the flat/high side of the listener's 千二百 (HHLHH). The
standard machinery should produce exactly what the listener expects, which
makes this a **pipeline bug rather than missing data**.

### 3. What our pipeline actually does

`accent_report --njd` (new) shows the tokenisation after
`text_to_njd` + `preprocess`:

| input | tokens (chain rules) |
|---|---|
| `1,200` | 千(**\***) 二(**C3**) 百(**\***) |
| `千五百` | 千(C3) 五(C3) 百(C3) |
| `二千二百円` | 二(C3) 千(C3) 二(C3) 百(C3) 円(C3) |

- For **digits**, the digit-sequence rewrite builds 千/百 tokens **without a
  chain rule** (`chain=*`), so C3 can never fire and 千 keeps its lexical
  accent 1 → our HLLHH instead of HHLHH. This is the concrete defect for
  `1,200`.
- For kanji input the chain rules *are* present, yet the labels still show
  accent 1 on 千, so there is a second question: whether `njd_set_accent_type`
  fires (and which node's rule governs which accent). That needs one more
  diagnostic: dump each node's accent before and after the accent steps.

### 4. Where a fix can live (all in musculus)

1. **Override table** — implemented and working (`--accent`); fixes any case
   the listener confirms, including multi-nucleus patterns a single
   dictionary entry cannot express.
2. **Our own numeral normalisation** — rewrite numbers in musculus's rule
   layer *before* NJD so the kanji/chain-rule path (or an explicit user
   dictionary entry) is preserved instead of the digit-sequence path that
   drops chain rules. Fits the "rules + CI-measurable ground truth" bet.
3. **Accent-type diagnostic then a targeted fix** — if the C3 application
   is simply not firing in our call order, correcting that would restore the
   standard behaviour for the whole class of numeral compounds at once.
4. A **user dictionary in jpreprocess format** (accent + chain columns,
   built with `dict_tools Build --user`) is supported by
   `JPreprocessConfig.user_dictionary`, so corrected entries can be supplied
   as data once we know which entry/rule to change.

### 5. Environment facts for the tdmelodic route

Docker CLI exists but **the daemon is unavailable** here, so tdmelodic's
Docker workflow is out; **Python 3.14.6 + torch 2.13.0 are available**, so
its Python route is feasible if it ever becomes necessary. Given (3), the
jpreprocess route looks both cheaper and more likely to be the real fix.

---

## Decisive comparison: jpreprocess matches the OpenJTalk reference (2026-09-14)

Installed **pyopenjtalk into a workspace-local `.venv`** (gitignored) as a
reference oracle and compared its NJD nodes with ours. The result changes
the plan:

| input | reference (OpenJTalk) | ours (jpreprocess) |
|---|---|---|
| 千円 | 千 acc=**2**/2, chain C3; 円 1/2 | — (same machinery) |
| 千二百 | 千 acc=**1**/2, 二 3/1, 百 2/2 (all C3) | **identical** |
| 千五百 | 千 acc=**1**/2, 五 3/1, 百 2/2 (all C3) | **identical** |
| 二千二百円 | 二 2/1, 千 acc=**1**/2, 二 3/1, 百 2/2, 円 1/2 | **identical** |
| 1,200 | 千 acc=1 chain **\***, 二 3/1, 百 2/2 chain **\*** | **identical** (including the dropped chain rules) |

**jpreprocess is faithful.** There is no port bug, and the earlier
"pipeline defect" framing was wrong: what the listener hears differs from
the announcer norm because **OpenJTalk's own numeral accent assignment
differs from that norm** (note it even treats 千円 as accent 2 and
千二百 as accent 1, i.e. it is context-sensitive, just differently).
Our frontend is therefore a solid *baseline*, not a bug to fix.

Consequences:
1. The fix must be an **intentional deviation** on top of the reference:
   our accent override table (done) and a rule layer derived from the
   literature — each deviation justified by the sources and confirmed by
   the listener.
2. The reference oracle is now a **regression baseline**: our labels must
   match pyopenjtalk except where we deliberately deviate, which makes
   accidental drift detectable in CI.
3. Why jpreprocess behaves this way is visible in
   `jpreprocess-njd/src/open_jtalk/accent_type.rs`: numerals take a
   dedicated `calc_digit_acc` path (it is what turned 五 into 3/1), while
   the chain rule (C3 = "accent = mora_size") is applied to the accent
   phrase's *top* node. The numeral tables trace back to OpenJTalk's
   digit LUTs, which is why the reference agrees.

## koniwa's schema: reading gold, not accent gold (2026-09-14)

`koniwa/schema.py` defines `Span { text_level0, kana_level0, text_level2,
kana_level3, memo }` inside `Annotation { start, end, data }` plus
`Meta { duration, series, album, title, … }`. That is **time-aligned
transcription at several levels (text and kana) with no accent or pitch
field**.

So koniwa is usable as (a) **reading/pronunciation gold with timings** and
(b) a source of audio for a future F0-based accent measurement — but it
cannot by itself provide H/L accent gold. Accent gold therefore comes from
the listener's annotations (already the case) and, later, from F0 analysis
or an accent-labelled lexicon.

## Deviations are opt-in: what the first rule taught us (2026-09-14)

Implemented the deviation layer (`JaProcess::apply_accent_deviations`, called
by the adapter only when enabled; CLI/`eval_cer` flag `--accent-deviations`;
`accent_report --deviations`). It rewrites field 10 (accent/mora) of the NJD
node strings before the labels are generated, i.e. the same level the
reference works at.

Rule 1 (a 千 followed by another numeral) was tried with two encodings and
measured:

| encoding | 千二百 tones | reading |
|---|---|---|
| reference (no deviation) | `HLLHH` | セH ンL (the form the listener rejects) |
| accent = 0 (heiban) | `LHLHH` | セL ンH |
| accent = mora count (odaka, what C3 computes) | `LHLHH` | セL ンH |

**Measured mapping** for a phrase-initial 2-mora word: accent 1 -> HL,
accent 0 -> LH, accent = mora count -> LH. Heiban and odaka are
indistinguishable in the H/L feature because the difference between them
falls *after* the word, on the following particle. The listener's target for
セン is a **plateau** (HH or LL), which neither encoding produces: the
phrase-initial rise is inserted by the prosody markers, so reaching HH/LL
needs **phrase-level control** (suppressing the initial rise or merging the
phrases), not a word-accent change.

Therefore rule 1 stays **experimental and off by default**, and the
listener's confirmed form is carried by the override table (which sets the
tones directly and does produce `LLLHH`).

Three candidates for the same sentence were rendered for an ear decision
(48 kHz, -20 LUFS, `accent-cand-{a,b,c}.wav`):

| file | path | 1,200円 tones |
|---|---|---|
| a | reference | `HLLHH` |
| b | deviation (odaka) | `LHLHH` |
| c | override table | `LLLHH` |

Next: pick the direction by ear. If the mid-sentence `HH` variant is wanted,
the next implementation is phrase-level (suppress the initial rise when the
千 phrase does not start the utterance), which our deviation layer can also
express once we know how to set the chain flag / phrase grouping.

## First shipped deviation: polite prefixes お / ご (2026-09-14)

The listener could not judge the numeral candidates because ご注文は itself
sounded wrong. Measured and traced:

- ours and the reference agree again: pyopenjtalk's labels for ご注文は are
  `A:-1+1+6` and `F:6_2#0_xx@1_1|1_6`, i.e. a1=-1, a2=1, a3=6, f1=6, f2=2 —
  **identical to ours**. The prefix ご gets acc=2 ("accent after the
  prefix"), which puts a nucleus at mora 2 of the phrase, so the realisation
  is ゴL チュH ウL モL ンL ワL: a fall right after チュ.
- The dictionaries disagree with that: NAIST-jdic has 注文 = チュウモン
  **0/4** (heiban, chain C2), and the standard references agree — 注文 is
  [0] heiban, and the literature records that the polite prefixes お / ご
  normally leave the base word's accent untouched (お勉強 stays heiban).
  So the listener's expectation, ゴL + チュウモンハ all high, is the norm and
  OpenJTalk is the outlier here.

Implemented as the deviation layer's **rule 1** (renumbered; the numeral
rule was removed, see above): a 接頭詞 お/ご followed by a heiban node has
its accent set to 0. Measured effect:

| input | baseline (reference) | with the deviation |
|---|---|---|
| ご注文は | `LHLLLL` (ゴL チュH ウL モL ンL ワL) | **`LHHHHH`** (ゴL チュH ウH モH ンH ワH) |
| ご注文 | `LHLLL` | **`LHHHH`** |
| お名前 | `LHLL` | **`LHHH`** |
| お勉強 | `LHLLL` | **`LHHHH`** |
| ご連絡 | `LHLLL` | **`LHHHH`** |

The class fixed here (お/ご + heiban noun) is far more common than the
numeral cases, so this is the higher-value deviation. Known limitation: the
literature notes rare exceptions where お/ご *do* change the base accent
(ご指導 is cited); our rule forces heiban there as well, and such a word can
be corrected with the override table.

Listening candidates for the same sentence, 48 kHz / -20 LUFS:

| file | configuration | ご注文は / 1,200円 tones |
|---|---|---|
| gift-fix-a.wav | baseline | `LHLLLL` / `HLLHH` |
| gift-fix-b.wav | deviations + override table | `LHHHHH` / `LLLHH` |

Gates: with the product configuration (deviations + the override table) the
accent report reports **2 annotated, 0 mismatches**.

## Rule promoted to default (2026-09-14)

The listener judged `gift-fix-b.wav` (prefix deviation + override table)
"quite natural" and good, so the polite-prefix deviation is **on by
default**: `Sbv2Adapter` starts with `accent_deviations: true`, the CLI and
`eval_cer` expose `--no-accent-deviations` for a reference-faithful reading,
and `accent_report --baseline` shows the raw reference view while the report
default is the product configuration. With the override table the gate is
**2 annotated, 0 mismatches**.

The 1,200円 `LLLHH` form carried by the override table was part of that same
approved file, so it stands for this sentence; the phrase-level `HH` variant
remains an unimplemented, lower-priority option.

## Regression gate is in place (2026-09-14)

`tests/l3_accent.rs` holds both lines described above:

- the baseline matches `tests/evaluation/annotations/ja_accent_reference.jsonl`,
  generated from pyopenjtalk by `scripts/gen_accent_reference.py` (the
  oracle's version is recorded in the fixture's `_meta` line). Current
  result: **18/18 items match**.
- items with `expected_tones` in `ja_accent.jsonl` must produce exactly
  those tones under the shipped configuration (deviations + the sample
  override table); items without one must be untouched by the deviation
  layer. Current: 8 intended deviations, 0 containment violations.

Adding a deviation therefore means declaring it in the annotations with a
reason — otherwise the gate fails. That is the mechanism that keeps
"intended" and "accidental" apart.
