# World material and model refresh

The base game and training ground use the same material profile. The approved
Mantaflow explosion assets, fixed camera and shared Blender/world lighting
profile are preserved.

## Material treatment

- Existing supplied photographic PBR maps keep their physical repeat distances,
  colour space, normal maps, roughness, occlusion and filtered mipmaps.
- Terrain has world-anchored meadow colour variation and broken soil/verge
  transitions. The pattern is identical across terrain detail levels.
- Architecture has additional vertices on broad faces for damp foundations,
  rain streaks and roof staining. Subdivision is bounded and performed when
  geometry is prepared, rather than per frame.
  Existing tangent data is interpolated through subdivision, avoiding a second
  shading-basis calculation when damaged houses rebuild.
- Colour-only metal, rubber, fabric, timber, bark and stone get small procedural
  normal/roughness maps. True supplied PBR maps take precedence. Fabric stays
  nonmetallic, while bare steel has its own metallic response.
- Armour retains distinct olive/desert camouflage with small paint chips, lower
  hull dirt, darker undersides and subdued gunmetal/rubber fittings. Aircraft
  paint keeps its relief maps instead of discarding them.

## Geometry and atmosphere

- Vehicles gain layered side panels, mud flaps, tow fittings, rear stowage and
  turret equipment, within the existing rigid mesh batches and animation pivots.
- Buildings gain electrical meters, conduit and entrance lamps, retaining the
  existing wall cutaways, doors, damage and rubble behaviour.
- Sniper shoulder cloth is a folded mesh rather than rectangular blocks. The
  complete rendered infantry instance must still remain below 7,000 triangles.
- Trees receive shadows and have branch structure and low growth around their
  bases. Boulders use a more detailed, smoothly shaded deformed mesh. Garden
  clearings follow individual building wings rather than large empty discs.
- Explosions push short, low dust bursts across the ground. Moving vehicles
  produce light dust; low helicopters produce downwash. Motion effects are
  camera culled, rate limited, suppressed on water and paused with the game.
  Combat effects receive the particle allowance before ambient motion.
- Infantry walking is silent. Weapon, order and vehicle audio remains active.

## Verification and viewing

`View Visual Demonstration.cmd` opens the shared art study; the same materials
are applied during a normal match and in the training ground. For close unit
inspection, the native executable supports `--infantry-models --smoke-test
<capture.png>` and `--models --smoke-test <capture.png>`.

Run `tools/check_realism_refresh.py` with the bundled Python runtime for town,
ground, model, infantry, training and explosion captures, followed by an actual
30-second combined-arms battle before/after comparison. It writes screenshots,
native timing reports and external process CPU/GPU recordings under `.data/`.
The comparison requires the preserved pre-change executable in the capture
directory; capture tests themselves use the current native executable.

Source changes are in `native/src/surface_wear.rs`, `surfaces.rs`,
`grass_blend.wgsl`, `scene.rs`, `vehicle_models.rs`, `building_models.rs`,
`infantry.rs`, `scenery.rs`, `landscape.rs`, `effects.rs` and `sound.rs`.
New relief textures are original procedural assets; source photo credits remain
in `materials/pbr/catalog.json`.

## Recorded check, 24 October 2026

309 Rust tests passed, along with strict Clippy and formatting checks. Rendered
checks covered the shipping town, field, vehicle assemblies, all infantry
classes, training ground, explosion study and combined-arms combat.

The final sequential 30-second battle comparison at 1100 × 850 with Balanced
VFX measured median frame time of 13.64 ms before and 14.26 ms after; 95th
percentile was 18.14 ms before and 16.92 ms after. The final run's slowest frame
was 118.62 ms; no multi-second combat pauses occurred in this short check.
Effects update stayed below 0.51 ms, including the added dust. Median total
machine CPU usage was about 16% for both runs; peak process working set rose
from about 2.45 GiB to 2.56 GiB. These are short development-build measurements,
not a long battle soak. Raw reports and CPU/GPU samples are retained in
`.data/render-checks/realism-refresh/` and `.data/battle-bench/realism-*`.
