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

## Lexical exceptions: ご指導 (2026-09-14)

The listener confirmed that ご指導 is accented — ゴL シH ドL ウL (`LHLL`) —
so the reference's realisation is right there and the prefix rule was wrong
to flatten it. The dictionary cannot tell the two apart: 指導 is marked
**0/3 (heiban)** exactly like 注文, and the reference gives both prefixes
acc=2/1. The difference is lexical (the literature cites ご指導 as the rare
case where お/ご *do* change the base accent), so the rule now carries a
listener-confirmed exception list (`PREFIX_ACCENT_EXCEPTIONS = ["シドウ"]`)
and `ご指導 -> LHLL` is declared in `ja_accent.jsonl`, which the regression
gate checks. Measured: ご注文は `LHHHHH` (heiban, listener-approved) and
ご指導 `LHLL` (accented, listener-confirmed).

## Listening set for 先週と比べると (2026-09-14)

The listener reports two acceptable shapes — 比べると continuing high versus
being separated with a fall — and wants to compare them by ear. Three
candidates were rendered (48 kHz, -20 LUFS, same phrase, in
`listening/05-shinshuu-variants/`):

| file | tones | shape |
|---|---|---|
| shinshuu-1.wav | `LHHHHLHHHH` | reference: 先週と heiban, ク falls, ラベルト high |
| shinshuu-2.wav | `LHHHHHHHHH` | 比べると continuous high (override `クラベルト -> HHHHH`) |
| shinshuu-3.wav | `LHHHHHHHHL` | continuous high with the final ト low (override `クラベルト -> HHHHL`) |

Both variant shapes are expressible with the override table we already ship,
so whichever the ear picks can be declared in the annotations and protected
by the gate straight away.

### High-initial variants (2026-09-14)

The listener asked for versions of 先週と比べると with the initial セ high. The
override table expresses this directly (tones are set absolutely), so the
phrase-level shape that the rule path could not reach is still available as
data. Six candidates now sit in `listening/05-shinshuu-variants/`, paired by initial mora:

| file | tones | note |
|---|---|---|
| shinshuu-1 | `LHHHHLHHHH` | reference (ク falls) |
| shinshuu-2 | `LHHHHHHHHH` | 比べると continuous high |
| shinshuu-3 | `LHHHHHHHHL` | continuous high, final ト low |
| shinshuu-4 | `HHHHHLHHHH` | セ high + ク falls |
| shinshuu-5 | `HHHHHHHHHH` | セ high + everything high |
| shinshuu-6 | `HHHHHHHHHL` | セ high + final ト low |

### Blinded set for third-party listeners (2026-09-14)

The listener is collecting evaluations from acquaintances, so a blinded copy
was prepared: `listening/07-blind-shinshuu/` holds the same six takes under neutral
names **A–F in a randomly shuffled order**, with `key.ndjson` gitignored (the
mapping is revealed only after scoring). Files and sheets were checked to
contain no mention of tone patterns, and all six measure exactly
**-20.00 LUFS**, so neither the names, the docs, nor the loudness can leak
which take is which. `score-sheet.md` asks for a 1–5 naturalness rating per
take plus a single top choice, which makes the answers comparable across
listeners.

Tallying once the sheets come back: reveal `key.ndjson`, then compute the
mean naturalness per variant and count top choices; record the outcome in
docs/benchmarks/listening-log.md and, if it overrides the current choice,
update `examples/accent-overrides.json` plus the `expected_tones` entry so
the regression gate follows.

### Blind round reversed the initial-mora choice (2026-09-14)

With the blinded A–F set, the listener picked **C or F** — and both are the
**high-initial (セH)** variants, the opposite of the explicitly chosen D
(candidate 3, セL) from the previous round. This is the first time a blind
comparison overturned an explicit choice, and it lines up with the
literature on initial-lowering suppression and with the listener's own
二千二百円 production (ニH).

Registered: **F = セH + continuous high + final ト low**
(`HHHHHHHHHL`), expressed compositionally in the override table
(`センシュウト -> HHHHH`, `クラベルト -> HHHHL`) and declared as
`expected_tones` so the regression gate follows. A product-rendered
reference take is at `listening/05-shinshuu-variants/shinshuu-registered-F.wav` (48 kHz,
-20 LUFS).

Third-party sheets from the blinded set are still outstanding; when they
arrive, tally mean naturalness and top choices per label and re-open the
decision if the group disagrees.

## The dictionary integration point is verified (2026-09-14)

The cheap check before investing in tdmelodic: does jpreprocess actually
accept an externally built dictionary and honour its accent columns?

Done end to end with our own data:

