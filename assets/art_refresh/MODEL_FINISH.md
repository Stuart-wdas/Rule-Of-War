# Surface and contact shading follow-up

This pass follows `MODEL_OVERHAUL.md`. It uses the existing refined geometry,
animation joints, scanned surfaces and approved fixed-camera explosion renders.

## Shared lighting

The game camera now carries a small linear-HDR daylight reflection probe.
It has a blue sky, a warm horizon and muted ground bounce, with prefiltered
roughness levels for metal, painted surfaces and glass. There is no second sun
disk: the directional light supplies sunlight. The probe adds neither a skybox
nor a fog overlay. Its intensity is 280; the warm sun direction and exposure
remain the same as the preceding art pass.

The diffuse cube is 8 x 8 per face. The specular cube is 64 x 64 per face with
seven mip levels. Together they contain 265,200 bytes of half-float data. These
are generated offline by `tools/build_daylight_probe.py` and embedded in the
executable. They represent sky and ground lighting, not reflections of nearby
vehicles or buildings. The directional shadow normal bias is reduced from 1.8
to 1.15 to attach shadows more closely to small fittings.

## Vehicle contacts and finish

Five ground vehicle kits have short-range contact visibility baked against
their exact exported assemblies: battle tank, support gun, troop carrier,
rocket artillery and anti-air. Sixteen hemisphere rays per unique surface
vertex capture nearby overhangs and overlapping fittings. The bake affects
indirect diffuse light; direct sunlight still uses the dynamic shadow map.

The same data contains a restrained edge-wear mask for painted bevels.
Stable surface variation breaks up the wear, changing colour, roughness and
metal response together. Existing colour, normal and roughness textures remain
in use. Infantry retains its baked character AO and aircraft retain their
authored material paths; all receive the shared daylight probe.

Contact channels are stored in mesh UV1, leaving texture and tangent UV0 intact.
The cache checks a geometry signature and vertex count before use. Changed or
missing meshes fall back to unoccluded shading without edge wear. The cache is
loaded once; no visibility rays or baking run during gameplay. This is bind-pose
contact shading, so it does not replace dynamic shadows as turrets move.

Ship `assets/art_refresh/model_contacts.json` alongside the other game assets.

## Rebuild

Run these from the repository root, with Python/numpy and Blender available:

```powershell
python assets/art_refresh/tools/build_daylight_probe.py
native/target/debug/rule-of-war.exe --export-art .data/model-finish-source
& 'C:/Program Files/Blender Foundation/Blender 5.2/blender.exe' --background --python assets/art_refresh/tools/bake_model_contacts.py -- .data/model-finish-source/runtime_kit.json
cargo build --locked -j2 -p rule-of-war-native --bin rule-of-war
python tools/check_model_finish.py
```

The runtime art exporter now preserves each texture's actual dimensions rather
than assuming 512 x 512 pixels. Contact bake statistics are recorded in
`model-contact-validation.json`. Native GPU captures and their actual camera,
probe and contact-cache diagnostics are under `.data/render-checks/model-finish`.

CPU validation: 320 native tests pass, including probe layout/finite HDR data
and safe contact-cache fallback. Strict Clippy passes. The GPU checks exercise
every ground vehicle material, infantry, both aircraft, town architecture,
Low graphics, DLSS Quality, a combined battle and vehicle destruction.

## Concrete and natural-ground follow-up

The airbase apron and ground-facing concrete use a separate material from walls,
kerbs and roofs. The photographic source is Poly Haven's **Hangar Concrete
Floor**, by Dimitrios Savva, CC0:
https://polyhaven.com/a/hangar_concrete_floor

Original 2K PNG files, API metadata and verified checksums are retained in
`assets/incoming_textures/hangar_concrete_floor`. The earlier Concrete Floor Worn
001 source is also restored at its full delivered 2K resolution for architectural
surfaces. No original source PNGs or archives are overwritten.

Run `assets/art_refresh/tools/import_apron.py` after the general PBR importer to
rebuild these derivatives. Physical texture repeats, channel conventions and
runtime hashes are recorded in `materials/pbr/catalog.json`.

Five-and-a-half-metre concrete pours have shallow-looking expansion joints,
small colour differences, metre-scale mottling, sparse spills, hairline cracks
and tyre scuffs. Thin joints fade with pixel footprint at wider camera distances.
The apron colour combines the lighter supplied worn concrete (65%) and the
hangar scan (35%) in linear light to retain an outdoor concrete reflectance.
Photographic normal and roughness maps provide the finer pores and aggregate.
Only broad, upward ground faces are split into this material; the process
preserves every original triangle and its position. There is no displacement
or collision/navigation change on the apron. Plazas receive the same flat
concrete treatment. Architectural concrete retains its separate wall finish.

The apron maps select existing colour-correct, normalised mip levels at 512,
1024 or 2048 pixels according to texture quality. Full source images remain
available in memory for upgrading quality again. A quality change replaces
texture handles so the GPU bindings cannot retain an earlier-resolution view.
Ground-face UVs account for building scale, preserving stable metre-scale detail
rather than shrinking it with the building model. The scan's two-metre source
detail is displayed over six metres for legibility at the fixed RTS camera;
both source and artistic repeat are recorded in the catalog. This changes the
size of visible surface wear without adding geometric bumps to the apron.

Natural soil gains the measured meadow normal as a larger-scale detail layer
while retaining its own fine grain. The terrain shader weights that extra
normal by natural-soil coverage; asphalt and prepared lanes receive no added
meadow detail. Prepared gravel foundations and concrete use separate materials.
Level natural grass remains rough; the surface's use determines the exclusion,
rather than slope alone.

