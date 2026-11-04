# Moving cloud shadows and vegetation models

The sun now projects a seamless, softly filtered cloud-transmission mask across
the world. Grass, fields, roads, building roofs and ordinary PBR foliage receive
the same moving pattern. Only direct sunlight is modulated: the cool sky fill
and ambient light remain present in the shade. The darkest part of a cloud
transmits 45% of the sun, making the passing patches darker while retaining sky
and ambient illumination in the shade.

The pattern is generated once as a 512-pixel linear texture. A world-space wind
of `(1.65, 0, 0.58)` metres per second translates the projection, independent of
camera position and terrain chunks. Sun direction and object-shadow cascades
retain their original orientation. Pausing the game also pauses cloud travel.
The light-plane pattern repeats over 768 metres; its noise wraps continuously.

This uses Bevy 0.18.1's `DirectionalLightTexture` and `pbr_light_textures` feature.
It adds one shared mask rather than extra transparent planes or per-tile meshes.
Adapters without native binding-array support retain ordinary sunlight, following
Bevy's built-in light-texture fallback. Low graphics still uses the same small
mask where supported. `--no-cloud-shadows` restores a clear-sun comparison.

## Tree and bush changes

- Plain cylinder trunks become tapered, slightly bent stems with a flared foot,
  longitudinal bark UVs and tangent data for normal maps. Scaled forks reuse the
  same geometry.
- Crossed diamond leaf cards become slightly curved, five-vertex leaf sprays.
  Their photographic UVs stay inside atlas cells, and blended canopy normals
  soften individual card lighting.
- Broadleaf crowns are less tall and regular, with varied lobe offsets and
  proportions. Conifers keep their layered outline.
- Shrubs use their own lower, irregular cluster shape instead of a reused tree
  crown. Existing planting positions and streaming/mesh-upload budgets remain.
- Full-detail broadleaf/conifer leaf meshes retain their previous 880/680-vertex
  budgets; distant crowns use 180 vertices. Curved sprays use additional triangles.

The existing photograph atlas contains broad leaves. A further species pass
with dedicated pine-needle sprays, unique branch textures and a few authored
old-tree silhouettes would provide the largest remaining visual improvement.

## Review and validation

`review.jpg` combines native captures of clear sun, cloud phase 0, cloud phase
120 seconds, and a closer vegetation view. `validation.json` records measured
ground shading and movement from those captures. The source code is in
`native/src/cloud_shadows.rs`, `surfaces.rs`, `natural_geometry.rs` and `scenery.rs`.

Run `tools/check_cloud_foliage.py` with the bundled Python to repeat the GPU
checks, including Low graphics and the Training Ground. `--cloud-time 0` and
`--cloud-time 120` freeze the weather pattern for reproducible captures.
`--environment-town --foliage-close` frames a closer patch of the actual plants.

Regression tests cover seamless bounded attenuation, paused motion, unit plant
normals, valid atlas cells, bark tangent streams and the retained LOD budgets.

Validation for this pass: all four new regression tests and six native capture
scenarios passed. The full suite ran 382 tests: 381 passed; the existing
`models::tests::town_lane_follows_the_ground_with_upward_faces` height assertion
failed in the concurrently revised settlement-road code. Strict Clippy also
reported existing style issues in `models.rs`, `rural_layout.rs`,
`rural_surfaces.rs` and `aircraft.rs`. No cloud or foliage lint failures occurred.
