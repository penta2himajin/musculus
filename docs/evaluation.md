# 評価方針(Testing Policy)

musculus の TTS パイプラインを評価する**方針**を定める。実測値は `docs/benchmarks/` 配下に JSON として蓄積し、この文書は含まない。euhadra の `docs/evaluation.md` の鏡像として、同じ 3 層構造・2 軸設計・baseline JSON 方針を引き継ぐ。

評価コードと評価データを同一リポジトリに混在させない。データは外部から都度取得する(アノテーション gold セットは例外として in-tree)。

---

## 1. テストスイートの 3 層構造

### 1.1 L1 — スモークテスト(CI 用、毎回実行)

**目的**: パイプラインが壊れていないことの確認 + 知覚性(round-trip CER)と速度の trend tracking。

**構成**:

1. `evaluate-synth` — 合成 smoke: fixture 文を mock/SBV2JE アダプタで合成し、WAV 出力・RTF・レイテンシ p50/p95 を記録
2. `evaluate-cer` — **round-trip CER**(§2): fixture 文 → musculus 合成 → 固定 ASR で転写 → CER
3. `evaluate-fast` — 正規化層の ablation(L3)と μ-bench(テキストのみ、秒単位)

**fixture**: 自作の正規化テスト文(数値・日付・記号)と、FLEURS ja または Common Voice ja の文章サブセット(M2 で確定)。

### 1.2 L2 — 標準ベンチマーク(リリース毎、手動実行)

**目的**: アダプタ間比較と SOTA 参照。proxy MOS を測る。詳細は §4。

### 1.3 L3 — musculus 固有検証(正規化層の直接評価)

**目的**: ASR の鏡像にある正規化層(数値・記号・日付・辞書)が正しく読みを展開できているかを、音声を介さず直接測る。詳細は §3。

---

## 2. round-trip CER(TTS の知覚性評価)

### 2.1 定義

TTS の出力は音声であり正解テキストと直接比較できないため、**ASR を物差しとして使う**:

```
[入力テキスト] → musculus(正規化+合成) → [WAV]
    → 固定 ASR で転写 → [転写テキスト]
    → 両側を同じ正規化へ → CER(文字単位, ja)/ WER(単語単位, en)
```

CER = (置換 + 削除 + 挿入) / 正解文字数(Levenshtein 距離)。ja は文字単位 CER、en は単語単位 WER。

**実装(2026-09-13)**: `src/eval.rs` に euhadra 移植の strict CER + round-trip 用の `cer_normalized`(句読点・大小文字を正字法ノイズとして両側から除去)。さらに**読みレベル CER** — 参照文・ASR 転写の両方を同一 ja フロントエンドで読みに落として音素列比較 — を併用する。正字法(猫 vs ねこ)の差を数えない知覚性の指標で、実測では「猫が座って動かない」が text CER 0.667 / reading CER 0.000 と分離した。ランナーは `examples/eval_cer.rs`、結果は `docs/benchmarks/cer-ja/`。

### 2.2 物差し ASR

- ja: parakeet-tdt_ctc-0.6b-ja(euhadra L1 と同一モデル)。**物差しはバージョン込みで baseline JSON に記録し、変更は baseline 更新と同 PR で行う**
- ASR 自身の誤りはスコアに含まれる。比較対象は「同じ物差しでの相対値」のみ

### 2.3 落とし穴と対策

| 落とし穴 | 内容 | 対策 |
|---|---|---|
| ASR 誤りの混入 | スコア = TTS 誤り + ASR 誤り | 物差し固定・バージョン記録 |
| hallucination | attention デコーダは破綻音声に流暢なでたらめを返しうる(euhadra の Canary 無音捏造事故) | 音声端の無音トリミング、文分割して短く渡す |
| 自然性が不可視 | 単調な声でも CER は完璧 | 自然性は §4 の proxy/人間評価で別軸測定 |
| 誤りの起因が混ざる | normalizer の誤展開が TTS を経由して CER に出る | §3 の L3 で正規化層を単独評価し切り分ける |
| サンプリングレート | 合成 48kHz → ASR 16kHz の変換品質 | 変換条件を固定・記録。ネイティブレートの値も併記 |
| 合成のばらつき | diffusion 系は seed 依存 | N seed の mean±σ を報告 |
| 正規化しすぎ | lenient で表記差を吸収すると層の寄与が見えなくなる(euhadra が strict/lenient を分けた経緯) | ablation は strict、live 比較は lenient、と使い分ける |

