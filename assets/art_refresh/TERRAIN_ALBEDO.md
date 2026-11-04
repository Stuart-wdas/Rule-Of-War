# Terrain albedo refresh

The shared terrain material now uses `materials/pbr/field_meadow_v1_albedo.png`
for grass, fine soil and gravel detail. It applies to the main maps and training
ground. Restart the native game to reload it.

The new image was created with the built-in OpenAI image generator. The exact
prompt and generation details are in `terrain-albedo-v1.json`. Its actual source
resolution is **1254 × 1254**, copied unchanged into the project. It is an
AI-generated base-colour asset, not an additional Poly Haven scan.

The physical repeat is 22 metres, chosen after close-camera render checks. The
initial shader blended 50–65% of the new surface with 35–50% of the previous aerial meadow
scan, retaining larger natural changes across the landscape. Road and soil
coverage, lighting and explosion assets retain their existing behaviour.

The companion normal map is neutral and the ORM map is constant matte,
unoccluded and nonmetallic. These are technical defaults, not measured material
maps. The aerial layer still supplies its own matching normal and roughness.
This pass adds colour detail; it does not add grass geometry or terrain relief.
The subsequent refresh in `WORLD_DETAIL.md` adds measured relief, geometry,
tracks and a revised 40–58% generated / 42–60% scanned blend.

Existing Low, Balanced and High texture settings control GPU texture sizes and
mipmaps. High cannot create detail beyond the source image resolution. The same
four terrain layers are used, so no additional texture array or draw call is
needed.

`tools/install_generated_meadow.py` refreshes this entry and its technical maps.
Use `--source path/to/new.png` to install a replacement source deliberately. The
previous grass and meadow source assets are preserved.
