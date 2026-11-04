# Spiky explosion — editable 3D remake

Created 20 October 2026. This effect was authored locally, then moved into
Rule of War alongside its two existing 3D explosions. It was not extracted from a game.

## What changed

- A short central fireball ignites and burns out by frame 21.
- Eleven spikes start **inside** that fireball, at frames 5–8.
- Launch speeds are 16–21 units/second at elevations of 41–64 degrees. Gravity
  (18 units/second squared) and air drag (0.28/second) bend each trajectory
  through a visible apex and back down; the tips cover over four units in eight frames.
- Fire is limited to compact moving tips, rather than filling the entire spikes.
- Smoke is deposited along each tip's actual path, then drifts, expands and fades.
- Rolling central smoke carries the later part of the effect.

## Files

- `spiky_explosion.blend`: the complete editable 3D scene, with procedural volume
  materials, animated particles, lights, and a virtual preview camera.
- `build_spiky_explosion.py`: reproducible Blender source, with a fixed seed.
- `frames/frame_001.png` through `frame_064.png`: 512 × 512 RGBA renders.
- `spiky_explosion_8x8_4096.png`: 64-frame RGBA flipbook, 8 × 8, 4K.
- `spiky_explosion_8x8_2048.png`: 2K alternative with the same ordering.
- `spiky_explosion_preview.mp4`: opaque presentation preview, not an alpha source.
- `particle_tracks.json`: engine-neutral sampled 3D positions, scales and opacity,
  plus each volume's orientation axis. Elongated smoke segments align with the
  local flight tangent to bridge gaps along the faster curved trajectories.
- `SpikyExplosion.mjs`: a Three.js adapter for the 3D tracks; one instanced draw
  call with procedural volume shading and no external image assets.
- `effect.json`, `validation.json`: settings and verification results.

## Playback and materials

Play once, **64 frames at 24 FPS = 2.6667 seconds**. Blender uses frames 1–64;
sprite sheets use indices 0–63, left-to-right and top-to-bottom. First and last
frames are transparent. PNGs use straight/unassociated alpha: standard
SrcAlpha/OneMinusSrcAlpha blending, with sRGB color and linear alpha.

The source has real 3D particle trajectories and volumetric shading. It is an
authored procedural effect, **not a baked fluid/CFD simulation**. No external
volume cache or licensed third-party assets are needed. Opening the `.blend`
does not require running the Python generator: the materials and keyframes are
already saved. Use Cycles/rendered view to inspect the intended volume shading.

The full effect is not a solid object suitable for a conventional mesh-only GLB.
Use the native scene, the real-time adapter, or the rendered flipbooks. The
experimental Three.js shader approximates the Blender material, so the two renderers will
not produce identical pixels. It includes fire and smoke, while the Blender
scene also includes small debris. Transparent particle intersections are an
approximation, and the adapter does not receive scene shadows or clip individual
raymarch samples against scene depth. Use uniform scale on its root object.

## Three.js use

Copy the module and tracks into your game's asset pipeline. Use your existing
Three.js installation; there are no CDN dependencies.

```js
import * as THREE from 'three';
import { createSpikyExplosion } from './SpikyExplosion.mjs';

const tracks = await fetch('/vfx/particle_tracks.json').then(r => r.json());
const explosion = createSpikyExplosion(THREE, tracks);
explosion.object.position.set(impactX, terrainHeight, impactZ);
explosion.object.scale.setScalar(1);
scene.add(explosion.object);

// Each game frame; age is seconds since detonation. Pass your current camera
// to select orthographic/perspective rays and sort smoke particles by depth.
if (!explosion.update(age, camera)) explosion.dispose();
```

The Three.js adapter converts Blender's Z-up coordinates to Three.js Y-up.
It does not change the existing game's source or add detonation triggers.
Its track playback was tested in Node; a browser was unavailable for shader QA.
Its shader uses 28 raymarch samples per visible particle; prefer flipbooks or
lower sample counts when many explosions overlap on screen.

Rule of War's native Bevy renderer also plays these tracks as 3D volumes. Open
`View 3D Explosions.cmd` at the repository root to compare all three variants;
press 3 for this one. Gameplay detonations choose one of the three variants
from a stable per-event random seed, so clients and replays agree on the choice.
Tank shell detonations, rocket/artillery blasts, vehicle/building destruction,
aircraft crashes, cannon detonations and missile airbursts share that selection.

## Rebuild

```text
blender -b --factory-startup --disable-autoexec --python build_spiky_explosion.py -- --render-all
```

The generator writes the scene, tracks and PNG frames alongside itself.
`package_spiky_explosion.py` then assembles sheets, previews, checksums and ZIPs.
All authored source and effect material in this directory are provided under
the included MIT license (`LICENSE.txt`).