---

## 3. L3: 正規化層の読み正解率

音声を使わない、**テキスト → 読みの直接評価**。ground truth を手で書ける・CI で回せる・失敗の原因が 1 層に絞れる(euhadra L3 の filler F1 と同型)。

### 3.1 データ形式

`tests/evaluation/annotations/{ja,en}.jsonl`:

```jsonl
{"input": "3.14",      "reading": "さんてんいちよん", "category": "numeral_decimal"}
{"input": "¥1,200",    "reading": "せんにひゃくえん", "category": "currency"}
{"input": "2026/2/14", "reading": "にせんにじゅうろくねんにがつじゅうよっか", "category": "date"}
{"input": "Wi-Fi",     "reading": "わいふぁい", "category": "abbreviation"}
```

### 3.2 指標(実装 2026-09-13)

1. **per-item 音素列一致**:入力をフロントエンドに通し、生成音素列(g2p)と gold 読み(カナ → モーラ分割 → 音素)を句読点除去の上で比較。**音素レベルで比較するため、発音が同じ綴り揺れ(「ジュウ」/「ジュー」)は等価**。テストは `tests/l3_ja.rs`
2. **gap フィールド**:フロントエンドがまだ読めない項目はアノテーションに `gap` を記録し、gate は「非 gap 100%」を強制。gap 項目は worklist(docs/benchmarks/l3-ja/baseline.json)として管理し、修正が入った PR で gap を落とす
3. span 単位 F1 は複数箇所混在文の評価で将来導入(現行セットは 1 項目 = 1 読み判断のため不要)
4. **CER の差が語彙処理の差であることがある**:2026-09-13、Irodori と SBV2 の reading CER 差(0.086 vs 0.035)は造語 "musculus" の文字読み処理がほぼ全てで、辞書を適用すると 0.039 vs 0.032 に収束した。**CER は「読めているか」を測るが「自然か」は測らない** — 同じ測定で盲検 A/B は Irodori 5/5(+2.75)だった。CER の差を見たら、まず語彙・正規化の差を疑う
5. **エンジンへ「読み」を渡す案は棄却(2026-09-13)**:musculus の SBV2 フロントエンドで正確なカナ読みを作り、それを Irodori に供給する案を `eval_cer --kana-readings` で検証。**reading CER は 0.086 → 0.202 に悪化**(「資料を12枚印刷してください」は 0.000 → 0.812)。Irodori のテキストエンコーダは自然な表記(漢字かな混じり)で学習されており、カナだけを渡すと単語境界・文脈の手掛かりが失われるため。**各エンジンには自然なテキストを渡し、語彙の問題は辞書で直す**のが正しい分担
6. **A/B 提示の前にラウドネス一致を検証する**:steps 比較の初回生成は −16 LUFS 目標でピークリミッタが作動し、2 つの音が約 3.6 dB ずれていた(RMS 0.14 vs 0.09)。ターゲットを下げて両側を目標到達させ、提示前に達成 LUFS をペア単位で検証する手順をスクリプト化した(`scripts/make_ab_test_steps.sh`)。**マッチしていない A/B は大小のバイアスを持ち込む**
7. **RTF は発話長とセットで報告する**:固定費(DACVAE decode・duration・符号化)が支配する短い発話では RTF が大きく出る。1.36 秒の発話で RTF 1.08 だった構成が、2〜6 秒の文セットでは 0.83 だった
6. **既知の死角:ピッチアクセント**。現行 L3 は音素列の一致のみを見るため、拍の高低が誤っていても通る。2026-09-13 の盲検 A/B で「せんに→ひゃく」(期待「せんに↑ひゃく」)というアクセント誤りが人間の耳で検出されたのが実例。**最初の実測で欠陥を検出(2026-09-14)**:native speaker のアノテーションにより、`1,200` の期待形は `HHLHH`(前置きあり)/`LLLHH`(文頭)で**センの 2 モーラが同一トーン**になることが判明。現行の素性は `HLLHH`(セ=H, ン=L)で両方に反する。原因候補は「特殊拍(ン・ッ・ー)は直前モーラのトーンを継承すべき」という規則が素性生成に無いこと。詳細と次の一手は docs/benchmarks/accent/ja-report.md。

