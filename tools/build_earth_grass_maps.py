"""Compile generated companion height data to PBR maps, preserving albedo bytes."""
from pathlib import Path
import hashlib
import json
import numpy as np
from PIL import Image, ImageFilter

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'materials/pbr/earth_grass_v2'
OUT = ROOT / 'assets/art_refresh/earth-grass-v2'

def main():
    path = ROOT / 'materials/pbr/catalog.json'
    catalog = json.loads(path.read_text())
    records = {}
    for name, repeat, depth in [('olive', 8., .08), ('stony', 6., .10)]:
        label = f'earth_grass_{name}_v2'
        albedo = SOURCE / f'{name}.png'
        height = Image.open(SOURCE / f'{name}_height.png').convert('L')
        assert height.size == Image.open(albedo).size
        h = np.asarray(height.filter(ImageFilter.GaussianBlur(1.1)), dtype=np.float32) / 255
        # Only technical data processing. Colour assets remain direct generated PNGs.
        dx = (np.roll(h, -1, 1) - np.roll(h, 1, 1)) * depth * h.shape[1] / (2 * repeat)
        dz = (np.roll(h, -1, 0) - np.roll(h, 1, 0)) * depth * h.shape[0] / (2 * repeat)
        normal = np.stack((-dx, -dz, np.ones_like(h)), axis=2)
        normal /= np.linalg.norm(normal, axis=2, keepdims=True)
        rgba = np.full((*h.shape, 4), 255, dtype=np.uint8)
        rgba[:,:,:3] = np.round((normal * .5 + .5) * 255).astype(np.uint8)
        Image.fromarray(rgba).save(ROOT / f'materials/pbr/{label}_normal.png')
        surrounding = np.asarray(height.filter(ImageFilter.GaussianBlur(9)), dtype=np.float32) / 255
        cavity = np.clip(surrounding - h, 0, .5)
        orm = np.full_like(rgba, 255)
        orm[:,:,0] = np.round(255 * np.clip(1 - cavity * .7, .7, 1)).astype(np.uint8)
        orm[:,:,1] = np.round(255 * np.clip(.94 - h * .09, .85, .96)).astype(np.uint8)
        orm[:,:,2] = 0
        Image.fromarray(orm).save(ROOT / f'materials/pbr/{label}_orm.png')
        (ROOT / f'materials/pbr/{label}_albedo.png').write_bytes(albedo.read_bytes())
        rgb = np.asarray(Image.open(albedo).convert('RGB'), dtype=np.float64) / 255
        linear = np.where(rgb <= .04045, rgb / 12.92, ((rgb + .055) / 1.055) ** 2.4)
        records[label] = {'repeat_m': [repeat, repeat], 'size': list(height.size),
            'source': 'OpenAI built-in image generation',
            'provenance': 'assets/art_refresh/earth-grass-v2/generation.json',
            'normal': 'OpenGL +Y, approximate shallow relief from generated companion height',
            'normal_height_range_m': depth,
            'albedo_linear_mean_rgb': linear.mean(axis=(0,1)).tolist(),
            'orm': 'R=local cavity AO G=matte roughness B=0',
            'lighting': {'sun_ray_world': [.444,-.778,-.444], 'short_shadow_direction_image': [1,-1], 'variation': 'translation only; no rotation or mirroring'},
            'usage': 'Natural terrain in ordinary matches, training and ground demo. Excludes asphalt/concrete.',
            'runtime_sha256': {s: hashlib.sha256((ROOT / f'materials/pbr/{label}_{s}.png').read_bytes()).hexdigest() for s in ['albedo','normal','orm']}}
        catalog['materials'][label] = records[label]
    catalog['materials']['photo_shoulder_v1']['usage'] = 'Natural road shoulders in ordinary matches, training and ground demo; asphalt and concrete retain their materials.'
    path.write_text(json.dumps(catalog, indent=2) + '\n')
    (OUT / 'materials.json').write_text(json.dumps(records, indent=2) + '\n')
    print(json.dumps(records, indent=2))

if __name__ == '__main__':
    main()