1. `cargo install jpreprocess-dictionary --features binary` gives
   `dict_tools`.
2. A one-entry CSV in the dictionary format (16 columns:
   `surface,left,right,cost,pos1..pos4,ctype,cform,orig,read,pron,accent/mora,chain,flag`)
   re-declares 千 as `2/2` instead of the system's `1/2`.
3. `dict_tools build --user jpreprocess <csv> <bin>` builds a 1 KB
   jpreprocess-format user dictionary.
4. `JaFrontend::with_user_dictionary(path)` loads it through
   `JPreprocessConfig { user_dictionary: Some({"path": …}) }` — new library
   API; `accent_report --user-dict <path>` exposes it for measurement.

Measured effect (baseline → with the user dictionary):

| input | without | with |
|---|---|---|
| 千二百 | `HLLHH` | **`LHLHH`** |
| 千五百 | `HLLHH` | **`LHLHH`** |
| 二千二百 | `LHLLHH` | `LHLLHH` (unchanged) |

**So the mechanism works** — the accent/mora column of a dictionary entry
reaches the labels and moves the realisation.

**Important nuance for the tdmelodic plan**: the *accent phrase head*
governs the realisation. In 二千二百 the head is 二, so re-declaring 千's
accent changes its node (verified in the `--njd` dump: 千 2/2) but not the
tone pattern. A generated standard-accent dictionary will therefore fix
head words broadly, while compound numerals (whose head is the first
numeral token) may still need whole-phrase entries or our rule/override
layer. That is consistent with everything measured earlier.

Pinned by `tests/user_dictionary.rs`, which loads the committed 1 KB fixture
(`tests/evaluation/fixtures/accent-userdict.{csv,bin}`) and asserts both the
change (千二百) and the head-governed non-change (二千二百). A dictionary
format bump fails the test loudly, which is the intent — rebuild the fixture
with the command recorded in the test header.

---

## tdmelodic is operational — what it does and does not fix (2026-09-15)

The environment hurdles were real and are now scripted
(`scripts/setup_tdmelodic.sh`, with `--verify`):

- tdmelodic is **Chainer**-based (development ended 2019), so it needs an
  older Python: Python 3.9.25 fetched by **uv** into the workspace, with
  Chainer 7.8.1 built via `--no-build-isolation` (its setup.py needs
  pkg_resources from the environment).
- It reads accents from **UniDic kana-accent 2.1.2** (144 MB, licence
  GPL v2.0 / LGPL v2.1 / modified BSD — commercial use free, recorded in
  docs/model-licenses.md §6), which needs **MeCab** to build. MeCab 0.996
  comes from the Debian source tarball (taku910/mecab publishes no release
  archives) and UniDic is compiled into `.mecab/lib/mecab/dic/unidic`.
- Runtime needs `PATH=.mecab/bin` and
  `SETUPTOOLS_USE_DISTUTILS=stdlib` (otherwise setuptools' vendored
  distutils lacks `msvccompiler` and imports fail).
- The neural model is only **1.36 MB** and downloads on first use; the
  per-word mode `tdmelodic-s2ya` runs in about a second, so no hours-long
  dictionary generation is needed for experiments.

### Results on the cases we care about

Notation: tdmelodic marks the **high morae** in brackets (`accent_symbol`
in its source is `{0: "]", 1: "", 2: "["}`); 機械学習 → キ[カイガ]クシュー
means the high span is カイガ.

| input | tdmelodic | our frontend (reference) | listener-confirmed |
|---|---|---|---|
| 千二百 | `セ]ンニ[ヒャク]` (HLLHH) | `HLLHH` | `LLLHH` / `HHLHH` |
| 千五百 | `セ]ンゴ[ヒャク]` | `HLLHH` | — |
| 二千二百円 | `ニ[セ]ンニ[ヒャク]エン` | `LHLLHHLL` | `HHLLHHHL` |
| 千円 / 二千円 | `セ[ンエン` / `ニ[センエン` | — | — |
| ご指導 | `ゴ[シ]ドー` (LHLL) | `LHLL` | `LHLL` ✓ |
| ご注文 | `ゴ[チュ]ーモン` (accented) | accented | heiban (LHHHH) |

**So tdmelodic does not fix the numeral compounds** — it agrees with
OpenJTalk there (as predicted: UniDic stores accents per word and numerals
are compositional). For the polite-prefix class it is mixed: right for
ご指導, accented for ご注文 like the reference. The numeral cases stay with
our override table.

### What it does add: coverage

Readings agree with our frontend on this sample, and the accents are the
added value for modern/compound vocabulary:

