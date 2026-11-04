# Persistent explosion ground marks

The old marks used one fixed five-metre crop from a multi-crater image, plus
twelve identical rounded dirt pieces. The crop included parts of neighbouring
marks and painted highlights. More seriously, its height could disagree with
the displayed terrain, leaving grass visible through the middle.

## Implemented plan

1. Replace the atlas crop with four complete, feathered soil/ejecta PBR masks
   derived from the existing soil scan. Keep all lighting out of the albedo.
2. Give shells smaller disturbed-soil scars and artillery/rockets wider marks.
   Store size, excavation type, seed and orientation with the persistent prop.
3. Replace the repeated rounded rim with a low, broken lip and a few angular
   soil clods, all in one mesh per impact with a shared material.
4. Project onto the terrain mesh actually installed by the renderer. This
   includes the coarse/detail transition at tile edges. Reproject after a
   nearby crater or terrain-tile replacement, retaining the existing mesh handle.
5. Check matched native captures, overlapping hits, slopes, cloud shade, low
   graphics, save round trips, older saves, water/armour exclusion and budgets.

Only existing heavy weapons deform simulation terrain. Tank shell and helicopter
rocket scars are cosmetic. Damage, navigation and the Blender explosion clips
retain their existing behaviour. Direct ground hits leave soil marks; water,
airbursts, armour and occupied building contact do not receive soil craters.
Repeated hits at the same crater do not stack identical marks. A heavy hit can
replace a shallow shell mark. The most recent 512 soil marks are retained;
retired marks leave their saved terrain deformation intact.

## Assets and review

- `review.jpg`: closer crops of actual native captures, including the original
  marks, revised marks, sloping ground and the same marks under cloud shade.
- `before-*.jpg` / `after-*.jpg`: complete 1920 x 1080 captures.
- `soil-masks.png`: four transparent 512-pixel soil masks in a 1024-pixel atlas.
- `bake.json`: source and output hashes. The source soil material's original
  provenance is recorded in `materials/pbr/catalog.json`.
- `validation.json`: checks recorded after the final native review.

`tools/bake_impact_scars.py` rebuilds the albedo, tangent-space micro normals
and roughness/occlusion maps under `materials/pbr/impact_scar_*`. Sunlight,
ambient fill and moving cloud shade are applied by the game's ordinary PBR
renderer. No new image-generation service or externally acquired art is used.

`tools/check_impact_scars.py` runs reproducible native captures. Its
`--impact-scar-check` scene invokes the real projectile impact path, clears the
transient effects and holds the camera so the persistent ground can be judged.
`--scar-slope` moves that scene onto a hill. The ordinary visual demonstration
also produces ground scars for its shell, artillery and rocket strikes.

Older saves without scar metadata render as a medium artillery scar with a
stable seed from the saved prop ID. New saves preserve the actual variation.
The renderer uses fewer than 850 vertices per mark and shares three 1024-pixel
PBR atlases. Unchanged marks do not upload new geometry every frame.

Validation completed: the full native suite passed all 422 tests, including six
new ground-scar tests. Formatting passed. Four final 1080p native scenarios
passed with the darker soil assets: flat ground, sloping ground, moving cloud
shade and Low graphics. Each produced the expected five persistent marks,
three weapon sizes and one shared mesh per mark. The review images show these
final captures; `validation.json` records the asset and screenshot hashes.
