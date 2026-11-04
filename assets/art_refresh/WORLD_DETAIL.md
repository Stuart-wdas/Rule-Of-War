# Battlefield surface and effects refresh

The shared native renderer applies these changes to both normal games and the
training ground. Restart the game after updating the executable and assets.

* Daylight: stronger, warmer direct sun, reduced cool directional fill and
  softer ambient energy give the scene a hot afternoon appearance. The sky
  remains cool, preserving separation in shaded surfaces. A small filmic
  contrast adjustment applies to the world and HDR explosions together. The
  approved explosion frames and their baked shadow direction are preserved;
  the original bake profile remains unchanged and records its source lighting.

* Ground: the measured 16-bit displacement from Poly Haven's
  [Rocky Terrain 02](https://polyhaven.com/a/rocky_terrain_02) accompanies its
  matching albedo, normal, AO and roughness. Render relief is limited to 12 cm
  either side of the simulation surface and fades out on water and paved areas.
  It does not change navigation, collision, weapon ranges or terrain saves.
  The original generated meadow albedo remains another blend layer. Meadow
  shading uses the full measured scan normal at its matching 90 m scale,
  independently of the colour blend; the generated layer's neutral normal no
  longer dilutes it. Dirt and asphalt retain their own normal-map transitions.
* Vegetation: larger irregular groups combine shrubs, narrow grass blades,
  ground cover and intermediate stones. Local scatter follows the ground and
  respects building clearings. Balanced and High allow low-growth shadows;
  existing SSAO supplies contact shading. Grass sun shadows are restricted to
  nearby chunk centres (48 m Balanced, 80 m High), avoiding expensive thin-blade
  shadows across distant cascades. Batches remain streamed by world chunk.
* Roads: terrain shoulders have irregular material transitions. Town roads use
  worn gravel margins rather than a continuous concrete band and follow the
  visible ground surface. Navigation retains its existing road geometry.
* Tracks: vehicles leave paired soil/abrasion marks. The same pressure mask
  bends low grass down in the colour, depth, shadow and motion-vector passes.
  Marks persist during a match; they are presentation history, not save-game
  data. Very old marks retire when the bounded history fills. Respawns,
  teleports, map changes, water and stationary units do not paint long streaks.
  Separate raised town/base road meshes do not yet receive the terrain wear
  shader; ground tracks remain visible on the terrain road surfaces and fields.
* Rockets: the existing turbulent smoke animation now runs on curved,
  depth-tested surfaces with lit exhaust colouring, irregular sizes, drift,
  expansion and fading. These approximate smoke volumes; they are not a runtime
  fluid simulation. Shot trajectories and damage timing are unchanged.
* Vehicles: tank hulls gain spare track shoes, retaining hardware and small
  panel fittings. Review captures use the same fixed lens at 1080p to separate
  model detail from resolution and camera-distance differences.

## Per-device controls

Escape → Settings controls environment, textures, shadows and upscaling.
Environment Low/Balanced/High uses 512/1024/2048 pressure masks (0.25/1/4 MiB),
2,048/6,144/12,288 retained tread segments and 256/512/768 projectile particles.
Nearby stone/grass counts also scale with environment detail. Texture and
explosion settings retain their existing independent controls.

## Sources and reproducibility

Poly Haven's original displacement is retained untouched under
`assets/incoming_textures/rocky_terrain_02`, with API metadata and its verified
MD5. Poly Haven assets use [CC0](https://polyhaven.com/license). Runtime
derivatives and hashes are in `materials/pbr/catalog.json` under
`meadow_relief_v1`; its ORM blue channel stores height, not metallic.
`tools/import_ground_relief.py` rebuilds the derivatives.

Growth vertex paths adapt Bevy 0.18.1's MIT/Apache-2.0 shaders; the relevant
licences are retained in `native/vendor/bevy_anti_alias`. The helper
`tools/build_growth_shaders.py` documents the modifications to both render paths.
Smoke art comes from the project's existing explosion VFX collection.

## Review

`tools/check_world_detail.py` creates native 1920 × 1080 captures, checks renderer
errors, tracks and smoke counts, and compares the camera metadata for matched
before/after pairs. It also exercises Low, DLSS, town surfaces and a combined
battle. Reports are saved under `.data/render-checks/world-detail`.

The October 24 validation completed 14 capture cases, including a GPU-visible
track-mask diagnostic, matched afternoon-lighting captures, Low detail, DLSS
Quality and a 30-second combined battle. All captures exited without renderer
or shader errors. The final code passed all 316 native tests and Clippy with
warnings denied. These checks used the RTX 3060 Laptop; the RTX 5050 Laptop
has not been benchmarked here. Comparisons are saved beside this document as
`world-detail-ground-comparison.png`, `world-detail-smoke-comparison.png` and
`world-detail-sun-comparison.png`.

The subsequent full meadow-normal change was checked with matched 1080p
before/after captures, Low detail and DLSS Quality. All three updated render
cases completed without shader errors. Results are under
`.data/render-checks/meadow-normal`; `meadow-normal-comparison.png` shows the
normal-strength change with the same lighting and camera.
