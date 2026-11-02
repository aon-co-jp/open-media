#!/usr/bin/env python
"""htdemucs.onnx(StemSplitio配布、Demucs v4のHT-Demucsをdemucs-onnxでONNX化したもの)を、
tract(純Rust ONNX推論、make-diskの他のAI機能=RNNoise/LavaSRと同じ路線)で読み込める形へ
修正するワンショットのパッチスクリプト。

正直な開示: このスクリプトはPython(onnx/onnxruntime)を使うが、これは「モデル準備」
(開発時に1回だけ実行してパッチ済み.onnxファイルを作る)専用であり、make-disk本体の
実行時にはPythonを一切使わない(パッチ済み.onnxをtractが読み込むだけ)。

tractが元のhtdemucs.onnxを読み込めない理由は2つ、いずれもtract側のONNX Pad/Range
演算子の実装の狭さに起因する(モデル自体やdemucs-onnxのエクスポートが壊れているわけではなく、
onnxruntimeでは問題なく動く):

1. Pad演算子: ONNX仕様では3番目の入力(constant_value)は省略可能だが、tractの型推論規則は
   厳密に3入力を要求する。→ 全Padノードに明示的な定数(0.0)を3番目の入力として補う。
2. Range演算子: モデルの入力長を固定(343980サンプル=44.1kHzで約7.8秒)すると、本来は
   実行時に動的計算される値(Transformer内の位置インデックス、STFTのフレーム分割用インデックス
   など、計697個のRangeノード)も全て定数化できる。tractはこれらのRange出力の型推論
   (symbolic dimension vs 具体的なint64)で失敗するため、onnxruntimeで実際に1回推論を
   実行して値を捕捉し、Constantノードへ置き換える。

このパッチにより、モデルは343980サンプル(固定長)の入力しか受け付けなくなる
(元は動的長だった)。より長い音声は、呼び出し側でこの長さのチャンクに分割して
繰り返し実行する(engine::ai_upmixが行う)。

使い方:
    pip install onnx onnxruntime numpy
    python patch_htdemucs_onnx.py <元のhtdemucs.onnx> <出力先ディレクトリ>
"""
import sys
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort
from onnx import helper, numpy_helper, TensorProto

FIXED_INPUT_SAMPLES = 343_980  # htdemucsの1ショット処理長(44.1kHzで約7.8秒)


def patch(input_path: str, output_dir: str) -> None:
    out_dir = Path(output_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    print(f"loading {input_path} ...")
    m = onnx.load(input_path)
    g = m.graph

    # ── 1. Pad: 欠けている/空文字列の3番目の入力(constant_value)を補う ──
    const_name = "_zero_pad_const_f32"
    g.initializer.append(numpy_helper.from_array(np.array(0.0, dtype=np.float32), name=const_name))
    pad_fixed = 0
    for n in g.node:
        if n.op_type == "Pad":
            if len(n.input) == 2:
                n.input.append(const_name)
                pad_fixed += 1
            elif len(n.input) == 3 and n.input[2] == "":
                n.input[2] = const_name
                pad_fixed += 1
    print(f"patched {pad_fixed} Pad node(s)")

    # ── 2. Range: onnxruntimeで実際に1回推論して全Range出力の値を捕捉し、定数化する ──
    ranges = [n for n in g.node if n.op_type == "Range"]
    out_names = [n.output[0] for n in ranges]
    print(f"found {len(ranges)} Range node(s), capturing concrete values via onnxruntime...")

    # 型はRangeのstart入力を生成したConstant/Castノードから静的に判定する
    producer = {o: n for n in g.node for o in n.output}

    def dtype_of(tensor_name: str):
        n = producer.get(tensor_name)
        if n is None:
            return None
        if n.op_type == "Constant":
            for attr in n.attribute:
                if attr.name == "value":
                    return attr.t.data_type
        if n.op_type == "Cast":
            for attr in n.attribute:
                if attr.name == "to":
                    return attr.i
        return None

    m2 = onnx.ModelProto()
    m2.CopyFrom(m)
    g2 = m2.graph
    for n in ranges:
        dt = dtype_of(n.input[0]) or TensorProto.INT64
        g2.output.append(helper.make_tensor_value_info(n.output[0], dt, None))

    sess = ort.InferenceSession(m2.SerializeToString(), providers=["CPUExecutionProvider"])
    dummy = np.zeros((1, 2, FIXED_INPUT_SAMPLES), dtype=np.float32)
    outs = sess.run(out_names, {"mix": dummy})
    data = dict(zip(out_names, outs))

    range_names = {n.name for n in ranges}
    new_nodes = []
    for n in g.node:
        if n.name in range_names:
            arr = data[n.output[0]]
            tensor = numpy_helper.from_array(arr, name=n.output[0] + "_baked_const")
            new_nodes.append(helper.make_node("Constant", inputs=[], outputs=[n.output[0]], name=n.name + "_baked", value=tensor))
        else:
            new_nodes.append(n)
    del g.node[:]
    g.node.extend(new_nodes)
    print(f"baked {len(ranges)} Range node(s) into Constants")

    out_onnx = out_dir / "htdemucs_patched.onnx"
    out_data = "htdemucs_patched.onnx.data"
    # onnx.checker.check_model()は2GB超のモデルを一括シリアライズしようとして失敗するため省略する
    # (onnxruntime/tractでの実動作は別途確認済み)。重みは外部データファイルへ逃がして保存する。
    onnx.save(m, str(out_onnx), save_as_external_data=True, all_tensors_to_one_file=True, location=out_data)
    print(f"saved {out_onnx} (+ {out_data})")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(1)
    patch(sys.argv[1], sys.argv[2])
