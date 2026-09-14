# モデル・依存ライセンス一覧

musculus は**重みを同梱しない**(AGENTS.md Prohibitions #1)。この文書は、setup スクリプトがユーザに取得させ、アダプタが実行時に読み込む全てのモデルと、その上位ライブラリのライセンスを記録する。euhadra の `docs/model-licenses.md` と同じ位置づけだが、musculus では「同梱しない」ことが規約の根幹なので、**上位ソースへの参照と、利用者が守るべき規約**を主に書く。

調査日: 2026-XX(M1 開始前)。ライセンスは各ソースの現時点の記載に従う。

---

## 1. コード側(リポジトリに置くもの) — 全て musculus と矛盾しない

> **M1 実装後の補足**: musculus の ja フロントエンド(g2p)は sbv2_core の LGPL 部分を複製せず独自実装した。参照した MIT データ(mora 表・シンボル表)の出自は各ファイル先頭の PROVENANCE 注記に記す。musculus のライセンス(MIT OR Apache-2.0)に変更はない。

| 項目 | ライセンス | 根拠 | 備考 |
|---|---|---|---|
| musculus | MIT OR Apache-2.0 | `LICENSE-MIT` / `LICENSE-APACHE` | euhadra と同じデュアル。ライブラリ消費者の企業ポリシー(Apache-2.0 の特許条項要求)に対応 |
| `ort` / ONNX Runtime | MIT | pyke.io / GitHub | `onnx` feature。rc.13 固定(ADR-0003) |
| sbv2_core(参照実装として読む) | **MIT**、ただし `jtalk.rs` のみ LGPL-3.0 | crates.io 全バージョンの `license` フィールド、neodyland/sbv2-api の LICENSE.md | **依存にはしない**(ADR-0003)。MIT なので読んで移植・言及は問題なし。⚠️ `jtalk.rs`(g2p 実装)は litagin02 の `g2p.py` / VOICEVOX 由来で LGPL ヘッダを持つため **musculus はこの 1 ファイルを複製せず、アルゴリズムを独自に実装した**(`src/sbv2/ja.rs` の PROVENANCE 注記参照)。`mora_list.json`・シンボル表は MIT のデータとして採用 |
| jpreprocess(将来 M3) | BSD-3-Clause | crates.io `license` フィールド | OpenJTalk の純 Rust 再実装 |
| jpreprocess-naist-jdic | BSD(NAIST Japanese Dictionary) | jpreprocess リポジトリ | **必ず `naist-jdic` feature を選択**。lindera の AGPL 辞書系(sbv2_core の `agpl_dict`)は使わない |

## 2. モデル重み(同梱しない。setup スクリプトが上位ソースを指す)

### 2.1 M1 ベースライン: googlefan/sbv2_onnx_models(tsukuyomi JP-Extra ONNX セット)

- 中身: `tsukuyomi.sbv2`(声モデル)+ `deberta.onnx`(音声フロントエンドの BERT)+ `tokenizer.json` + ONNX Runtime バイナリ(MIT)
- **HF リポジトリに license タグがない**。カードは「sbv2-api を参照」とのみ記載 → ライセンスは下位の声(deberta はそのモデルのライセンス)に従うという構造。再配布はせず、setup スクリプトはこのリポジトリを**直接指す**こと
- **deberta.onnx の出所**: SBV2 系は ku-nlp/deberta-v2-large-japanese-char-wwm 系(litagin の ONNX 化は CC-BY-SA-4.0 タグ)。CC-BY-SA-4.0 は**実行時利用に問題なし**だが、ONNX を改変・再配布すると ShareAlike 義務が発生し得る — musculus が再配布しない理由のひとつ。googlefan セット内の deberta がどの deberta かは M1 で実物確認する

### 2.2 声の規約(利用者が守るもの — musculus の README / CLI で案内する)

**つくよみちゃん(CV. 夢前黎)** — tsukuyomi モデル:

- キャラクターライセンス: フリー素材、**商用利用可・事前連絡不要・クレジット必須**(説明とクレジット)。公式: https://tyc.rei-yumesaki.net/about/terms/
- 声のみをキャラなしで使う場合(音声合成ソフト経由)は「使用する音声合成ソフト・音声ライブラリの規約に従う」が原則
- VOICEPEAK つくよみちゃん(AHS/Dreamtonics)の EULA: 生成音声の商用・業務利用可。**「別のソフトウェアに組み込んで商用利用する際は要問い合わせ」(条項 12)**
- ⚠️ **グレー**: litagin 系 tsukuyomi モデルの学習データが VOICEPEAK 生成音声由来である場合、「EULA の許諾範囲内か」は明文化された保証がない。モデル配布側(litagin)の判断が前提となっており、musculus はその上で動く。商用利用を計画する場合は VOICEPEAK 発行元への確認を推奨 — M1 で model card の明示規約を再確認し、ここを更新する

**あみたろ / 小春音アミ(あみたろの声素材工房)** — sbv2_amitaro / sbv2_koharune_ami:

- 規約内なら誰でも・無料・商用可。**クレジット必須**:「あみたろの声素材工房(https://amitaro.net/)」または「小春音アミ」。公式: https://amitaro.net/voice/voice_rule/
- 禁止: 年齢制限用途、政治・宗教活動、素材の転売・独占、素材の抽出可能な形での同梱
- AI モデルの配布は日本語版 FAQ で条件付き OK(SBV2 形式なら事前相談 + クレジット + 非公式である旨の明記)。**英語版規約では「AI モデルの配布禁止」となっており、日英で差異がある** — 日本語環境での利用は日本語版規約を、英語圏配布は英語版規約を確認すること
- 自作モデルに「あみたろ」等の名前を使うことは禁止(公式と混同防止)

## 3. AGPL-3.0 の扱い(重要)

- **litagin02/Style-Bert-VITS2(学習・推論の Python コード)は AGPL-3.0**(fishaudio/Bert-VITS2 の fork)。JP-Extra の**ベースモデル重み**も HF タグで AGPL-3.0(litagin/Style-Bert-VITS2-2.0-base-JP-Extra)
- musculus への影響は**構造的に分離されている**:
  1. musculus の Rust 実装は sbv2_core(MIT)を参照にした**独立実装**であり、AGPL コードを一切含まない・リンクしない。AGPL コードを移植しない限り musculus は MIT のままでよい
  2. 重みは**実行時にユーザが取得し、メモリ上で ONNX 推論するだけ**。重みファイルの読み込みは musculus を AGPL の派生物にしない(コードのリンクが存在しないため)
  3. したがって setup スクリプトの鉄則: **URL を指すだけで、重みを再ホスト・同梱・改変配布しない**。AGPL の義務は重みを再配布する者が負う
- sbv2_core の `agpl_dict` default feature は lindera 系 AGPL 辞書を指す。musculus がいつか jpreprocess を直接依存にする際は `naist-jdic`(BSD)を明示選択する

## 4. 未確認(M1 で確定させる)

1. googlefan セット内 `deberta.onnx` の出自モデルとそのライセンス(§2.1)
2. tsukuyomi.sbv2 の元 PyTorch モデルの model card(学習データの出自、声の規約の明示)— HF API の検索で litagin 名義の tsukuyomi モデルが見つからなかったため、sbv2-web スペースや sbv2-api のドキュメントから辿る
3. VOICEPEAK 生成音声を学習データにすることの EULA 上の整合(§2.2 つくよみちゃん、グレー)
4. **M4 の Irodori 用 — 確認済み(2026-09-13、スパイク実装と同時)**: ONNX アーティファクトは webgpu ランタイム作者の公式パッケージ [noguchis/irodori-tts-onnx](https://huggingface.co/noguchis/irodori-tts-onnx)(license: mit-and-apache-2.0)。内訳:Irodori-TTS-500M-v3 重み **MIT**(モデルカードになりすまし禁止条項)、Semantic-DACVAE-Japanese-32dim codec **MIT**(facebook/dacvae-watermarked 由来)、llm-jp-3-150m トークナイザ **Apache-2.0**。アーティファクト同梱の LICENSES/ に各一次ライセンスが同梱されている(setup_irodori.sh が取得)。musculus は重みを同梱しない(posture 不変)。なお v4.1-Small については **ライセンス確認済み(2026-09-14)**: 重みは MIT(Aratako/Irodori-TTS-v4.1-Small のモデルカード)、テキスト/キャプションエンコーダの [sbintuitions/modernbert-ja-310m](https://huggingface.co/sbintuitions/modernbert-ja-310m) も **MIT**、codec は MIT。倫理条項(なりすまし禁止・誤情報禁止・免責)が重みに付随する。**残る実装上の障害はライセンスではなく、公式 ONNX エクスポートが存在しないこと**(現行の v3 用エクスポートスクリプトは 500M-v3 向け)

## 5. 運用ルール

- setup スクリプトは各リソースの**一次配布元 URL** と、取得時に表示すべき規約へのリンクをコメントに含める
- CLI で初めて声を使う際、`--voice` のライセンス概要(クレジット要件)を表示する(M1 以降)
- この一覧の更新は、採用モデルが変わる PR と同じ PR で行う(baseline JSON と同じ流儀)

## 6. アクセント・韻律データの資源(2026-09-14 調査)

ADR-0006 の判断根拠。**musculus はこれらを同梱せず、setup スクリプトで取得する前提**。

| 資源 | ライセンス | 可否と用途 |
|---|---|---|
| [tdmelodic](https://github.com/PKSHATechnology-Research/tdmelodic)(日本語アクセント辞書生成) | **BSD-3-Clause** | ✅ 商用可。UniDic + NEologd からニューラル推定で大語彙アクセント辞書を生成([Tachibana & Katayama, ICASSP 2020](https://doi.org/10.1109/ICASSP40776.2020.9054081))。標準アクセント辞書層の本命 |
| [tdmelodic_openjtalk](https://github.com/sarulab-speech/tdmelodic_openjtalk) | **未確認** | ⚠️ OpenJTalk 辞書化。jpreprocess と直結しうるため優先的に確認する |
| [UniDic](https://clrd.ninjal.ac.jp/unidic/)(国語研短単位自動解析用辞書) | **GPLv2 / LGPLv2.1 / 修正BSD のトリプル**(現代書き言葉 Ver.2.x+、現代話し言葉 Ver.2.2.x+ 以降) | ✅ 商用可。アクセント情報付与は [KAKEN 19K13173](https://kaken.nii.ac.jp/ja/grant/KAKENHI-PROJECT-19K13173/)(クラウドソーシング) |
| [UniDic非コアデータ](https://teru-oka-1933.github.io/unidic_non_core/) | **Apache-2.0**(2019.08 に MIT から変更) | ✅ 商用可。ただし確認した公開物はカタカナ略語・複合語分割リストで、**アクセントデータの所在は要確認** |
| [koniwa(声庭)](https://github.com/koniwa/koniwa) | アノテーション **CC0**、プログラム **Apache-2.0**、音声 CC BY / PD | ✅ **韻律アノテーション付き音声**。L3 gold・検証コーパス |
| [OJAD](https://www.gavo.t.u-tokyo.ac.jp/ojad/pages/notes)(オンライン日本語アクセント辞書) | **教育・学術研究目的に限定**。「営利活動を目的とした企業の研究開発での利用はお控えください」 | ❌ 製品では使用しない |
| NHK日本語発音アクセント新辞典 | 著作物(放送文化研究所の研究記事は参照可) | ❌ データ同梱不可。**公開された“原則”のみ**規則設計の根拠として参照 |

**運用**: 「標準アクセント」を目指す場合も、**NHK 辞書そのものは使わず**、(1) 文献の原則に基づく規則、(2) ユーザ所有の上書き表、(3) tdmelodic(BSD-3)で生成した辞書、の三層で到達する。

**NHK アクセント辞典の扱い(参照用)**:『NHK日本語発音アクセント新辞典』は著作物であり、**musculus には同梱せず、データを複製もしない**。参照用として扱う範囲は:

- 放送文化研究所が公開している**研究記事**(規則・傾向・支持率の記述)→ 規則設計の根拠として引用
- 消費者が辞書を所有している場合、必要な形を**自分のアクセント上書き表**(`--accent`)に記入して使う — データは消費者の手元に留まる

資源の全体像と次の作業は [docs/accent-resources.md](accent-resources.md) に記録。
