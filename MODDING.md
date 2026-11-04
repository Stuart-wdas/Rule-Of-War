# Modding and architecture

The active application is a Rust package in the root Cargo workspace. Gameplay
runs at 20 ticks per second; Bevy systems render snapshots and send commands.
Multiplayer uses the same simulation on the native host.

| File | Responsibility |
|---|---|
| `native/src/main.rs` | Bevy startup, camera and tactical input |
| `native/src/cursor.rs` | Shared hover/click targeting, contextual tactical cursors and hints |
| `native/src/battlefield.rs` | Fixed 512-metre core battlefield, landforms and dirt routes |
| `native/src/training.rs` | Fixed practice range terrain and staging position |
| `native/src/training_simulation.rs` | Practice purchases, targets, counters, resets, healing and separate save |
| `native/src/fog.rs` | Enemy-unit fog while the map remains visible |
| `native/src/simulation.rs` | Commands, movement, combat, production, economy and saves |
| `native/src/tactics.rs` | Cover, suppression, wounds, retreat, steering and battlefield destruction |
| `native/src/destruction.rs` | Deterministic source-model wrecks, hull footprints, fractures and blast falloff |
| `native/src/feedback.rs` | Immediate order markers and queue overlays |
| `native/src/sound.rs` | Weapon-specific PCM cues, footsteps and movement engine loops |
| `native/src/projectiles.rs` | Interpolated visible tracers, shells and rocket arcs |
| `native/src/animation.rs` | Shared joint poses for gait, aiming, recoil and medical work |
| `native/src/specialists.rs` | Bridges, patrol and formation strength |
| `native/src/settlements.rs` | Settlement generation and street blocking |
| `native/src/settlement_layout.rs` | Seeded freestanding houses and rotated collision footprints |
| `native/src/runtime.rs` | Fixed ticks, command queue and render snapshots |
| `native/src/generation.rs` | Seeded procedural terrain |
| `native/src/terrain.rs` | Binary terrain, sampling and terrain modifications |
| `native/src/editor.rs` | Brush stroke undo/redo |
| `native/src/world.rs` | Strategic ownership, adjacency and supply |
| `native/src/checkpoint.rs` | Legacy JSON checkpoint import |
| `native/src/network.rs` | Native host authority, guest commands, reconnect and persistence |
| `native/src/frontend.rs` | Map selection, globe, editor, configuration and lobby |
| `native/src/menu.rs` | Title screen, operation cards, preferences and field manual |
| `native/src/loading.rs` | Background operation generation/import and atomic replacement |
| `native/src/theme.rs` | Embedded fonts and presentation colors |
| `native/src/landscape.rs` | Background smooth terrain, shorelines and shared nature assets |
| `native/src/scenery.rs` | Fixed world-grid vegetation, retained chunks, view coverage and batched tree meshes |
| `native/src/models.rs` | Procedural unit, building and settlement geometry |
| `native/src/building_models.rs` | Segmented walls, door/window openings, roof sheets and architectural detail |
| `native/src/garrison.rs` | Reserved windows, routed doorway crossings, exit movement and firing arcs |
| `native/src/military_models.rs` | Detailed ground vehicles, facilities and airfield |
| `native/src/bases.rs` | Fixed base plans and retired roster import cleanup |
| `native/src/base_scene.rs` | Persistent base service roads and perimeter dressing |
| `native/src/surfaces.rs` | Material grain, UV projection and irregular foliage |
| `native/src/effects.rs` | Layered pressure bursts, wreck fires, transient lights and bouncing model fragments |
| `native/src/camera_drag.rs` | Cursor-anchored map dragging and click threshold |
| `native/src/scene.rs` | Lighting, actor interpolation, animation and tactical overlays |
| `native/src/ui.rs` | Status and production actions |
| `native/src/tools.rs` | Headless Rust terrain and performance commands |
| `native/catalog.json` | Unit and building balance data |
| `native/config.json` | Configuration defaults and limits |

Balance and configuration JSON are embedded at compile time. Edit those files
and rebuild with `cargo run --locked`. For a new unit or building, update the
catalog, relevant simulation rules, available actions in `ui.rs`, and its mesh
in `models.rs`. Extend the starting-force configuration where appropriate.

