# Accent and prosody resources for later use

Reference inventory for the accent workstream (ADR-0006). Everything here
is recorded so a later session can pick it up without re-surveying the
field. **musculus bundles none of it** — dictionaries and corpora are
fetched by a setup script or supplied by the consumer.

## What we can use

| Resource | Licence | How it would be used | Status |
|---|---|---|---|
| [tdmelodic](https://github.com/PKSHATechnology-Research/tdmelodic) — Japanese accent dictionary generator (PKSHATechnology-Research) | **BSD-3-Clause** | Generates a large-vocabulary accent dictionary from UniDic + NEologd with a neural estimator ([Tachibana & Katayama, ICASSP 2020](https://doi.org/10.1109/ICASSP40776.2020.9054081)). This is the intended source of the standard-accent layer: emit word entries (surface, reading, accent type, mora count) and feed them to the frontend so the *labels* are right | not started; needs Python/torch and a dictionary build |
| [tdmelodic_openjtalk](https://github.com/sarulab-speech/tdmelodic_openjtalk) | **unverified** | Turns tdmelodic output into an OpenJTalk dictionary — the closest fit for jpreprocess, which is OpenJTalk-derived | check the licence first, then whether jpreprocess accepts an externally built dictionary |
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

## Next actions in order

1. Override layer is implemented (`src/accent.rs`, `--accent` on the CLI,
   `eval_cer` and `accent_report`). Extend the table as more forms are
   confirmed by the listener.
2. Verify `tdmelodic_openjtalk`'s licence and whether jpreprocess can load
   an externally built dictionary (`jpreprocess-dictionary`'s build tools).
3. If that path works: generate the standard-accent dictionary in a setup
   script (documented as a Python requirement), keep it out of the repo,
   and compare its output against the current frontend with the accent
   report plus listening.
4. Build the L3 accent gold from koniwa + our own annotations and gate it
   in CI, with the "at most one nucleus per accentual phrase" invariant as
   a separate test.
