"""Bake four neutral, unlit soil/ejecta PBR masks from the project's soil scan.

No directional light or painted crater shadow is stored in these maps: the
terrain and broken lip provide shape under the game's actual sun/cloud light.
"""
import hashlib
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'materials/pbr'
REVIEW = ROOT / 'assets/art_refresh/impact-scars'
CELL = 512


def smooth(a, b, x):
    u = np.clip((x - a) / (b - a), 0, 1)
    return u * u * (3 - 2 * u)


def noise(rng, size):
    grid = Image.fromarray(rng.random((size, size)).astype('float32'))
    return np.asarray(grid.resize((CELL, CELL), Image.Resampling.BICUBIC))


def bake():
    source = ROOT / 'materials/pbr/soil_albedo.png'
    soil = np.asarray(Image.open(source).convert('RGB').resize((CELL, CELL))) / 255.
    maps = [np.zeros((CELL * 2, CELL * 2, 4), dtype='uint8') for _ in range(3)]
    yy, xx = np.mgrid[:CELL, :CELL].astype('float32')
    x, y = (xx + .5) / CELL * 2 - 1, (yy + .5) / CELL * 2 - 1
    radius, angle = np.hypot(x, y), np.arctan2(y, x)
    for variant in range(4):
        rng = np.random.default_rng(7103 + variant * 911)
        broad, medium, grain = [noise(rng, n) for n in (10, 47, 210)]
        phase = rng.uniform(0, np.pi * 2, 5)
        boundary = (1 + .11 * np.sin(angle * 3 + phase[0])
                    + .07 * np.sin(angle * 7 + phase[1])
                    + .04 * np.sin(angle * 13 + phase[2]))
        r = radius / boundary + (broad - .5) * .10
        core = 1 - smooth(.19, .43, r)
        rim = np.exp(-((r - .45) / .13) ** 2)
        # Uneven radial streaks and isolated grains, rather than a circular halo.
        lobes = np.clip((np.sin(angle * 19 + phase[3] + broad * 3) - .28) * 1.2, 0, 1)
        streaks = lobes * (1 - smooth(.48, .94, r)) * smooth(.37, .54, r)
        islands = smooth(.64, .78, grain) * smooth(.55, .72, broad)
        coverage = 1 - smooth(.40, .73, r + (medium - .5) * .20)
        breakup = .25 + .75 * smooth(.30, .70, medium * .65 + broad * .35)
        coverage *= 1 - smooth(.30, .63, r) * (1 - breakup)
        alpha = np.maximum(coverage, streaks * .48 + islands * .35)
        alpha *= (1 - smooth(.78, .98, radius)) * .98
        alpha[radius > .99] = 0
        photo = np.roll(soil, (variant * 87, variant * 137), axis=(0, 1))
        luminance = photo.mean(2)
        # Fresh subsoil has restrained warm colour; the centre is disturbed,
        # darker earth, with enough variation to remain legible in shade.
        outer = np.array([70., 61., 45.])
        centre = np.array([36., 30., 22.])
        colour = outer + (centre - outer) * core[..., None]
        colour += np.array([4., 2., -1.]) * rim[..., None]
        colour *= (.55 + luminance * .69 + medium * .25)[..., None]
        colour *= (1 - streaks * .19)[..., None]
        colour += (grain - .5)[..., None] * 15
        # Local furrows/clods only. The actual lip has geometric normals.
        furrows = np.sin(angle * 31 + phase[4] + broad * 2)
        relief = (grain - .5) * .012 + (medium - .5) * .024
        relief += furrows * rim * .014
        dy, dx = np.gradient(relief, 6.5 / CELL)
        normal = np.stack([-dx, -dy, np.ones_like(dx)], axis=2)
        normal /= np.linalg.norm(normal, axis=2, keepdims=True)
        albedo = np.dstack([np.clip(colour, 0, 255), alpha * 255]).astype('uint8')
        normal = np.dstack([(normal * .5 + .5) * 255, np.full_like(dx, 255)]).astype('uint8')
        orm = np.dstack([(0.83 + medium * .16) * 255,
                         (0.9 + grain * .09) * 255, np.zeros_like(dx),
                         np.full_like(dx, 255)]).astype('uint8')
        row, col = variant // 2 * CELL, variant % 2 * CELL
        for atlas, tile in zip(maps, [albedo, normal, orm]):
            atlas[row:row + CELL, col:col + CELL] = tile
    REVIEW.mkdir(parents=True, exist_ok=True)
    files = []
    for suffix, pixels in zip(['albedo', 'normal', 'orm'], maps):
        path = OUT / f'impact_scar_{suffix}.png'
        Image.fromarray(pixels).save(path)
        files.append(dict(path=str(path.relative_to(ROOT)), sha256=hashlib.sha256(path.read_bytes()).hexdigest()))
    Image.fromarray(maps[0]).save(REVIEW / 'soil-masks.png')
    manifest = dict(cellPixels=CELL, variants=4, source=str(source.relative_to(ROOT)),
                    sourceSha256=hashlib.sha256(source.read_bytes()).hexdigest(),
                    lighting='Unlit albedo and local micro normals; live game lighting', files=files)
    (REVIEW / 'bake.json').write_text(json.dumps(manifest, indent=2) + '\n')
    print(json.dumps(manifest, indent=2))


if __name__ == '__main__':
    bake()