Operation loading and terrain mesh preparation use worker threads. Apply game
state on the main thread only after a successful load; geometry readiness gates
simulation/input. Render actors from the faction-filtered `Battle` snapshot.
The UI theme embeds licensed fonts; attribution is in
[native/assets/CREDITS.md](native/assets/CREDITS.md).

Native town lots persist in saves and faction snapshots. Geometry, collision,
cover searches and click targets use these same lots. Capture belongs to the settlement. Each house has persistent `integrity`;
collapsed houses become rubble and leave collision/cover. Damage scales settlement
income by surviving integrity. Unit activity/aim snapshots drive
render poses; incoming-fire reactions and medical treatment remain host-authoritative.

New operations use a mirrored six-facility base plan, graded into the terrain.
`footprintWidth`, `footprintDepth` and `worldScale` in the catalog govern collision,
scenery clearing and rendering. Naval entries are retained only for old save
reading and frozen routing tests; `retired` excludes them from active rosters,
and imports remove retired units/facilities and refund their pending production.

The combat map is the fixed Forward Base defined in `battlefield.rs`; the
practice map is defined in `training.rs`. Both are hydrated in `terrain.rs`.
`menu.rs` exposes the combat operation and Training Ground. Training uses the
same authoritative combat, cover, construction and production rules, with
stationary targets, no AI commander and no victory condition. Training state
and target positions persist in a separate save. Old binary map presets,
campaign and network data remain for compatibility and developer checks.
Generate experimental terrain with `--generate-terrain <file> <cols> <rows>
<seed>`, or use the in-game editor and export authored map JSON with F4. The
terrain CLI writes the fields consumed by the native renderer and simulation;
legacy river-flow and downstream arrays are not exported.

Use `cargo test --locked` for simulation, persistence and networking checks.
Frozen data in `native/tests/` comes from the archived browser implementation
and remains an independent comparison for migration. Cargo does not regenerate
it or invoke Node. Add behavior tests when changing simulation rules; use the
documented native screenshot smoke command for rendering changes.

`aircraft_models.rs`, `aircraft.rs`, `garrison.rs`, `checkpoint.rs`, `specialists.rs`, `tactics.rs`, `training_simulation.rs` and `settlements.rs` are currently included by
`simulation.rs`; format those files explicitly with `rustfmt --edition 2024`
when editing them. The obsolete browser sources and tooling have been removed;
the independent JSON fixtures remain available to native regression tests.

## Materials to find and label

Create a folder named `materials` in the repository root, next to `Play Native.cmd`.
The game reads these PNG files when it starts. Short names such as `brick.png`
are accepted as base colour, and `painted_steel.png` is an alias for
`painted_metal_albedo.png`. Canonical `_albedo.png` files take precedence.
Restart after adding or replacing
textures; a rebuild is not needed. Missing maps keep the procedural finish.

Start with the six building materials below. Look for seamless, tileable PBR
material images photographed straight on, without baked lighting, perspective,
lettering or objects. Use muted, weathered surfaces rather than polished finishes.
1K or 2K PNGs are a good starting size; all maps in a set should use matching UVs.

