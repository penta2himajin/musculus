# ADR-0001: euhadra を鏡像とする trait パイプライン構造

- ステータス: 採用
- 日付: M0
- 関連: docs/spec.md §3

## 背景と決定

musculus は penta2himajin/euhadra(音声入力フレームワーク)の TTS 側の対として設計される。euhadra の本質 — 「各段階が Rust trait である構成可能パイプライン」「ライブラリが本体で OS 統合は消費側の仕事」「LLM を要らない層が主戦力」「測定文化」— を方向反転して継承する。

```
euhadra:  音声 → [VAD] → AsrAdapter → TextFilter → TextProcessor → [LlmRefiner] → 出力(テキスト)
musculus: テキスト → SpeechNormalizer → TextProcessor → TtsAdapter → AudioEmitter(音声)
```

## 採用する構造

- **trait 面**(`SpeechNormalizer` / `TextProcessor` / `TtsAdapter` / `AudioEmitter`)は安定の対象。euhadra の `AsrAdapter` 系 trait が「実装することがこの crate に依存する理由」であるのと同じ位置づけにする
- エラー型は `#[non_exhaustive]` で、呼び出し側が行動を分けられる区別(ModelLoad / Config / NoText / Inference / Cancelled)を保持する(euhadra `AsrError` の鏡像)
- `0.x` の間、trait を囲むもの(builder、具体実装、評価ハーネス)は流動的。minor version で壊れ得る

## 結果

- 正: euhadra の資産(評価ハーネスの考え方、ASR を「物差し」として再利用する構図、ドキュメント流儀)がそのまま引き継げる
- 正: Irodori 等の比較実装が trait の差し替えで入る(M4 の前提)
- 負: euhadra と 1:1 の型・名前を意識しすぎると TTS 固有の要件(スタイル条件付け、文分割)を歪める可能性がある。TTS 固有の判断は euhadra との対称性より用途を優先する

## 意思決定

この対称性は設計の出発点であり、目的ではない。TTS で正しくない構造は、対称性があっても採用しない。