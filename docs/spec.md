# musculus 技術仕様

ステータス: M0 ドラフト(設計は流動的。`0.x` の間、特に trait 面は実装との対話で変わる)
最終更新: M0 作成時

## 1. 概要と命名

musculus は **euhadra**(penta2himajin/euhadra)の鏡像として、**テキストを音声へ**変換する構成可能なパイプラインフレームワークを Rust で作る。

```
euhadra:  音声 → [VAD] → AsrAdapter → TextFilter → TextProcessor → [LlmRefiner] → 出力(テキスト)
musculus: テキスト → SpeechNormalizer → TextProcessor → TtsAdapter → AudioEmitter(音声)
```

命名は euhadra の "ear → cochlea → snail → *Euhadra*" と対をなす:

> mouth → mouse → **musculus**(ハツカネズミ *Mus musculus*。musculus はラテン語で「筋肉」を意味し、マウスの小さな筋肉の隆起に似ていることから)

耳に始まるものに、口に始まるものを対にする。euhadra が聞くライブラリなら、musculus は喋るライブラリである。

## 2. 目標と非目標

### 目標

1. **ライブラリが本体**。各段階を Rust trait として提供し、任意の段階を差し替え可能にする。CLI は `cli` feature の opt-in バイナリ(euhadra 同型)。
2. **ローカルファースト**。合成エンジンは ONNX Runtime(`ort`)上でネイティブ実装する。クラウド API はアダプタとして書けるが、何も同梱しない。
3. **日本語ファースト**。正規化(読み展開)は言語ごとの作業であり、日本語の ground truth と測定文化から始める。多言語は ja の設計が検証された後(M5)。
4. **LLM を要らない層が主戦力**。数値・記号・日付の読み展開やユーザ辞書はルールで実装し、CI で測れる ground truth を持つ(euhadra Tier 1/2 と同じ立場)。
5. **測定文化の継承**。round-trip CER、RTF、proxy MOS をコミット済み JSON として追跡する(詳細は `docs/evaluation.md`)。

### 非目標(0.x の間)

- ストリーミング逐次合成(M4 まで設計に含めない。euhadra が streaming ASR を測定の上断念した判断の対称として、バッチを先に正しくする)
- 音声モデル重みの同梱・再配布(setup スクリプトでユーザが取得する)
- ユーザ辞書の同梱。musculus は挙動を持ち、辞書はユーザが所有する(euhadra の TermDictionary と同じ哲学)
- 独自の音声フォーマット/エフェクトスタック

## 3. アーキテクチャ

各段階が trait。デフォルトビルドはパイプライン実行時 + ルールベース正規化のみで、ML ランタイムもシステムライブラリも依存しない。

```
テキスト入力
    → SpeechNormalizer   (読み展開: 数値・記号・日付・漢字読み)     [Tier 1]
    → TextProcessor      (ユーザ辞書・表記揺れ)                   [Tier 2]
    → TtsAdapter         (ローカル合成エンジン)                    [onnx feature]
    → AudioEmitter       (再生 / WAV / stdout)                    [playback feature]
```

| trait | 対応する euhadra trait | 実装(M0/M1 時点) |
|---|---|---|
| `SpeechNormalizer` | `InverseTextNormalizer`(の逆方向) | M3 完了(2026-09-13): `JaNormalizer`(日付・記号・時刻・マイナスの単一スキャン正規化)+ ユーザ辞書(`TermDictionary`) |
| `TextProcessor` | `TermDictionary` 等 | M0: trait のみ。M3: 辞書 |
| `TtsAdapter` | `AsrAdapter` | M0: trait + mock。M1: SBV2JE(ONNX) |
| `AudioEmitter` | `OutputEmitter` | M0: trait + mock。後続: WAV / cpal 再生 |

エラー型は `AsrError` の鏡像として `#[non_exhaustive]` で設計する(M0 コード参照)。

### 将来の拡張(設計に含めない、含意だけ記録)

- **文分割(Segmentator)**: euhadra の VAD+Segmenter の鏡像。「どこで文を切って逐次合成するか」はストリーミング設計時に必要になる。バッチ版ではパイプライン外の補助関数で足りる
- **スタイル/キャプション条件付け**: SBV2 のスタイルベクトル、Irodori の caption/参照音声。`SpeechSegment` は最小フィールドから始め、実装が正しい形を示すまで拡張しない

