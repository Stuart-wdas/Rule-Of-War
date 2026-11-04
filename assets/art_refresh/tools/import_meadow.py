"""Download CC0 scan originals and derive colour-managed runtime meadow maps."""
from pathlib import Path
import concurrent.futures, hashlib, json, sys, urllib.request
import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
ROAD = '--road' in sys.argv
ASSET = 'aerial_asphalt_01' if ROAD else 'rocky_terrain_02'
TARGET = 'asphalt' if ROAD else 'meadow'
SOURCE = ROOT / 'assets/incoming_textures' / ASSET
OUT = ROOT / 'materials/pbr'
SOURCE.mkdir(parents=True, exist_ok=True)
sys.path.insert(0, str(ROOT / '.data/vfx-tools/python-libs'))
import png

MAPS = {
    'diff': '1144cf37cee6625215bc0d58ce64c13b',
    'nor_gl': 'ebf5dd974833176225f7f6241e93edbc',
    'arm': '306938afc089f9b50e1504c2515203f4',
}
if ROAD:
    MAPS = {'diff':'919ca6dec0f961ae1b76e3cfeef7809e',
            'nor_gl':'5c9651f4c10d3b1c40a73ece2808852a',
            'arm':'34cfca121459807595e2f7a15a400a78'}
def fetch(item):
    key, checksum = item
    path = SOURCE / f'{ASSET}_{key}_2k.png'
    url = f'https://dl.polyhaven.org/file/ph-assets/Textures/png/2k/{ASSET}/{path.name}'
    if not path.exists():
        urllib.request.urlretrieve(url, path)
    assert hashlib.md5(path.read_bytes()).hexdigest() == checksum, path
    w, h, rows, info = png.Reader(filename=str(path)).asRGBA()
    data = np.vstack([np.asarray(row) for row in rows]).reshape(h, w, 4)[:, :, :3].astype(np.float32)
    data /= (2 ** info['bitdepth'] - 1)
    if key == 'nor_gl':
        data = data * 2 - 1
        data[:, :, :2] *= .8
        data /= np.maximum(np.linalg.norm(data, axis=2, keepdims=True), 1e-6)
        data = data * .5 + .5
    if key == 'arm':
        data[:, :, 2] = 0  # dielectric; preserve AO and roughness
    suffix = {'diff': 'albedo', 'nor_gl': 'normal', 'arm': 'orm'}[key]
    target = OUT / f'{TARGET}_{suffix}.png'
    Image.fromarray(np.uint8(np.rint(np.clip(data, 0, 1) * 255))).save(target)
    print(f'{suffix}: {w}x{h}, verified original', flush=True)
    return suffix, hashlib.sha256(target.read_bytes()).hexdigest(), url

with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
    results = list(pool.map(fetch, MAPS.items()))
catalog = json.loads((OUT / 'catalog.json').read_text())
catalog['runtime_max_size'] = max(catalog.get('runtime_max_size', 1024), 2048)
catalog['materials'][TARGET] = {
    'repeat_m': [30., 30.] if ROAD else [90., 90.], 'size': [2048, 2048],
    'source': f'https://polyhaven.com/a/{ASSET}',
    'author': ('Rob Tuytel' if ROAD else 'Amal Kumar') + ' / Poly Haven', 'license': 'CC0 1.0 Universal',
    'normal': 'OpenGL +Y', 'normal_strength': .8, 'orm': 'R=AO G=roughness B=0',
    'runtime_sha256': {s: h for s, h, _ in results},
    'source_urls': [u for _, _, u in results],
}
(OUT / 'catalog.json').write_text(json.dumps(catalog, indent=2))
(SOURCE / 'SOURCE.txt').write_text(f'{ASSET} / Poly Haven. CC0 1.0 Universal.\n'
    f'https://polyhaven.com/a/{ASSET}\nhttps://polyhaven.com/license\n'
    'Source PNGs are untouched; runtime derivatives are in materials/pbr/catalog.json.\n')
