# ADR-0002: ja ベースラインは SBV2JE、比較候補は Irodori-TTS

- ステータス: 採用
- 日付: M0
- 関連: docs/spec.md §6–§7, docs/evaluation.md

## 背景と決定

最初の `TtsAdapter` 実装を **Style-Bert-VITS2 JP-Extra(SBV2JE)** とする。SBV2JE ベースラインが動いた後に **Irodori-TTS** を比較アダプタとして実装し、最終決定はユーザ自身の盲検聴取(CMOS 相当)で行う。多言語系(en 等)は ja の設計が決着した後(M5)に着手する。

## 根拠(測定ベース)

1. **SBV2JE は ja オープン TTS で唯一、人間評価による検証が存在する水準**。Rackauckas & Hirschberg 2025(arXiv:2505.17320)でネイティブ評価者 11 名の MOS が人間録音と統計的同等(4.37±0.74 vs 4.38±0.77、p≈0.91)
2. **多言語 LLM 型 TTS は ja で弱い**。CosyVoice 2 の ja 実測は CER 18.79% / NMOS 3.42(arXiv:2412.10117、ko は CER 7.98%)。ja-first 方針の下、ja 特化モデルが物差しに適する
3. **実装可能性が証明済み**。sbv2_core(neodyland/sbv2-api)が Rust + ort での推論を実績として持ち、jpreprocess で C 依存なしの音素化が可能
4. VOICEVOX はエコシステムで普及しているが学術的な MOS 証拠を持たない。第一の物差しには使えない(第二アダプタ候補として将来再評価)

## Irodori-TTS を比較候補とする理由

- 重み MIT・ja 特化・ゼロショット音声クローン/キャプション設計/絵文字スタイル制御で、euhadra と組む双方向音声への長期ポテンシャルが最大
- ONNX 化の先行例(ngc-shj/irodori-tts-webgpu、MIT)があり「証明済み経路の Rust 移植」として実装できる
- ただし **MOS 評価は未発表・CPU RTF 未知数**。ゆえに直決せず、ベースライン確立後の測定 + 聴取で決める

## 決定プロトコル(M4)

docs/evaluation.md §5 の自分用 CMOS プロトコル(盲検・同テキスト・-3〜+3)+ 客観裏付け(round-trip CER / UTMOS / RTF)。結果を `docs/benchmarks/` に記録し、決定を本 ADR の追記または新 ADR で記す。

## 意思決定

「人間評価で検証済みのものを物差しにし、未検証のものは測ってから採否する」。これは euhadra がモデル選定で ΔWER を使ったのと同じ原則の TTS 版である。