| Label / file prefix | Texture to find | Used for |
|---|---|---|
| `plaster` | Weathered off-white lime plaster or stucco, subtle stains and fine cracks | Plastered house walls |
| `brick` | Aged red/brown brick with pale mortar, regular courses, no graffiti | Brick walls and chimneys |
| `roof` | Worn dark grey slate tiles or muted clay shingles, with distinct rows | Roof sheets, caps and porch roofs |
| `wood` | Weathered timber boards, visible grain, subdued brown, no dramatic lighting | Doors, shutters, frames and porch posts |
| `concrete` | Aged matte concrete with small pits and light grime | Foundations, sills, lintels and facilities |
| `steel` | Dull galvanised steel, slight scratches and light corrosion | Gutters, downpipes, beams and metal equipment |
| `painted_metal` | Matte olive military paint with modest wear | Faction-tinted painted parts |
| `cloth` | Muted khaki/olive woven canvas, small weave, no logos | Uniforms, equipment and canvas |
| `soil` | Neutral, low-contrast dry soil and small pebbles; avoid strong colour | Ground detail, multiplied by terrain biome colours |
| `bark` | Rough vertical bark, seamless in both axes | Tree trunks |
| `stone` | Weathered grey rock, no identifiable large objects | Rocks |
| `grass` | Seamless short grass, overhead view | Grass terrain |
| `gravel` | Seamless compacted gravel, overhead view | Roads, settlement lanes and range lanes |
| `asphalt` | Seamless weathered asphalt without painted markings | Airfield runway, apron and taxiways |
| `rubber` | Seamless dark tyre rubber | Vehicle and aircraft tyres |
| `aircraft_paint` | Seamless neutral grey aircraft paint, fine wear | Faction-tinted A-10 and Apache bodywork |
| `leaves` | Transparent foliage atlas, three columns by four rows | Tree foliage cards |
| `scorch` | Transparent blast-scar atlas, largest scar in upper-left area | Ground blast marks |

For each prefix, name the maps as follows (brick is the example):

- `brick_albedo.png`: base colour. This is the only map needed to start.
- `brick_normal.png`: optional OpenGL / Y+ tangent normal map. Load in linear colour.
- `brick_orm.png`: optional packed map: **red = ambient occlusion, green = roughness,
  blue = metallic**. Load in linear colour. Brick, wood, plaster and soil should
  have black metallic channels. A plain grayscale roughness image is not an ORM map.

Do the same for `roof_albedo.png`, `roof_normal.png`, `roof_orm.png`, and the other
labels. If a download only offers separate AO, roughness and metallic maps, keep
those original files too; they need packing before being named `_orm.png`.
Use images whose licence allows use in your game and keep the source/licence text
with each set. The loader currently accepts PNG, not JPG or EXR.

The current procedural finishes already differentiate mortar, roof courses,
wood grain, canvas and concrete. External albedo maps replace those patterns;
normal/ORM maps add surface relief and material response without changing geometry.
Weapon `terrainDamageRadius` is independent of splash radius: only artillery,
rockets and bombs opt in. Actual ground impacts avoid intact structure, vehicle
and wreck footprints, and an existing crater prevents repeated digging in one spot.

### Aircraft tuning

`native/src/aircraft.rs` owns saved flight phases, runway reservations, servicing
and ballistic releases; `airfield_layout.rs` shares stand coordinates between
simulation and models. In `native/catalog.json`, bomber `cruiseAltitude` is 72 m,
`bombPayload` is six and `bombIntervalSeconds` is 0.3. Aircraft `fuelCapacity`,
`fuelBurnPerDistance`, `fuelBurnPerSecond`, `returnFuelReserve` and `refuelSeconds`
control endurance and recovery. Bomb `damage` and `damageRadius` control splash;
`terrainDamageRadius` separately opts bombs into safe ground craters. Medium tank
shells still do not deform terrain. Stand types and counts must be changed in
both the shared layout and the catalog capacity when extending the airfield.


Aircraft silhouettes are authored in `native/src/aircraft_models.rs`: the bomber
uses an A-10-style fuselage, straight wings, rear turbofans, twin vertical fins,
forward canopy, nose cannon and underwing bomb racks. The helicopter uses an
Apache-style tandem cockpit, engine shoulders, sensor turret, Longbow radar,
weapon wings, fixed wheeled gear and separately animated main/tail rotors.
`Airframe` and `Airfoil` are reusable meshes. Bodywork uses `aircraft_paint`,
with steel fittings, rubber tyres and dark glass. Aircraft paint is tinted grey
for friendly bombers, olive for friendly helicopters and tan for enemy aircraft.
Mounted bomber bombs follow its actual remaining payload; its unit role and
sortie simulation remain the same.

