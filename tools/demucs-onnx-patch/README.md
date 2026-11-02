# demucs-onnx-patch

`src-tauri/src/engine/ai_upmix.rs`(AIアップミックス、モノ/ステレオ→5.1ch/7.1ch)が使う
HT-Demucsモデルの準備スクリプト。

## 背景

AIアップミックスは、実在する音源分離モデル**HT-Demucs**(Meta、MIT)の
[demucs-onnx](https://github.com/StemSplit/demucs-onnx)による ONNX 変換版
([StemSplitio/htdemucs-onnx](https://huggingface.co/StemSplitio/htdemucs-onnx))を、
make-diskの他のAI機能(RNNoise/LavaSR)と同じく**純Rustのtractで推論**する
(Pythonは実行時に一切使わない)。

ただし配布されている`htdemucs.onnx`をそのまま`tract`で読み込むと、tract側のONNX
`Pad`/`Range`演算子の実装が狭く、2種類のエラーで失敗する(onnxruntimeでは問題なく動く。
モデル自体やdemucs-onnxのエクスポートが壊れているわけではない):

1. **Pad**: ONNX仕様では3番目の入力(`constant_value`)は省略可能だが、tractの型推論規則は
   厳密に3入力を要求する。
2. **Range**: モデルの入力長を固定すると、本来は動的計算される値(Transformer内の位置
   インデックス、STFTのフレーム分割用インデックスなど、計697個のRangeノード)も全て
   定数化できる。tractはこれらのRange出力の型推論(symbolic dimension vs 具体的なint64)で
   失敗する。

`patch_htdemucs_onnx.py`は、この2つをonnxライブラリで機械的に修正し(Pad: 明示的な定数
入力を追加、Range: onnxruntimeで実際に1回推論して値を捕捉しConstantノードへ置換)、
tractで読み込める`htdemucs_patched.onnx`を作る。**2026-09-28にtractでの読み込み・
最適化・推論実行(出力shape `[1,4,2,343980]`)まで実機確認済み。**

パッチ後のモデルは入力長が343,980サンプル(44.1kHzで約7.8秒)固定になる
(元は動的長だった)。より長い音声は`ai_upmix.rs`側でこの長さのチャンクに区切って
繰り返し推論する(単純な固定長チャンク分割で、Demucs本来のオーバーラップ加算による
クロスフェードは行わない近似——チャンクの境目でごく僅かな不連続が生じ得る)。

## 使い方

```bash
python -m venv .venv
.venv/Scripts/pip install onnx onnxruntime numpy  # Windows。macOS/Linuxは .venv/bin/pip

# StemSplitio/htdemucs-onnx から元モデルを取得(約316MB)
curl -L -o htdemucs.onnx https://huggingface.co/StemSplitio/htdemucs-onnx/resolve/main/htdemucs.onnx

.venv/Scripts/python patch_htdemucs_onnx.py htdemucs.onnx <出力先ディレクトリ>
# -> <出力先>/htdemucs_patched.onnx (+ .onnx.data、合計 約2.2GB)
```

生成した`htdemucs_patched.onnx`(+`.onnx.data`)を、環境変数
`MAKE_DISK_DEMUCS_MODEL_DIR`が指すフォルダへ置くと、make-diskがAIアップミックスに使う。

## 既知の課題(次回)

- **モデル配布は未実装**。パッチ済みモデルが約2.2GB(GitHub Releasesの標準上限2GB超)と
  大きく、他のAI機能(LavaSR約56MB等)と同じ「初回自動ダウンロード」方式をまだ用意できて
  いない。配布先(分割ホスティング、HuggingFaceへの再アップロード等)の検討が必要。
- モデルファイル自体が316MB(元)→2.06GB(パッチ後)に肥大化している(693個のRangeノードを
  ノード属性としてConstant化したため、`save_as_external_data`の対象〈グラフの
  initializer〉に含まれず外部データ化されない)。将来的には、ベイクした定数を
  ノード属性ではなく通常のinitializerとして追加すればサイズを抑えられる可能性がある。
- チャンク境界のクロスフェード(オーバーラップ加算)は未実装。
