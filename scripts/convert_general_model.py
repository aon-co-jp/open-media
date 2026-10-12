"""realesr-general-x4v3(.pth) → ncnn(.param/.bin)。torch無しで.pthを読み、animevideov3と同じ層構成(SRVGGNetCompact)で書き出す。
dn: ノイズ除去の強さ(公式は wdn モデルとの重みの線形補間: w = dn*general + (1-dn)*wdn)。"""
import io, pickle, struct, sys, zipfile, urllib.request, array, os

BASE = 'https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0/'
out_dir = sys.argv[1]
dn = float(sys.argv[2]) if len(sys.argv) > 2 else 0.5
cache = os.path.join(out_dir, '_pth'); os.makedirs(cache, exist_ok=True)

def fetch(name):
    p = os.path.join(cache, name)
    if not os.path.exists(p):
        urllib.request.urlretrieve(BASE + name, p)
    return p

class Storage:
    def __init__(self, key): self.key = key

class Tensor:
    def __init__(self, storage, offset, size, stride): self.s, self.o, self.size, self.stride = storage, offset, size, stride

def rebuild(storage, offset, size, stride, *a):
    return Tensor(storage, offset, size, stride)

class U(pickle.Unpickler):
    def __init__(self, f, z, prefix):
        super().__init__(f); self.z, self.prefix = z, prefix
    def find_class(self, module, name):
        if name == '_rebuild_tensor_v2': return rebuild
        if name == 'OrderedDict':
            import collections; return collections.OrderedDict
        if name.endswith('Storage'): return type(name, (), {})
        return super().find_class(module, name)
    def persistent_load(self, pid):
        # ('storage', storage_type, key, location, numel)
        return Storage(pid[2])

def load_state(path):
    z = zipfile.ZipFile(path)
    names = z.namelist()
    pkl = [n for n in names if n.endswith('data.pkl')][0]
    prefix = pkl[:-len('data.pkl')]
    obj = U(io.BytesIO(z.read(pkl)), z, prefix).load()
    if 'params_ema' in obj: obj = obj['params_ema']
    elif 'params' in obj: obj = obj['params']
    sd = {}
    for k, t in obj.items():
        raw = z.read(prefix + 'data/' + t.s.key)
        n = 1
        for d in t.size: n *= d
        arr = array.array('f'); arr.frombytes(raw[t.o * 4:(t.o + n) * 4])
        sd[k] = (arr, list(t.size))
    return sd

a = load_state(fetch('realesr-general-x4v3.pth'))
b = load_state(fetch('realesr-general-wdn-x4v3.pth'))
assert a.keys() == b.keys()
def blend(k):
    x, sx = a[k]; y, sy = b[k]
    assert sx == sy
    return array.array('f', (dn * p + (1 - dn) * q for p, q in zip(x, y))), sx

idx = sorted({int(k.split('.')[1]) for k in a if k.startswith('body.')})
convs = [i for i in idx if len(a[f'body.{i}.weight'][1]) == 4]
prelus = [i for i in idx if len(a[f'body.{i}.weight'][1]) == 1]
assert convs[0] == 0 and len(convs) == len(prelus) + 1, (convs, prelus)
n_conv = len(convs)

lines, blob, cur = [], [], 0
def add(kind, name, ins, outs, params=''):
    lines.append(f"{kind:<24} {name:<24} {len(ins)} {len(outs)} {' '.join(ins)} {' '.join(outs)}{(' ' + params) if params else ''}")
lines_hdr = []
add('Input', 'input.1', [], ['data'])
add('Split', 'splitncnn_input0', ['data'], ['input.1_splitncnn_0', 'input.1_splitncnn_1'])
prev = 'input.1_splitncnn_1'; blobn = 100
binout = bytearray()
for ci, i in enumerate(convs):
    w, sw = blend(f'body.{i}.weight'); bias, _ = blend(f'body.{i}.bias')
    oc, ic = sw[0], sw[1]
    nxt = f'b{blobn}'; blobn += 1
    add('Convolution', f'Conv_{2*ci}', [prev], [nxt], f'0={oc} 1=3 4=1 5=1 6={oc*ic*9}')
    binout += struct.pack('<I', 0) + w.tobytes() + bias.tobytes()
    prev = nxt
    if ci < len(prelus):
        sl, _ = blend(f'body.{prelus[ci]}.weight')
        nxt = f'b{blobn}'; blobn += 1
        add('PReLU', f'PRelu_{2*ci+1}', [prev], [nxt], f'0={len(sl)}')
        binout += sl.tobytes()
        prev = nxt
last_oc = a[f'body.{convs[-1]}.weight'][1][0]
scale = 4
assert last_oc == 3 * scale * scale, last_oc
add('PixelShuffle', 'DepthToSpace', [prev], ['ps'], f'0={scale}')
add('Interp', 'Resize', ['input.1_splitncnn_0'], ['up'], f'0=1 1={scale}.000000e+00 2={scale}.000000e+00')
add('BinaryOp', 'Add', ['ps', 'up'], ['output'])
os.makedirs(out_dir, exist_ok=True)
name = 'realesr-general-x4v3'
open(os.path.join(out_dir, name + '.param'), 'w', newline='\n').write('7767517\n%d %d\n' % (len(lines), blobn - 100 + 6) + '\n'.join(lines) + '\n')
open(os.path.join(out_dir, name + '.bin'), 'wb').write(bytes(binout))
print('convs', n_conv, 'bin bytes', len(binout), 'dn', dn)