## 4. Domain types

- `AudioChunk` — euhadra の概念と同じ:音声サンプル(`f32`)とサンプルレート。チャンク分割は入力側の都合を運ぶ
- `SpeechSegment` — 合成対象の最小単位。`text` + 任意の `voice`(0.x では String id)。スタイル/キャプションは後続バージョンで移動し得る(euhadra の `Command`/`StructuredInput` が Phase 2 で移動するのと同じ注意書き)
- `Synthesis` — アダプタの出力:`Vec<AudioChunk>` と導出(`duration()`)
- `Correction` — 正規化層の差分報告(euhadra の TermDictionary と同じ span 付き報告)
- エラー: `TtsError`(ModelLoad / Config / NoText / Inference / Cancelled / Unsupported)、`EmitError`、`NormalizerError`

## 5. Feature 構成

| Feature | 追加するもの | コスト |
|---|---|---|
| *(default)* | パイプライン実行時 + ルールベース正規化トレイト群 | 純 Rust |
| `onnx` | ONNX 合成アダプタ(SBV2JE、後に Irodori) | `ort` rc.13、Rust 1.88 |
| `playback` | スピーカ再生(`cpal`) | Linux は ALSA headers |
| `cli` | `musculus` バイナリ | Rust 1.85 |
| `testing` | mock アダプタとテスト用ツール | `[dev-dependencies]` で使う |

依存ポリシー: euhadra と同じく、ライブラリ消費者が不要なシステムパッケージ(ALSA 等)や ML ランタイムの依存を継承させない。ランタイムは **`ort` 2.0.0-rc.13 + ndarray 0.17** に固定する(euhadra と同一バージョン。根拠は [ADR-0003](decisions/0003-ort-native-implementation.md))。

## 6. 日本語ベースライン: Style-Bert-VITS2 JP-Extra(SBV2JE)

**ステータス: M1 完了(2026-09-13)。** `Sbv2Adapter`(ort rc.13 直叩き)が `vendor/sbv2` の tsukuyomi JP-Extra セットで「こんにちは」を合成する。RTF 実測は `docs/benchmarks/sbv2/baseline.json`(M1 Max, release, RTF p50 0.199)。CLI: `cargo run --features cli,onnx -- synth "こんにちは"`。残る確認事項は `docs/model-licenses.md` §4。

最初の `TtsAdapter` 実装。意思決定の記録は [ADR-0002](decisions/0002-ja-baseline-sbv2-je.md) にある。

**選定の根拠(測定)**:

- Rackauckas & Hirschberg 2025(arXiv:2505.17320)が SBV2JE をネイティブ評価者 11 名の MOS で測定し、人間録音と統計的同等(4.37±0.74 vs 4.38±0.77、p≈0.91)と報告。ja オープン TTS で人間評価による検証が存在するのはこれがほぼ唯一
- 多言語 LLM 型 TTS(CosyVoice 2 など)は ja で明確に弱い(CER 18.79%、NMOS 3.42、arXiv:2412.10117)
- Rust での実行可能性は sbv2_core(neodyland/sbv2-api)が証明済み。jpreprocess(OpenJTalk の純 Rust 再実装)で C 依存なしに音素化できる

**実装経路**:

1. `TtsAdapter` を ort rc.13 直叩きで自前実装。sbv2_core は**依存にしない**(ort rc.9 / ndarray 0.16 との二重化を避けるため)。参照実装として読み、MIT 等のライセンス確認の上で移植する
2. モデルは ONNX 化済みセット(googlefan/sbv2_onnx_models:`tokenizer.json` + `debert.onnx` + `.sbv2` 形式)を setup スクリプトで取得。**重みは同梱しない**
3. `scripts/setup_sbv2.sh`(euhadra の `scripts/setup_*.sh` と同じ idempotent 流儀)

**M1 実装前に確認する事項**:

