# musculus

## Overview

musculus は euhadra(penta2himajin/euhadra、音声入力フレームワーク)の TTS 側の対として設計される、Rust 製の構成可能なテキスト→音声フレームワーク。各段階が Rust trait(`SpeechNormalizer` / `TextProcessor` / `TtsAdapter` / `AudioEmitter`)であり、ローカルファースト(ONNX Runtime 上でネイティブ合成、重みは同梱しない)、日本語ファースト。命名は mouth → mouse → *musculus*(耳の euhadra に対する口)。設計は @docs/spec.md、評価方針は @docs/evaluation.md、意思決定は @docs/decisions/ に記録する。

## Project Structure

```
src/              # ライブラリ本体: types.rs(ドメイン型)/ traits.rs(trait 面)/ mock.rs([testing])
src/main.rs       # CLI エントリ([cli] feature のみビルド)
docs/
  spec.md             # 技術仕様(アーキテクチャ、エンジン決定、マイルストーン)
  evaluation.md       # 評価方針(L1 CI / L2 リリース / L3 正規化 F1)
  benchmarks/         # 評価ランナーが書き出す実測 JSON(生成物)
  decisions/          # ADR
tests/            # 統合テストと評価アノテーション(tests/evaluation/)
scripts/          # モデル取得等の setup スクリプト(M1 以降)
```

## Development Setup

- Rust: MSRV 1.78(デフォルト feature)。`onnx` feature は依存が 1.88 を要求する(ort 2.0.0-rc.13)

```bash
# Pre-push hook (format / lint / clippy).
git config core.hooksPath git-hooks
```

## Build & Test

```bash
cargo build --workspace
cargo test  --workspace
# ONNX 合成アダプタ(M1 以降)
cargo build --features onnx
```

## Development Principles

- **測定文化の継承**: 性能・品質の主張には計測を伴わせる。ベンチマーク結果は `docs/benchmarks/` に JSON でコミットし、回帰判定は「相対(baseline 比)+ 絶対(hard floor)」の 2 軸(euhadra 流儀)
- **LLM を要らない層が主戦力**: 数値・記号・日付の読み展開はルール + CI で測れる ground truth(tests/evaluation/annotations/)で実装する
- **重みと辞書はユーザ所有**: musculus は挙動を持ち、モデル重み・ユーザ辞書は同梱しない(setup スクリプトで取得、辞書は消費アプリが入力する)
- trait 面の変更は docs/decisions/ に ADR を書く。trait 面は安定対象、その周辺(builder、具体実装、評価ハーネス)は 0.x で流動的

## Architectural Boundaries

- default build は ML ランタイム・システムライブラリ非依存(純 Rust)。`ort` は `onnx` feature の後ろのみ、`cpal` は `playback` feature の後ろのみ
- ONNX ランタイムは `ort` 2.0.0-rc.13 + ndarray 0.17 に固定。別バージョンの ort/ndarray を依存に入れると二重化する(ADR-0003)
- `docs/benchmarks/` の JSON は評価ランナーの生成物であり、手編集は「意図的 baseline 更新(根拠をコミットメッセージに書く)」のみ
- round-trip CER の物差し ASR はバージョン込みで baseline JSON に記録する。物差し変更は baseline 更新と同じ PR で行う

## Prohibitions

1. モデル重み・辞書データをリポジトリに commit しない(setup スクリプト経由のみ)
2. default feature に `ort` / `cpal` / ML ランタイム依存を足さない
3. `docs/benchmarks/` の実測値を根拠なしに手で書き換えない
4. 物差し ASR のバージョンを baseline 記録と別 PR で変更しない
5. L3 アノテーションの gold(意図した読み)を、実装を通すために書き換えない(gold の変更は別 PR で理由を明記)

## Git Conventions

共通ルールに従う。プロジェクト固有の追加はなし。

## Session Handoff

Long-running workstreams use GitHub issues for cross-session continuity. See `docs/handoff-protocol.md` for the full protocol.

- Label: `session-handoff`
- One issue per workstream (not per session)
- On session start, read the relevant handoff issue and confirm the **Next action** with the user before executing.

## Internationalisation

If this project ships a Japanese-facing entry point, follow `docs/i18n-policy.md`:

- Translations are suffix files (`README.ja.md` next to `README.md`); no language directories.
- Only `README.md` and the user-facing introduction tier of `docs/` are in scope. Engineering docs and ADRs stay English-only.
- Each translated file carries a `> Source: <name>.md @ <sha>` header. PRs are never blocked on translation parity.

---

<!-- Common rules below this line apply to every project. -->

## Common Development Rules

### TDD (Red → Green → Refactor)

All implementation work proceeds in this cycle:

1. **Red**: write a failing test that captures the intended behaviour.
2. **Green**: write the minimum code that makes the test pass.
3. **Refactor**: tidy up while keeping tests green.

When a test fails, fix the production code — do not delete, skip, or weaken the test.

### Measure, Don't Conjecture

Base decisions on observed data, not assumptions. Before optimising, claiming a bottleneck, or asserting that something is slow or broken, measure it — profile, benchmark, log, or reproduce. When you report a cause, cite the measurement that supports it.

### Git Conventions

- **Conventional Commits**: `feat:` `fix:` `docs:` `refactor:` `test:` `ci:` `chore:`. Project-specific prefixes (e.g. `data:`, `experiments:`) live in the project's `AGENTS.md`.
- **Branch naming**: use a short prefix for the agent or author followed by a topic, e.g. `claude/<topic>`, `codex/<topic>`, or `human/<topic>`.
- **Trailer**: when an AI agent authors the commit, append a trailer crediting the agent. Do not embed model name or session info in the trailer; put those in the commit body if needed.
- **Pre-push hook**: install via `cp git-hooks/pre-push .git/hooks/pre-push && chmod +x .git/hooks/pre-push` (or `git config core.hooksPath git-hooks`). The hook runs format / lint / clippy before every push. Tests are intentionally omitted — TDD keeps them green at commit time.

### Pull Requests

- **Always ready for review.** Open PRs in the "ready" state, never as drafts. Draft PRs do not fire review-requested events and slow the loop.
- **Auto-subscribe after creating a PR.** Immediately after the PR is created, subscribe to its activity without asking the user. Rationale: the user explicitly opted into the "agent opens and watches its own PRs" workflow at the template level, so the per-PR confirmation is noise. Unsubscribe only when the user says to stop, when the PR merges, or when it is closed unmerged.
- **One PR per workstream**, matching the handoff issue. Reference the issue with `Closes #N` per `.github/PULL_REQUEST_TEMPLATE.md`.

### Stream Idle Timeout Mitigation

Cloud agent sessions occasionally fail with `Stream idle timeout - partial response received` on long output. To reduce risk:

1. **Stage long writes.** For long documents or source files, write the skeleton (headings, function signatures, trait stubs) first, then fill each section in follow-up edits. Avoid single blocks larger than ~200 lines.
2. **Watch out after large reads.** Reading a big file (e.g. `Cargo.lock`, large generated modules) and then immediately producing long output is a common trigger. Split into separate turns or excerpt only the relevant portion.
3. **Recover carefully.** A timeout can still leave the file write completed. Run `git status` before retrying so the same content is not written twice.

### Common Prohibitions

1. Do not delete, skip, or comment out existing tests.
2. Do not modify CI configuration without explicit instruction.
3. Do not weaken production code merely to make tests pass.
4. Do not commit credentials, API keys, signed URLs, or anything in `.env*`.