A procedural-map classification bug is also corrected: biome 4 means natural
plains, and must not select the asphalt layer. Authored battlefield roads and
training lanes retain their explicit masks. A regression check covers natural
plains relief and perfectly flat prepared training lanes.

`tools/check_surface_refresh.py` captures close apron detail, matched ground,
aircraft parking and town views, training, Low texture quality, DLSS, quality
changes during play and a combined battle. Captures and diagnostics are under
`.data/render-checks/surface-refresh`.

Validation for this follow-up: all 323 native tests and strict Clippy pass.
Ten 1080p GPU scenarios pass, including live texture-quality changes, Low,
DLSS Quality, training and combat. The shared terrain normal is byte-identical
to `materials/pbr/meadow_normal.png`. The final concrete detail scale was also
checked again in the close view, aircraft parking, town, Low and DLSS captures.
`surface-refresh-validation.json` records the summary and diagnostic locations.

## Airbase foundation coverage and weathering

The large pale airfield foundation previously retained the ordinary gravel
material, so the earlier concrete pass did not cover it. Ground-facing airfield
foundation triangles now use their own continuous hardstanding variant of the
photographic ground material. Concrete paving retains its slab-joint variant.
Natural gravel, asphalt, wall materials and roofs retain their existing paths.

Both variants combine three weathering frequencies with a warped world-space
field: broad dark deposits, smaller dust variations and sun-bleached patches.
Photographic aggregate, pores, normal and roughness maps remain underneath.
Oil marks have irregular boundaries; tyre scuffs are broken up rather than
forming uninterrupted circles. Weathering also affects roughness. No animation
is used in these fields, so patterns stay fixed to the ground as the camera pans.

The foundation has a separate normalised perimeter coordinate in UV1. Dust and
stains vary along that boundary, softening its colour transition. This remains
opaque ground geometry: there is no alpha-layer sorting or added displacement.
The extra foundation material shares the existing photographic maps and all
three texture-quality levels; no additional large texture set is loaded.

`tools/check_concrete_weathering.py` explicitly checks that airfield foundation
batches receive the new material. Its close view targets the formerly plain
strip between runway and taxiway. It also checks the apron, wider base, town,
Low, DLSS Quality and texture-quality changes during play. Captures are saved
under `.data/render-checks/concrete-weathering`.

Validation: 324 native tests and strict Clippy pass. Seven 1080p rendering cases
pass, including the previously untreated foundation strip, Low, DLSS Quality,
live texture-quality changes and the wider base. The material routing regression
also checks that natural gravel and asphalt keep their separate material paths.

## Metal and painted-armour sheen

Exposed steel and brass now use a smoother response to the existing warm sun
and sky probe. Steel retains its fine roughness map, with a 0.55 material factor
(roughly 0.25 effective roughness for the supplied map), and a lighter gunmetal
colour. Brass uses a 0.60 factor. Dark coated fittings use a 0.72 factor.

Camouflage paint remains predominantly dielectric. Its generated roughness is
now 0.40–0.60 instead of 0.77–0.95, and reflectance is 0.45, giving it a satin
finish. The same photograph still drives its wear and roughness variation.
Geometric paint chips blend towards smoother, more metallic exposed edges.
Existing warm vertex stains raise roughness and reduce metallic response on
dirty running gear and lower hulls. Rubber and fabric retain their matte paths;
rust has a separate rough, dielectric response rather than inheriting steel.

Painted aircraft panels also receive a satin finish. The Apache's paint template
uses a 0.68 roughness factor and 0.42 reflectance. The A-10 livery uses 0.46
roughness and 0.42 reflectance. Parked and flying aircraft share these materials.
No new reflection textures, lighting changes, clearcoat layer or geometry are
introduced. `tools/check_metal_sheen.py` captures the ground vehicles and aircraft
with the same fixed camera, alongside Low and DLSS checks. Results are under
`.data/render-checks/metal-sheen`.

Validation: native build, all 326 tests, strict Clippy and seven GPU render checks passed. Tank and Apache before/after comparisons plus the A-10 were visually reviewed. See metal-sheen-validation.json.

## Field weathering on armour

The vehicle finish now adds irregular dry dust and earth deposits around the
lower hull, wheels and track shoes, with a lighter dusty layer on upward-facing
panels. Dark vertical streaks break up side plates, while the existing offline
contact map places additional grime in recesses and around fittings. Paint
wear on exposed bevels is stronger and broken into irregular chips.

Deposits raise roughness and suppress metal reflections locally. Clean paint
and exposed edges retain the previous satin/metal sheen; glass and lamps stay
on their existing material paths. This is a fixed, used-in-the-field appearance,
not a gameplay damage indicator or accumulating mud simulation.

The shader recovers bind-pose model coordinates from each animated mesh's
transform. Weathering therefore follows moving hulls, turning turrets and gun
recoil rather than sliding through world space. It reuses the existing shader,
contact data and meshes without extra draw calls or texture downloads.

`tools/check_tank_weathering.py` produces matched before/after captures of the
battle tank and support gun, plus carrier, Low and DLSS render checks under
`.data/render-checks/tank-weathering`.

Weathering validation: build, all 326 tests, strict Clippy and five GPU render checks passed (battle tank, support gun, carrier, Low, DLSS Quality). Battle/support before-and-after renders were visually reviewed under matched lighting. See tank-weathering-validation.json.