| word | reading (ours = tdmelodic) | tdmelodic accent |
|---|---|---|
| 確率微分方程式 | カクリツビブンホーテーシキ | カ[クリツビブンホーテ]ーシキ |
| 電験一種 | デンケンイッシュ | デ[ンケンイ]ッシュ |
| 生成的人工知能 | セーセーテキジンコーチノー | セ[ーセーテキジンコーチ]ノー |
| 大規模言語モデル | ダイキボゲンゴモデル | ダ[イキボゲンゴモ]デル |
| マルクスアウレリウス | マルクスアウレリウス | マ]ルクスア[ウレリ]ウス |

### Integration architecture (the tractable shape)

Use tdmelodic **offline**, not at runtime and not over the whole NEologd
vocabulary (that run takes hours to days):

1. a word list (the consumer's domain vocabulary, or a common-word list)
   goes through `tdmelodic-s2ya`;
2. the tool converts the output into a **jpreprocess-format user-dictionary
   CSV** (16 columns, accent from tdmelodic, chain rule from the rules);
3. `dict_tools build --user jpreprocess` produces the user dictionary;
4. `JaFrontend::with_user_dictionary` loads it — the integration point
   verified above.

Caveat measured earlier and still true: an entry moves the realisation when
the word **heads its accent phrase**, so multi-word compounds may need
whole-phrase entries.

Next step: build that converter and compare our frontend against the
tdmelodic-filled dictionary on a word set, then let the listener judge a
sample.

### From model to dictionary: the converter and the disagreement scan (2026-09-15)

`scripts/tdmelodic_to_userdict.py` turns a word list into a
jpreprocess-format user-dictionary CSV, and the CLI gained `--user-dict`
(also on `eval_cer`; `Sbv2Adapter::load_dir_with_user_dictionary`) so the
dictionary is loadable in production, not only in the diagnostic example.

Design decisions, both forced by measurement:
- **Whole-word entries**, not per-token: UniDic's best path splits compounds
  and mis-reads the pieces (大規模言語モデル came out as 大=オー + 規模),
  while `s2ya` returns the whole reading and its accent.
- The accent comes from tdmelodic's marked reading (`]` = the mora where the
  pitch falls → accent position; no marker = plateau/heiban); multi-nucleus
  readings are skipped, because one dictionary entry is one accent phrase.

The converter produced 8 clean entries for the first test list (機械学習
4/7, 確率微分方程式 10/13, 音声合成 5/8, …), and **loading that dictionary
changed nothing**: our frontend already assigns those accents. So the
question became "where do the two actually disagree?", answered by a scan
over 39 modern/compound words:

- **31 agree, 8 disagree** — and every disagreement is a multi-word
  compound: 個人情報保護, 働き方改革, 地域活性化, 地球温暖化, 少子高齢化,
  感染症対策, 教師なし学習, 電験一種.
- That is the value proposition in concrete form: for simple and most
  modern vocabulary our frontend is already right; the compounds are where
  an external estimator can add something.

A blind A/B set for five of those words (個人情報保護, 働き方改革,
地球温暖化, 感染症対策, 教師なし学習) is at `listening/06-compound-accent-tdmelodic/`
(48 kHz, -20 LUFS, per-word randomised, key withheld) with a score sheet.
The judgement decides the next step: widen the generated dictionary, keep
the frontend as is, or choose per word.

Caveats recorded: Chainer on macOS warns that Accelerate can produce
incorrect results (the documented examples match exactly, but the warnings
are real); the per-word mode takes about a second, so experiments need no
full-NEologd generation; and an entry only moves the realisation when the
word heads its accent phrase.

---

## The listener's three compound intuitions match the standard rules (2026-09-15)

The listener gave target pronunciations for three words that both the current
frontend and tdmelodic had missed. Checked against the compound-accent
literature, all three are **the standard patterns**, not idiolect.

