# ADR-0003: ort rc.13 直叩きの自前実装(sbv2_core への依存を置かない)

- ステータス: 採用
- 日付: M0
- 関連: docs/spec.md §5–§6

## 背景と決定

SBV2JE の最初の実装方式を、既存クレート `sbv2_core` への依存ではなく、**euhadra と同じ `ort` 2.0.0-rc.13 + ndarray 0.17 での手書きパイプライン**とする。sbv2_core は依存にせず参照実装として読む。

## 根拠

1. **ランタイムの二重化を避ける**。sbv2_core(0.2.0-alpha8)は `ort` 2.0.0-rc.9 と ndarray 0.16 に固定される。euhadra の onnx feature は ort rc.13 / ndarray 0.17。同一ツリーに置くと ONNX Runtime と ndarray が 2 系統に分かれ、バイナリが太り、将来 euhadra と musculus を同一ワークスペースで使う(双方向音声の最終形)際に干渉リスクが残る
2. **euhadra の実装様式との整合**。euhadra は Canary / Whisper-ONNX / Paraformer / Dolphin をすべて ort 直叩きで手書きし、その方針で「依存が重くなるのは feature の後ろ」という依存ポリシーを維持してきた
3. **先行実装は読める**。sbv2_core の推論コード(`synthesize` の入力系:x_tst / tones / lang_ids / style_vec / bert / sdp_ratio / length_scale)と irodori-tts-webgpu の JS ランタイムが公開されており、移植対象の把握に十分

## コストと緩和

- 負: 最初の音声が出るまでの工数が sbv2_core 依存より増える
- 緩和: 参照実装の存在、および trait(`TtsAdapter`)が既に分離されているため、実装方式の入れ替え(将来 CoreML EP の追加、別実装への差し替え)が表面に波及しない
- 緩和: M1 で RTF が実用外なら、その時点で方式を再評価する(測定を理由にした再決定を妨げない)

## 意思決定

依存は最小に保ち、実装は参照実装を読んで手書きする。sbv2_core が rc.13 へ追従した後も、すでに手書きした実装を捨てて依存に寄せる意図は今のところない(その判断は M1 の測定後に下す)。