Visual references: [USAF A-10 fact sheet](https://www.af.mil/About-Us/Fact-Sheets/Display/Article/104490/a-10c-thunderbolt-ii/)
and [Boeing Apache](https://www.boeing.com/defense/military-rotorcraft/ah-64-apache).

### House layouts

`native/src/settlement_layout.rs` shares the occupied wings, exposed wall edges,
doors, windows, collision and health for Square, L, H and straight houses. Town
and training houses use one storey (3.2 m walls) and matching 10 x 10 m plots;
the shell has an 8.4 m outer envelope with open recesses. The low two bits of
`Lot.style` choose the plan and the next two choose the wall finish. Facilities
retain their own dimensions. Legacy saves migrate house sizes and release
affected occupants beside their doors, preserving unit health and house damage.


The loader derives mip levels at startup without editing the supplied PNGs.
`native/src/texture_filter.rs` averages colour in linear light, weights transparent
colour by alpha, and renormalizes normal maps. Samplers use linear minification,
magnification and mip filtering with 16x anisotropy. Keep original PNGs at their
full resolution; pre-blurring or replacing them is unnecessary.

`Part.roof_lot` identifies house roof and upper-wall cutaway groups (facility roofs use `usize::MAX`).
`RoofLayer` switches shared transparent materials by actual visible occupancy,
without rebuilding the model. Roofs use 12% opacity and stop casting shadows
while occupied. `Wreck.crash` persists an airborne helicopter's world height,
velocity, vertical speed and tumbling state. Its final impact has 5.5 m splash
radius and 2.5 damage; it does not deform terrain.

Grass uses `native/src/grass_material.rs` and the embedded `grass_blend.wgsl`
shader: filtered texture patches blend with world-anchored offsets, rotations and
broad colour noise. User PNGs remain unchanged. `natural_noise.rs` controls
vegetation clustering; scatter competes with neighbouring cells across chunk
boundaries to keep trees apart. `settlement_layout.rs` owns town roads and house
plots; `simulation::CAPTURE_RADIUS` is shared by capture logic and its overlay.

Vehicle navigation is in the included `native/src/navigation.rs`. It indexes
rotated building wings and wrecks in spatial buckets and uses a clearance radius
from each vehicle's scaled hull dimensions. Routes, shortcuts, movement,
separation and render prediction share that clearance. The cache is omitted from
saves and rebuilt after obstacle geometry changes. Old partial overlaps recover
by driving outward while retaining the movement order.

TownPlan in `settlement_layout.rs` saves the H/L/E street layout and its objective
plaza independently of the house shapes. New operations include one of each town
plan in a shuffled order. Grass and vegetation clearings follow the streets and
plots rather than painting a square around every settlement.

`--profile` enables wall-time sampling in `performance.rs`; render-check JSON
contains per-stage median, 95th/99th percentile and maximum times. Worker labels
identify background mesh work. `--benchmark-navigation` measures the same four
route requests around a sixteen-house test town without the renderer.

Building damage uses `destruction::damage_stage` and `damaged_parts`: eight stable geometry stages
from saved house integrity or facility HP. New facade/roof finishes reuse plaster and roof
textures with explicit tints. `Snapshot.viewer` keeps roof cutaways tied to the viewing faction;
`snapshot_visible` hides hostile interiors without removing window targets from combat.
Capture timing constants live beside `CAPTURE_RADIUS` in `simulation.rs`.

`native/src/flipbook.rs` imports the completed explosion and smoke sheets from
`assets/Realistic_Explosion_VFX/` at build time. The source collection, manifests,
licenses and rebuild scripts stay intact. Explosion uses the 4096 atlas and smoke
uses the two 2048 atlases. Frames are split in top-left row order, with independent
alpha-aware mip chains and clamp filtering to prevent frame bleed. Materials use
unlit straight-alpha blending to preserve the prelit fire and dark smoke. Playback
is non-looping, follows particle lifetime, freezes when paused, and resets with
existing effect cleanup. Frames and materials are shared across explosions;
no per-impact texture decoding or material allocation occurs. Terrain dust remains
procedural; the pack's provisional dust is not shipped as a completed simulation.

Heavy blasts use `CapMotion` in `effects.rs`: smoke-textured lobes rise, spread and
curl around a torus over a narrow pair of anchored plume sprites. Late fireball
frames supply cap turbulence and embers; cap and stem lifetimes are about 6-7 seconds.
Only the sprite transforms and animation phases change; texture/material frames
stay shared and the existing particle budget still applies.
