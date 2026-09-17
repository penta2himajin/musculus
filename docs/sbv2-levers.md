# SBV2 levers: documented usage, our defaults, and what is actually left

Reference notes for the Style-Bert-VITS2 JP-Extra path (docs/spec.md). The
authority is Style-Bert-VITS2's own inference API
([`style_bert_vits2/tts_model.py`](https://github.com/litagin02/Style-Bert-VITS2/blob/master/style_bert_vits2/tts_model.py),
[`constants.py`](https://github.com/slp-hu/Style-Bert-VITS2/blob/layer-b-cadence-seq/style_bert_vits2/constants.py))
and the ONNX export script
([`convert_onnx.py`](https://github.com/slp-hu/Style-Bert-VITS2/blob/layer-b-cadence-seq/convert_onnx.py)).
Our port mirrors `sbv2_core`, whose defaults differ from upstream's — that
difference is the interesting part.

## Inputs the exported model accepts

From the official JP-Extra export: `x_tst`, `x_tst_lengths`, `sid`, `tones`,
`language`, `bert` (the Japanese BERT features — the JP-Extra architecture
zeroes the Chinese/English ones), `style_vec`, `length_scale`, `sdp_ratio`,
`noise_scale`, `noise_scale_w`. Our `synthesize_vits2` feeds a knob only when
the session declares that input, so a conversion that bakes a value into the
graph stays correct.

## The knobs

| knob | semantics (upstream docs) | upstream default | ours |
|---|---|---|---|
| `style` + `style_weight` | row of `style_vectors`; weight interpolates `mean + (style - mean) * w`; >1 can collapse the voice | `Neutral`, w = 1.0 | id 0, w = 1.0 |
| `sdp_ratio` | DP/SDP mixture: 0 = deterministic duration only, 1 = stochastic only; **higher adds tempo variation (緩急)** | **0.2** | **0.0** |
| `noise_scale` | noise for the deterministic duration predictor | **0.6** | **0.677** |
| `noise_scale_w` | noise for the stochastic duration predictor | 0.8 | 0.8 |
| `length_scale` | speech length/rate; larger is slower | 1.0 | 1.0 |
| `line_split` + `split_interval` | split on **newlines** and insert silence between parts (upstream notes that splitting carries emotion better) | True, 0.5 s | one pass; `--split-sentences`/`--max-chars` opt-in (measured in listening sets 03/04) |
| `given_phone` + `given_tone` | explicit phone and tone sequence — the official accent-control API | — | our accent override table / user dictionary cover the same need |
| `pitch_scale`, `intonation_scale` | post-processing of pitch and intonation range (quality degrades away from 1.0) | 1.0 | not exposed |
| `assist_text` | emotion reference text | off | **not applicable**: JP-Extra consumes only the Japanese BERT features |

`style_weight` interpolation is implemented exactly as the reference
(`mean + (style - mean) * weight`, `src/sbv2/bundle.rs`), so the lever is
ready — but see the model note below.

## The style lever is a no-op for the voice we ship

`vendor/sbv2/tsukuyomi.sbv2` carries `style_vectors.json` with
**`shape: [1, 256]` — a single style row**. Row 0 is the mean, and the blend
formula with `style == mean` returns the mean for every weight, so
`--style`/`--style-weight` cannot change this voice. A multi-style model
(upstream examples such as あみたろ's have `Neutral` + several numbered
styles, where the names come from the model directory's `style2id.json`,
which our `.sbv2` bundle does not carry) would make the lever meaningful, and
adding such a voice needs no code change.

## What is actually left to audition: `sdp_ratio` (and `noise_scale`)

Measured on the long narrative, three runs per setting:

| `sdp_ratio` | audio duration across runs |
|---|---|
| 0.0 (ours) | 10.04 s, 10.04 s, 10.04 s — **deterministic** |
| 0.2 (upstream) | 9.68 s, 9.73 s, 9.76 s — **variable** |

That matches the documented semantics exactly (0 = deterministic duration
only) and shows the lever is real rather than cosmetic. `noise_scale` is the
second candidate: 0.6 upstream against our 0.677 (the Rust reference's
value).

The audition set is `listening/09-sdp-ratio/` (5 pairs, 48 kHz, -20 LUFS
verified, per-pair randomised, key withheld). `--sdp-ratio`, `--noise-scale`,
`--noise-scale-w` and `--length-scale` are now CLI flags, with the reference
defaults, so whichever setting wins can be made the default and gated.

## sdp_ratio verdict: no winner, default unchanged (2026-09-15)

`listening/09-sdp-ratio/` was scored: **0.2 won 2, 0.0 won 2, one tie**
(mean +0.25). The difference is at the listener's resolution and does not
correlate consistently with text length (0.2 was preferred on the short and
medium pairs, 0.0 on the long narrative and the last pair; the 0.0 side was
described as 途切れ途切れ in two pairs and the 0.2 side as worse in two
others). So the default stays **0.0**, the Rust reference's value, and
`--sdp-ratio` remains available for per-use experiments.

The measurement behind it is solid even though the preference is not: 0.0 is
deterministic in duration (10.04 s three times) and 0.2 varies
(9.68/9.73/9.76 s).

An accent observation from the same round turned into a fix: the listener
heard 昇る as non-heiban inside 「最も高く昇る夜」 while it is correct alone
(`LHH`, and the dictionary has 昇る = 0/3 heiban). That is an accent-joining
problem in context, so `ノボル -> LHH` is registered in the override table and
the phrase is declared in `ja_accent.jsonl` (the ノボル span is
listener-confirmed; the surrounding morae are the frontend's own pattern,
pinned only to catch drift).

`noise_scale` (0.677 vs upstream 0.6) remains the one unmeasured lever.
