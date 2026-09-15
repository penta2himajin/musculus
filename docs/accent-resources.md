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
`ab-test-accent/`):

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
data. Six candidates now sit in `ab-test-accent/`, paired by initial mora:

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
was prepared: `ab-test-accent-blind/` holds the same six takes under neutral
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
reference take is at `ab-test-accent/shinshuu-registered-F.wav` (48 kHz,
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
地球温暖化, 感染症対策, 教師なし学習) is at `ab-test-accent/tdmelodic/`
(48 kHz, -20 LUFS, per-word randomised, key withheld) with a score sheet.
The judgement decides the next step: widen the generated dictionary, keep
the frontend as is, or choose per word.

Caveats recorded: Chainer on macOS warns that Accelerate can produce
incorrect results (the documented examples match exactly, but the warnings
are real); the per-word mode takes about a second, so experiments need no
full-NEologd generation; and an entry only moves the realisation when the
word heads its accent phrase.