- JP-Extra モデルセットの正確な構成(`.sbv2` 形式の中身:bert / style vector estimator / decode など)
- litagin02 系モデル(tsukuyomi / amitaro / himari など)の**声ごとの利用規約** → `docs/model-licenses.md` として euhadra と同様に整備する(→ **調査済み**: sbv2_core は MIT、litagin02/Style-Bert-VITS2 本体と JP-Extra ベース重みは AGPL-3.0、声の規約は tsukuyomi=クレジット必須・商用可、amitaro=クレジット必須・敏感用途禁止。残る未確認は model-licenses.md §4)
- sbv2_core リポジトリのライセンス(移植の法的根拠)→ **MIT 確認済み**(crates.io 全バージョン)。依存にはせず参照として読む(ADR-0003)
- CoreML EP を M1 で入れるか(RTF に直結。まず CPU で測り、必要なら)

## 7. 比較実装: Irodori-TTS(M4)

SBV2JE ベースラインが動いた後に、比較アダプタとして実装する。意思決定は**ユーザ自身の聴取(MOS 相当)**で行う。

**プロトコル(自分用 CMOS)**:

1. 同じテキスト 10〜20 文、両アダプタで合成。可能なら同じ声(音声クローン参照を使って声の好みと合成品質を分離する)
2. 順序をランダム化し、どちらがどちらか分からない状態で聴く(盲検)
3. -3〜+3(0.5 刻み)の CMOS 式採点。結果を `docs/benchmarks/` に記録し、ADR に決定を書く
4. 客観裏付けとして round-trip CER・UTMOS・RTF を併記する

**Irodori 側の技術的注意**:

- diffusion 系(RF-DiT)なので **CPU RTF が未知数** — M4 の最初の計測項目
- ONNX エクスポートは v3(500M)が先行例(ngc-shj/irodori-tts-webgpu、MIT)。v4.1-Small を使うなら自前エクスポート作業が入る
- 構成要素は 4 セッション(text encoder / duration / DiT / DACVAE decoder)。seed で出力が変わるため N seed の mean±σ を報告する

## 8. 評価計画

方針の詳細は [docs/evaluation.md](evaluation.md) に集約する。骨子:

- **L1(CI)**: round-trip CER(euhadra の ja ASR を物差しに固定)+ RTF + smoke
- **L2(リリース時)**: UTMOS(参考値、英語学習の注記付き)/ SpeechBERTScore / TTSDS2。結果は `docs/benchmarks/` に JSON で記録
- **L3(musculus 固有)**: 正規化層の直接評価(数値・記号・日付の読み F1)+ ablation ΔCER
- **人間評価**: 必要時に自分用 CMOS(上記プロトコル)。proxy 指標の ja 妥当性はその時点で校正する

## 9. マイルストーン

| Phase | 内容 | 受け入れ基準 |
|---|---|---|
| M0 | 設計文書 + workspace 足場(trait / types / mock / CI) | `cargo test --workspace` green |
| M1 | SBV2JE アダプタ(ort 直叩き)+ setup スクリプト + CLI synth→WAV | 「こんにちは」が WAV に出る。RTF 計測例あり |
| M2 | 評価基盤。**完了**(2026-09-13):L3 読み gate(非 gap 14/14)+ round-trip CER(mean text 0.140 / reading 0.050、`docs/benchmarks/cer-ja/baseline.json`、euhadra L1 共作物差し)。proxy MOS は M4 前に校正 | 両 gate が回る ✓ |
| M3 | 正規化層。**完了**(2026-09-13):`JaNormalizer`(L3 ギャップ 7/8 閉鎖、CER reading 0.050→0.035)+ `TermDictionary`(辞書併用で L3 22/22)。残 1 項目(latin-letters)は辞書層の所有 | L3 22/22 with dict ✓ |
| M4 | Irodori 比較アダプタ + 自分用 CMOS | 盲検プロトコルで決定を ADR 化 |
| M5 | 多言語(en)拡張 | ja の設計が en に歪んでいないことの検証 |

## 10. 未決事項

- モデルライセンスの残りの未確認項目 → `docs/model-licenses.md` §4(deberta.onnx の出自、tsukuyomi 元モデルの card、VOICEPEAK 学習データのグレー、Irodori 側 codec/encoder のライセンス)。コード側のライセンスは全て確定済み(MIT/BSD 系、musculus は MIT のまま)
- `.sbv2` 形式の内容(M1 の最初の実装タスク)
- `SpeechSegment` のスタイル/キャプション表現(実装が示すまで凍結)
- ストリーミング合成(M4 以降)
- crates.io 公開時の README 英語化(ライセンスは決定済み: MIT OR Apache-2.0、euhadra と同型)