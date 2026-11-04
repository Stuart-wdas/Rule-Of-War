# Military model and material refinement

This pass updates the shared native renderer, including the training ground.
Restart through `Play Native.cmd` after rebuilding. Camera, combat balance,
navigation footprints and weapon socket positions retain their existing rules.

## Ground vehicles

Battle/support/anti-air vehicles now use a closed, faceted turret shell with a
sloping upper face rather than the hull-shaped turret. The support gun has a
distinct rear fighting compartment. Carriers and rocket artillery use a shaped
armoured cab; the carrier also has a recessed rear ramp, hinges, roof rails and
spaced side panels. Wheels use profiled tyre shoulders and bead recesses.
Layered deck edges, grab fittings, rear containers and side protection complete
the existing tracks and suspension. Turret/barrel/wheel articulation is retained.

Faction camouflage has a more restrained palette and reuses the supplied
`materials/aircraft_paint.png` for neutral photographic paint grain and scratches.
Lower hulls receive dust and soil staining; aircraft receive separate light dust
and longitudinal grime, including parked aircraft, rather than ground-vehicle
mud. The A-10 keeps its authored livery and reference silhouette.

## Aircraft

The existing reference-shaped airframes and canopies are retained. The Apache
gains transmission vents, cockpit gutters, wire-cutting fittings and side access
covers. The A-10 gains small wing-root access panels and vents. Additional
details stay in the rigid material batches and within the aircraft stand bounds.
The quality budget targets roughly 12,000 triangles, allowing more where useful.
The A-10 body and canopy now use subdivided curved surfaces with held cap
boundaries. Existing rotor, fan, gun and gear joints
are preserved.

Measured complete-kit triangle counts after refinement:

| Model | Triangles |
| --- | ---: |
| Battle tank | 13,776 |
| Support gun | 14,492 |
| Troop carrier | 15,316 |
| Rocket artillery | 11,304 |
| Anti-air | 14,740 |
| Apache | 12,574 |
| A-10 | 8,686 |
| Equipped infantry, before role attachments | 10,834 |

## Infantry

The Blender source now uses **10,834 triangles** for the equipped character.
Additional sleeve/trouser rings, a finer head/helmet and rounded, padded pouch
and pack profiles use the expanded geometry budget. Source
fabric/leather bump and roughness are baked into a matching 2K normal atlas;
short-range ambient occlusion supplies contact shading around webbing and gear.
Skin and weapon surfaces retain their own material response. The GLB exports
only its baked UV atlas as UV0 so the runtime tangent basis matches its normals;
the editable Blender mesh retains the source UVs. The four gameplay clips,
35-bone skeleton, grip controls and muzzle marker remain intact. Faction tints
are restrained so the baked materials retain their colour differences.

Armour shells and tyres are exported from Blender with real multi-segment
bevels and weighted/smooth normals. Their editable source is
`assets/art_refresh/refined_kit.blend`; `tools/build_refined_kit.py` rebuilds its
runtime mesh data. Armour paint uses matching 1K colour, normal and roughness
detail; camouflage colour boundaries do not generate relief.

Rebuild with Blender using `assets/infantry/tools/build_infantry.py -- --no-render`.
Run `validate_infantry.py` with Python and `check_roundtrip.py` with Blender.
The source, GLB and individual normal/AO maps are under `assets/infantry`.

## Architecture

Houses gain projecting window lintels and shutter louvers. Individual curved
tile noses supply real roof-edge depth while the scanned maps supply the finer
roof field. Military facilities gain high wall vents, rain hoods, utility boxes
and conduit; vents sit above their windows. The watchtower's duplicated diagonal
members are replaced with proper cross braces. Roof and wall attachments retain
their cutaway/destruction grouping. Architecture colours use a quieter palette.

## Review

Matched 1080p captures are saved under `.data/render-checks/model-overhaul`.
The baseline executable and original infantry source/export are retained there.
The vehicle close-up fixture accepts `--vehicle-model KIND` for the carrier,
support gun, anti-air and rocket artillery, in addition to `--tank-model`.
These are real-time model refinements; the aircraft are not wholly new sculpts,
and the small character remains an RTS mesh rather than a cinematic character.

Validation: all 318 native tests passed, including complete-model budgets,
new mesh face/normal checks, aircraft stand bounds, weapon sockets and gameplay
regressions. Strict Clippy passed. The infantry GLB passes independent buffer,
skin, UV-channel and material checks and 17 sampled source/export poses. The
native suite covers every vehicle kit, infantry roles, both aircraft, buildings,
Low detail, DLSS Quality and a 30-second combined battle at 1920 x 1080.