The rule set: NHK Broadcasting Culture Research Institute's revision notes
([NHKアクセント辞典"新辞典"への大改訂(3)](https://www.nhk.or.jp/bunken/research/kotoba/pdf/20170101_10.pdf))
classify compounds by the **mora count of the second element**:

| N2 length | pattern | prediction |
|---|---|---|
| ≤ 2 morae | 前部末型 (most common) | nucleus on **N1's last mora** |
| 3–4 morae | 後部一型 (most common) | nucleus on **N2's first mora** (when N2 is 頭高 or 平板/尾高; 窪薗・山本 1999) |
| ≥ 5 morae | 後部保存型 | N2 keeps **its own** accent; N1 is deaccented; some compounds go fully flat |

Component accents (UniDic and NAIST-jdic agree): 個人 1/3 · 情報 0/4 · 保護 1/2
· 地球 0/3 · 温暖 0/4 · 教師 1/3 · 学習 0/4.

| word | structure | N2 | rule | predicted | listener | current frontend | tdmelodic |
|---|---|---|---|---|---|---|---|
| 個人情報保護 | 個人情報 + 保護 | 2 | 前部末型 | 核 on ホ → `LHHHHHHHL` | **`LHHHHHHHL`** ✓ | `LHHHHHHLL` ✗ | heiban ✗ |
| 地球温暖化 | 地球 + 温暖化 | 5 | 後部保存型 (N2 heiban) | plateau → `LHHHHHHH` (8 morae) | **`LHHHHHHH`** ✓ | `LHHLHHHH` (splits phrases) ✗ | fall at 6 ✗ |
| 教師なし学習 | 教師なし + 学習 | 4 | 後部一型 | 核 on ガ → `LHHHHHLLL` (9 morae) | **`LHHHHHLLL`** ✓ | 核 9 ✗ | 核 6 ✓ |

So the listener's ear reproduces the standard rules exactly, and **neither
engine matches all three** — which is why the earlier A/B came out 2–2–1.
It also points at the generic fix: the three-way N2-length rule (plus the
窪薗・山本 subrules for 3–4 morae) is implementable as a rule layer, rather
than one override per word. The three patterns are registered as overrides
for now (annotated: 12, mismatches: 0), and the rule layer is the follow-up.

### The compound-accent rule layer (implemented, opt-in)

The three listener-confirmed words match the standard three-way classification,
so the rule itself was implemented rather than only its instances:
`src/compound.rs` applies NHK's N2-mora-count rule (≤2 → 前部末型, 3–4 →
後部一型 with the 窪薗・山本 subrule, ≥5 → 後部保存型), shifts a nucleus off a
special mora, and rewrites the compound's tones. `JaProcess::apply_compound_rules`
converts the phone stream to morae and back, so it composes with the override
table (which is applied afterwards and therefore still wins).

Measured footprint: it changes **10 of the 61 annotated words** — 生成的人工知能,
大規模言語モデル, 自然言語処理, 再生可能エネルギー, 地球温暖化, 感染症対策,
働き方改革, 少子高齢化, 地域活性化, 個人情報保護. Two observations from that:

- It matches tdmelodic exactly on 生成的人工知能 (`LHHHHHHHHHHLL`), and it
  reproduces the rule's shape on 地球温暖化 and 個人情報保護.
- It *degrades* 大規模言語モデル (baseline and tdmelodic both give
  `LHHHHHHHLL`; the rule gives `LHLHHHHHLL`), and it cannot touch
  教師なし学習 at all because なし is an 形容詞, so the noun+noun chain breaks.

Because an unvalidated deviation must not ship as the default, the rule is
**opt-in**: `--compound-rules` on the CLI and `eval_cer`, a new
`accent_report --compound-rules`, and `Sbv2Adapter::with_compound_rules`
(default off). The product configuration (deviations + the override table)
still reports **annotated: 12, mismatches: 0**.

Validation set: `listening/08-compound-rules/` pairs the rule's output against
the current one for all ten affected words (48 kHz, -20 LUFS, randomised,
key withheld). The judgement decides whether the rule becomes the default, a
per-word proposal, or is dropped.

---

## Consecutive /i/ vowels and the accent: the listener's question answered (2026-09-15)

The listener asked whether the phonology really changes when /i/ sequences
meet (生成的人工知能 has セイ + イ-like runs) and whether the pitch they heard
is what the literature predicts. It is, and the relevant sources are:

**[「母音が連続するときの発音」(日本語検定協会のコラム)](https://www.nihongokentei.jp/column/nakagawa-shuta/column-29.php)** surveys
every vowel sequence in standard Japanese against the 新明解 and NHK
dictionaries:

- **[ii]**: 「いい」 is accepted as either イイ or イー; sequences appear in
  押し板・かわいい, long vowels in おにいさん・おじいさん — so /i/+/i/ is
  realised either as two morae or as one long vowel.
- **[ei]**: only a few words keep the sequence (えい "ray", 稼いで); **most
  Sino-Japanese words take the long vowel** — 映画 = エーガ, 影響 = エーキョー
  — with エイ as the formal reading. Our frontend coalesces セイ to セエ
  (measured in the mora view), which follows that tendency.
- **Accent and coalescence interact**: for [oo], 多い was traditionally
  **オオ＼イ (low-high-low)** and that fall between the vowels is given as the
  reason a long vowel was hard to form; the newer, simpler **オ＼ーイ
  (high-low-low)** made オー easy. So a pitch fall inside the sequence blocks
  coalescence, and coalescence removes a mora that could bear the nucleus.

**[「日本語のアクセント」(tokyoaccent.com)](https://tokyoaccent.com/accent/accent.htm)** states the nucleus
rule directly: 「いわゆる特殊拍(引き音、撥音、促音)は核になることはない。…また**連母音の後半拍・無声化した拍も核になりにくい**」
— special morae cannot carry the nucleus, and the **second half of a vowel
sequence** and **devoiced morae** resist it too. The [TUFS module](https://www.coelang.tufs.ac.jp/mt/ja/pmod/practical/03-01-01.php)
gives the same rule for learners.

**[現代日本語の母音連続回避のためのわたり音挿入について](https://www.kci.go.kr/kciportal/ci/sereArticleSearch/ciSereArtiView.kci?sereArticleSearchBean.artiId=ART001581406)**
explains the avoidance strategies: glide insertion ([j]/[w]) applies only to
sequences whose sonority rises (/ia, ie, io, ea, ua, uo, ue, oa/); others
diphthongise — so /ii/ neither gains a glide nor becomes a diphthong, it
stays a sequence or a long vowel. And [long vowels behave as two-mora vowel
sequences](https://lemonschool.tokyo/japanesephonetics-phonology/longvowelssequences2/)
(they split into two notes in songs), so treating them as two morae is right.

**Two consequences for musculus**, both now recorded:

1. Our special-mora set covers ン, ッ and long vowels. The literature adds
   **vowel-sequence second halves and devoiced morae** as nucleus-resistant,
   so `shift_off_special` should be extended to those (a concrete follow-up;
   the current output for the three listener words is unaffected).
2. The listener's expectation for 生成的人工知能 is `LHHHHHHHHHHLL`-shaped
   (the fall after チ, before ノウ) and the compound rule already produces
   exactly that, which is why they judged that pair 同等 with a positive
   lean.

## Compound-rule verdict: not adopted as the default (2026-09-15)

`listening/08-compound-rules/` was scored: **rules 0 wins, current 2 wins,
8 ties** (mean −1.50). The pre-registered criterion said a current-side win
drops the rule, so it stays opt-in and the default remains the reference
fidelity plus the validated prefix deviation plus the override table.

The listener's notes were more useful than the tally: for many words their
expected pattern differs from *both* systems (phrase splits, different
nuclei), and those expectations are now registered in the override table and
declared in `ja_accent.jsonl` (自然言語処理 `LHHHHHHL`, 働き方改革
`LHHHHHHLLL`, 少子高齢化 and 地域活性化 `HLLLHHHH`). The report gate is at
**annotated: 16, mismatches: 0**.

## Diagnostics that closed two hypotheses (2026-09-15)

Two literature-backed hypotheses about the accent layer were measured against
the 61 annotated words, using pyopenjtalk's labels (the reference, which our
frontend matches):

1. **Nucleus on a devoiced mora** (「無声化した拍も核になりにくい」): **0 words**.
   Note that OpenJTalk applies its devoicing step *after* the accent steps, so
   the ordering makes this possible in principle — it simply does not happen
   in this sample.
2. **Nucleus on the second half of a vowel sequence or long vowel** (「連母音の
   後半拍も核になりにくい」): **0 words**.

So the frontend already respects both halves of the 核の移動 rule that our
own `shift_off_special` implements only for ン/ッ/long vowels. Extending it
would be a no-op on this evidence, and the hypothesis is closed rather than
implemented.

Also checked and closed: the devoiced-vowel *phone* representation. Our g2p
lowercases the label's uppercase vowel just as the reference does
(`sbv2_core/src/jtalk.rs:558`), so dropping the case marker is faithful, not a
port bug.

## What the listener's remaining observations actually are (2026-09-15)

Several notes describe gradual pitch rather than a discrete nucleus — 「オンダンカ
がキュウよりもややトーンが低くなる」, 「やや右下がり」,
「クはサよりもトーンが落ちる」. These are **declination / M-level**
phenomena, and the accent layer's H/L feature is binary: it cannot express
them, so no amount of listening on those words can turn them into a
registerable pattern. They are recorded as a **feature limitation**: handling
them would need pitch-contour control (F0 modelling downstream of the accent
layer), not an accent-value change. Two other recurring notes — 「音核が分かれて
いる」(働き方改革, 少子高齢化, 地域活性化) — point at **accent-phrase grouping**
rather than nucleus placement; that is a different mechanism (the NJD chain
flags / phrase step) and is parked.
