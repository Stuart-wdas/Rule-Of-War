"""Compile generated height DATA into PBR channels; never retouch the albedos.

Heights are artistic approximations, not measured scans. Only shallow normal
relief is inferred from them; collision and terrain geometry remain unchanged.
"""
from pathlib import Path
import hashlib
import json
import numpy as np
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
MAPS = ROOT / 'materials/pbr/ground_demo'

def compile_maps():
    catalog_path = ROOT / 'materials/pbr/catalog.json'
    catalog = json.loads(catalog_path.read_text())
    manifest = {}
    for name, repeat, depth in [('meadow', 14., .10), ('shoulder', 8., .045)]:
        albedo = MAPS / f'{name}.png'
        original_hash = hashlib.sha256(albedo.read_bytes()).hexdigest()
        height_image = Image.open(MAPS / f'{name}_height.png').convert('L')
        assert height_image.size == Image.open(albedo).size
        # Numerical smoothing avoids sparkle from single-pixel AI height noise.
        height = np.asarray(height_image.filter(ImageFilter.GaussianBlur(.85)), dtype=np.float32) / 255.
        height *= depth
        dx = (np.roll(height, -1, 1) - np.roll(height, 1, 1)) / (2 * repeat / height.shape[1])
        dz = (np.roll(height, -1, 0) - np.roll(height, 1, 0)) / (2 * repeat / height.shape[0])
        normal = np.stack((-dx, -dz, np.ones_like(height)), axis=2)
        normal /= np.linalg.norm(normal, axis=2, keepdims=True)
        rgba = np.full((*height.shape, 4), 255, dtype=np.uint8)
        rgba[:, :, :3] = np.round((normal * .5 + .5) * 255).astype(np.uint8)
        Image.fromarray(rgba).save(MAPS / f'{name}_normal.png')
        # Gentle local cavity occlusion, independent of sun direction.
        broad = np.asarray(height_image.filter(ImageFilter.GaussianBlur(7)), dtype=np.float32) / 255.
        cavity = np.clip(broad - height / depth, 0, .4)
        orm = np.full_like(rgba, 255)
        orm[:, :, 0] = np.round(255 * (1 - cavity * .40)).astype(np.uint8)
        orm[:, :, 1] = np.round(np.clip(.92 - height / depth * .06, .85, .94) * 255).astype(np.uint8)
        orm[:, :, 2] = 0
        Image.fromarray(orm).save(MAPS / f'{name}_orm.png')
        assert hashlib.sha256(albedo.read_bytes()).hexdigest() == original_hash
        # Copy raw generated colour bytes to a versioned runtime asset.
        target = ROOT / f'materials/pbr/photo_{name}_v1_albedo.png'
        target.write_bytes(albedo.read_bytes())
        for suffix in ['normal', 'orm']:
            (ROOT / f'materials/pbr/photo_{name}_v1_{suffix}.png').write_bytes((MAPS / f'{name}_{suffix}.png').read_bytes())
        record = {'repeat_m': [repeat, repeat], 'size': list(height_image.size),
            'albedo_linear_mean_rgb': np.power((np.asarray(Image.open(albedo).convert('RGB'), dtype=np.float64) / 255 + .055) / 1.055, 2.4).mean(axis=(0,1)).tolist(),
            'source': 'OpenAI built-in image generation',
            'provenance': 'assets/art_refresh/ground-demo/generation.json',
            'normal': 'OpenGL +Y, central differences of AI companion height, approximate shallow relief',
            'normal_height_range_m': depth,
            'orm': 'R=gentle local cavity AO G=matte roughness B=0',
            'usage': 'Opt-in --ground-demo only; no baked moving-object shadows',
            'runtime_sha256': {suffix: hashlib.sha256((ROOT / f'materials/pbr/photo_{name}_v1_{suffix}.png').read_bytes()).hexdigest() for suffix in ['albedo', 'normal', 'orm']}}
        catalog['materials'][f'photo_{name}_v1'] = record
        manifest[name] = record
    catalog_path.write_text(json.dumps(catalog, indent=2) + '\n')
    (ROOT / 'assets/art_refresh/ground-demo/materials.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(manifest, indent=2))

if __name__ == '__main__':
    compile_maps()