**計測機構は実装済み(2026-09-14)**:`src/sbv2/ja.rs::kana_tone` が mora 単位の H/L 素性を取り出し、`examples/accent_report.rs` が入力→カナ→tones を一覧する。`tests/evaluation/annotations/ja_accent.jsonl` に `expected_tones`(1 文字 = 1 モーラの H/L)を記入すると一致チェックになり、非ゼロ終了するので CI gate にできる。現状の出力は docs/benchmarks/accent/ja-report.md。

**切り分けの注意**:このレポートが見るのは decode に渡る**素性**であり、音響的な F0 ではない。「素性は正しいが実現が平坦」というケース(1,200 がその疑い)はこのレポートでは検出できないため、必要なら F0 の実測を併用する

### 3.3 カテゴリ

整数/小数/負数、日付(複数表記)、時刻、通貨・単位、曖昧読み(`10分` = じゅっぷん/じっぷん、`1人` = ひとり)、記号・略語、漢字の特殊読み。曖昧読みは「意図した読み」を gold に明記し、**孤立トークン(ルールの正しさ)と文脈込みの文(実際の読み上げ品質)を別ファイルで管理する**。

### 3.4 実装上の位置づけ

- 正規化層は jpreprocess(OpenJTalk 純 Rust 再実装)が数値展開の大部分を担う。musculus は「jpreprocess に任せる分 + 独自辞書・記号展開の分」の**所有権を層ごとに明示**し、L3 はその保証をテストする
- **2 軸設計**: (a) 直接評価 F1(テキストのみ、CI 毎回)(b) ablation ΔCER — normalizer を off にした round-trip CER との差でエンドツーエンドへの寄与を示す
- round-trip CER が悪い時、L3 の正解率が高ければ TTS 側、低ければ正規化側、と即座に切り分けられる

---

## 4. L2: proxy 指標(リリース時、参考値)

| 指標 | 役割 | 注意 |
|---|---|---|
| **UTMOS** | MOS 予測(BVCC 学習、英語) | **ja での妥当性は未検証**。初回の自前 CMOS(§5)で相関を校正するまでは傾向追跡のみに使う |
| **SpeechBERTScore** | 生成音声と参照音声の SSL 特徴間 BERTScore。多言語 SSL(XLSR)で ja も有効 | 参照音声が必要。人間相関は UTMOS の OOD 実測より高い(Interspeech 2024) |
| **TTSDS2** | 人間主観で検証済みの客観指標(ρ≈0.67)。PyPI `ttsds` | 四半期更新の公開ベンチマークと突き合わせる |
| **SIM** | 話者類似度(WavLM-ECAPA コサイン) | 音声クローン検証時のみ。≥0.7 で飽和、循環性に注意 |
| DNSMOS / PESQ / MCD | 使用しない | DNSMOS は雑音抑圧学習であり TTS 評価への流用は誤用。MCD は人間相関が弱い |

結果は `docs/benchmarks/<name>/*.json` にコミットし、CI baseline と同じ「相対(regression %)+ 絶対(hard floor)」の 2 軸で回帰判定する(euhadra 流儀)。

---

## 5. 人間評価

自動 proxy は人間評価の**代替ではなく傾向追跡の代理**である。意思決定(M4 のアダプタ選定など)は人間の聴取で行う。

**自分用 CMOS プロトコル**:

1. 同じテキスト 10〜20 文を両アダプタで合成。声の好みと合成品質を分離したい場合、音声クローン参照で声を統一する
2. 順序をランダム化し盲検で聴く
3. -3〜+3(0.5 刻み)で採点し、`docs/benchmarks/` に記録、決定は ADR 化
4. 客観裏付け(round-trip CER / UTMOS / RTF)を併記

より大きな判断や外部向けの報告には、ITU-T P.808 流儀の小規模クラウド MOS と、ja の自動 MOS 校正データ(UTMOS の ja 相関測定)を初回に 1 回実施する。コミュニティ比較の参照点として TTS Arena を用いる(en 中心の注記付き)。

---

## 6. データ方針

- 評価データはスクリプトで都度取得。ライセンスが OSS 配布物と衝突するデータは commit しない(CIEMPIESS 教訓の ja 側 mirror: ja gold セットは自作するため原則問題なし、素材コーパスのライセンスのみ注意)
- ja 文章の参照セットは M2 で FLEURS ja / Common Voice ja(CC-BY 4.0 / CC0)から選定し、この文書に追記する
- 合成音声の評価用 WAV はリポジトリに commit しない(体積管理。ハッシュとメタデータのみ記録)