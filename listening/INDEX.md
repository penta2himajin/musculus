# Listening archive — index

Every set of audio comparisons musculus has produced, one directory per
question, numbered in the order the questions came up. Each set has its own
`README.md` (protocol and what to listen for) and `score-sheet.md` (the
scores, with the reveal appended once scored).

`key.ndjson` maps the randomised A/B labels to the systems under test. Keys
are **gitignored**: they are opened only after scoring, and the reveal is
recorded in that set's score sheet.

Audio files are gitignored too (`*.wav`): the repository keeps the recipes
and the results, not the sound. Regenerate any set with its generator script
(see the set's README) or the command recorded in the relevant doc.

| # | Set | Question | Result |
|---|---|---|---|
| 01 | [01-engine-vs-engine](01-engine-vs-engine/README.md) | Irodori vs SBV2JE, same voice and text | **Irodori 5/5, mean CMOS +2.75** → dual-engine posture (ADR-0005) |
| 02 | [02-irodori-steps](02-irodori-steps/README.md) | Irodori 5 vs 40 Euler steps (same engine, same seed) | **40 steps 5/5, mean −2.80** → low-step mode rejected |
| 03 | [03-sbv2-whole-vs-split](03-sbv2-whole-vs-split/README.md) | SBV2 one pass vs per-sentence split + 0.4 s silence | **split 3–2, mean exactly 0.00** → always-split rejected |
| 04 | [04-sbv2-whole-vs-grouping](04-sbv2-whole-vs-grouping/README.md) | SBV2 one pass vs length-aware grouping (40 chars) | **group 1 win 2 ties 0 losses**; controls ±1 = noise floor → not yet the default |
| 05 | [05-shinshuu-variants](05-shinshuu-variants/README.md) | 先週と比べると: six accent shapes (initial セ low/high × 比べると shape) | **F registered** (セH + continuous high + final ト low); the blind round overturned an earlier explicit choice |
| 06 | [06-compound-accent-tdmelodic](06-compound-accent-tdmelodic/README.md) | Compound-word accent: tdmelodic-generated dictionary vs the current frontend | **2–2–1, no clear winner** → frontend stays the default; tdmelodic kept as a per-word option |
| 07 | [07-blind-shinshuu](07-blind-shinshuu/README.md) | Blind repeat of set 05 for third-party listeners (A–F, no hints) | Judged C-or-F locally (both high-initial セ); acquaintance sheets were not collected — the listener evaluated alone |
| 08 | [08-compound-rules](08-compound-rules/README.md) | Compound-accent rules (NHK's N2 mora-count classification) against the current output, 10 affected words | **awaiting the listener's judgement** (rule layer is opt-in until then) |
| 09 | [09-sdp-ratio](09-sdp-ratio/README.md) | SBV2 `sdp_ratio`: deterministic duration (0.0, our default) vs stochastic mixing (0.2, upstream default) | **2–2–1, no winner** → default stays 0.0; `--sdp-ratio` kept |
| — | [scratch/](scratch/) | Loose outputs from earlier steps (candidate variants, product examples) | Not scored; kept for reference |

## Reading the archive

- Sets 01–04 compare **engines or segmentation**; sets 05–07 compare
  **accent (prosody)**.
- Every set matched loudness before presentation (per-pair verification, see
  each README); set 04 also carries deliberate **noise-only controls**,
  which is how we know its resolution is about ±1 CMOS.
- The durable conclusions live in `docs/benchmarks/listening-log.md`,
  `docs/decisions/`, and `docs/accent-resources.md`; this index is the map.
