MILITARY TEXTURE SOURCES — FIRST DELIVERY
10 original material packs | 2K PNG | acquired 23 October 2026

CONTENTS
Part 1 — Terrain and roads: short green grass, dry patchy grass, compacted earth,
         fine asphalt and fine gravel. Five packs.
Part 2 — Concrete and walls: weathered concrete, neutral plaster and worn brick.
         Three packs.
Part 3 — Roofs: weathered terracotta and grey slate. Two packs.

Extract the three outer delivery ZIPs to the same directory. Their shared
Military_Textures_Batch01 root merges. Each original material has its own
folder containing a PBR ZIP and source.txt with source, licence, physical
repeat size, exact map names and optional 4K URLs. The inner ZIPs can stay zipped.
All packs have colour, OpenGL/+Y normal, roughness, AO and displacement PNGs.
ambientCG original ZIPs are untouched. Poly Haven ZIPs contain unchanged
individual original PNGs assembled into one archive per material.
Original bit depth is retained. Bricks085 is 2048 x 1024; other maps are 2048 square.
Terracotta's supplied opacity map is retained. There are no paid assets.

LICENCES
All selected assets are CC0 1.0 Universal. Commercial use and redistribution
are permitted without required attribution. Provider confirmation and CC0 links
are recorded beside each pack. Creator names are retained as useful provenance.

IMPORTANT SOURCE LIMITS
- Roof stock tile dimensions beneath overlaps are not specified by the suppliers.
  Notes distinguish supplier repeat sizes from estimated visible tile exposure.
- Roof Slates 03 supplies an effectively uniform roughness map (approximately 1).
  Its colour and normal vary; additional roughness wear must be tuned later.
- Grey Plaster 03 is a visual render/plaster match; lime chemistry is not claimed.
  Its current page and numeric metadata say 1 x 1 m, while a legacy API scale
  string says 1.5 x 1.5 m. We use 1 x 1 m and record the conflict.
- Fine gravel is neutral grey. Blend earth/dust during integration for dusty roads.
  Slate is aged grey; darken through the material tint to suit the final palette.

INTEGRATION
Use the supplied *_NormalGL.png or *_nor_gl_2k.png normal, not NormalDX.
Albedo/colour should be sampled in sRGB. Normal, roughness, AO, masks and height
are data maps and should be sampled as linear / non-colour data.
ORM is intentionally left unpacked: R=AO, G=roughness, B=metallic.
These ten surfaces are dielectric: use B=0 for all of them.
Avoid baking AO into the colour a second time. Preserve roof map alignment.
Keep the source repeat in metres when setting UVs; rectangular brick must keep
its 2:1 aspect ratio. A common terrain coordinate system still needs per-material
scale factors. Tune tint, normal strength and surface response under the shared
game/Blender lighting. Blend grass/dirt/gravel with masks or terrain weights.
These are source assets, not runtime-size recommendations. Choose runtime
resolution, mipmaps and GPU formats per material in a separate integration pass.
No runtime ORM generation, recolouring, resizing or normal conversion was applied.
Only the JPEG contact previews are resized; they are not material maps.

4K OPTIONS
4K masters are available from the recorded source links; this delivery contains
2K PNG packs only. Higher resolutions and additional material families were left
for later batches. No 8K or larger files were downloaded.

VALIDATION
ZIP CRCs and required map presence checked for each pack. Every delivered
material PNG was decoded for validation and checked for 2K dimensions. Poly Haven
download bytes were checked against the provider MD5. SHA-256 checksums and map
metadata are in manifest.json. Source colour previews were visually reviewed.
Seamless source repeats were selected; the final in-engine UV scale, tint,
lighting response and terrain transitions have not been tested in your project.
