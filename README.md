# Rule of War

Rule of War is a Rust/Bevy desktop strategy game. Rendering, gameplay, menus,
terrain, and saves run in the native executable.

Read [GAME.md](GAME.md) for a plain-language guide to the game, its mechanics
and art direction.

The airfield now offers a **UH-1Y Venom** carrying **12 troops** for **24 supply**.
Use **L** to load, right-click to fly, and **U** to land and unload. The same unit
is available in the training shop. [Huey photographs and modelling notes](assets/aircraft/huey/README.md).

The next material pass is listed in [TEXTURE_SHOPPING_LIST.txt](TEXTURE_SHOPPING_LIST.txt).
Put original PBR downloads under `assets/incoming_textures/`, one pack per folder.

**Graphics:** open Escape → Settings for per-computer Low/Balanced/High presets,
terrain texture size, environment density, shadows, explosions and optional NVIDIA
DLSS Super Resolution. Native rendering remains the default. Both the training
ground and battlefield use the scanned meadow and worn-road surfaces. See
[graphics details and build setup](assets/art_refresh/PHOTOGRAPHIC_WORLD.md).

Soft cloud shadows now drift across the ground, roads and buildings, following
the sun and pausing with the game. Trees have tapered, bent trunks and less
regular crowns; bushes use lower, fuller clusters and curved leaf sprays.
See [the atmosphere and vegetation review](assets/art_refresh/cloud-foliage/README.md)
for native captures and comparison controls.

Explosion ground marks now use irregular disturbed soil, low broken rims and
scattered clods, with sizes matched to shells, artillery and rockets. They follow
the displayed terrain through detail changes and nearby crater edits, and keep
their variation in saved games. See [the impact-mark review](assets/art_refresh/impact-scars/README.md).

Sun shadows now follow the visible ground through pan and zoom, including the
full-map view. Thin, capped clear-air haze separates distance without hiding the
battlefield. Fields reuse the existing materials for dry patches, planted rows,
tractor tramlines and headlands; row shading fades smoothly below pixel size.

House walls and roofs now keep their material scale and alignment when turned;
roof tiles follow the slope and ridge. Meadow/stony detail and road aggregate
use finer physical scales. Household plots reuse the existing garden kit for
fences, vegetable beds, benches, timber stores, side paths and worn wall edges,
with four layouts per town and open road and doorway access.

Double-click **Play Native.cmd**, or run from this directory:

```powershell
cargo run --locked
```

The launcher builds into `native/target` and runs a temporary executable copy.
This lets you rebuild while a previous game is open. The copy is removed when
the game closes; saves and supplied materials stay in their usual locations.

**Visual demonstration:** double-click **View Visual Demonstration.cmd** to open
Saint-Orne, a six-house summer art-direction study. It uses the pinned
semi-orthographic camera, warmer directional light, quieter building
materials, planted gardens and the new layered Blender effects. The effects
cycle automatically. **1 / 2 / 3 / 0** trigger shell impact, artillery, vehicle
destruction and rocket impact; **Enter** repeats the selected effect; **R** restores the scene.
**B** compares lighting/material treatments, **H** reveals ordinary unit orders, and **F6**
cycles effects quality. WASD pans and the wheel selects zoom levels. Its save
file is `native-visual-study.json`, separate from battle and training saves.
The battlefield and training ground now share the chosen palette, pinned camera
and hybrid Blender effects. Panning and zooming remain available; orbit is disabled.

Double-click a unit's roster card to focus it. **BASE / RETURN [Home]** jumps to
your base and back to the previous view. To review the Blender-authored fiery
mushroom and spiky debris blast, double-click **View Explosion Preview.cmd**.
Its offline player has playback, frame scrubbing and a side-by-side comparison.
The current effects are four weapon-specific Cycles bakes, with brown shell
dirt, brief artillery/rocket ignition and darker vehicle fuel smoke, matched to
the effective warm map lighting. Open **View Explosions In Blender.cmd** for the
editable four-scene gallery. Photographic references, previews and rebuild notes
are in [the daylight impact pack](assets/Realistic_Explosion_VFX/daylight_impacts/README.md).
Earlier effects remain in `assets/Realistic_Explosion_VFX/reference_blasts/`;
aircraft sources are in `assets/aircraft/a10/`.

Build requirements: Rust 1.94+, Visual Studio C++ Build Tools and the Windows SDK.
See [native/README.md](native/README.md) for controls, saves and
the terrain editor.

The title screen's **New Battle** action starts the fixed
2048 x 1024 metre Forward Base against the computer. Campaign, multiplayer and
alternate maps are unavailable in the player flow during core-loop development.
Start a fresh battle to use the revised scale and starting forces.

Fresh battles now use a compact countryside settlement in the central town's
former footprint: twelve village houses, two outlying three-building farms,
a bending main road, village lanes, crop fields, pasture and planted hedgerows.
The same countryside styling continues across the full battlefield: larger
irregular crop and stubble plots, gated hedgerows, connected farm tracks and
woodland belts, with clear space around both airfields and deployment areas.
The asphalt main road continues to both map edges. Roads and farm tracks use
continuous surfaces with the supplied materials, and field boundaries retain
their shape independently of terrain detail and camera zoom.
The 2048 x 1024 battlefield adds distinct western and eastern hamlets and two
more three-building farmsteads, for five capture points across three towns.
Surrounding countryside extends 2 km beyond the playable edges. The camera's
pan and zoom limits account for the viewport so the visible landscape stays
under every screen corner; orders and units remain inside the playable map.
The layout reuses the supplied house, foliage and ground assets. Existing saves
retain their saved house layouts; start **New Battle** to see the rebuilt map.

Choose **Training Ground** for a dedicated mechanics test map. Buy units
instantly at normal supply prices, place targets and cover, toggle return fire
or fog, refill supply, and test medic rescue. Press **T** for the shop/tools and
scroll their panels for more controls. The range has no victory condition;
its save is separate from the combat operation. See [GAME.md](GAME.md#training-ground).

Choose **Simulation / AI vs AI** to watch both factions command, recruit, and
fight for the same battlefield. The full battlefield is visible to spectators;
each AI keeps its normal sight rules. Pan with WASD or drag, zoom with the wheel,
press **Space** to pause/resume, **F2** for a fresh simulation, and **Escape** for
the menu. **F5/F9** save/load `native-simulation.json`, separate from your battle
and training saves. **Continue Simulation** resumes that save from the menu.

Sight scales with map dimensions for both factions and both operations. On the
1024-metre maps, assault infantry see 64 metres, tanks 144 and snipers 224. Weapon
reach is independent of sight distance. Buildings and hills obstruct spotting;
existing vegetation provides concealment based on its height and thickness.
Movement and recent fire make partial concealment less effective. Friendly units
share spotted contacts. Fog hides enemy soldiers and vehicles outside sight. Terrain, scenery, buildings and
the minimap stay visible; atmospheric haze is capped at 8% opacity. Selected units
display their authoritative sight radius.

Infantry can enter houses and facilities, reserve separate windows and change
windows to face targets with a clear firing line. Town and training houses use
one storey and common 10 x 10 metre plots with Square, L, H and straight plans. Right-click a friendly or
neutral building to enter, **G** finds a nearby building, and **U** leaves.
Large shells deal area damage. House health scales with size, and facilities
withstand repeated hits unless one projectile exceeds their remaining health.
Vehicles and turrets turn at rates suited to their type and size. Infantry falls
back under pressure and changes blocked or heavily suppressed firing positions.
Select any mix of units and press **Ctrl+1-9** to save a squad; **1-9** recalls it.
Squads persist in saves, and **Shift+1-9** adds a squad to the current selection.

Each infantry unit uses the rigged Blender soldier with camouflage, a helmet,
plate carrier, compact pack and M4. Shared mesh and texture assets give each
soldier an independent skeleton. Movement, cover and firing blend the authored
clips; torso aim and barrel sockets follow the current pose. Medics are unarmed
with medical crosses, and support soldiers put away their rifles while working.
Faction patches and uniform tints identify opposing soldiers.
Orders reserve destination cover within eight metres and hold it after arrival.
Projectile sockets follow the actual rendered barrel parts, including recoil.

The fixed terrain has rolling ridges, fields, dirt routes and woodland rather
than randomly generated hills. Weapon effects use animated model sockets:
rifle tracers, cannon rounds, artillery arcs, rocket exhaust and A-10 cannon strafes.
Use WASD or middle/right-drag to pan, the wheel to zoom, C to focus selection,
Q for attack-move, R to retreat, and Escape for the menu. F8 opens terrain editing.

Development commands:

```powershell
cargo build --locked
cargo test --locked
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked -- --benchmark-scale
cargo run --locked -- --generate-terrain .data/custom.terrain 128 128 my-seed
cargo run --locked -- --prebuild-terrain .data/generated-terrain
```

Terrain tools run without opening a window. Prebuilding writes native terrain
binaries for the three map presets into the directory you supply. Existing
embedded assets remain in `assets/terrain/`.

The obsolete browser edition and npm tooling have been removed. JSON files
contain embedded game data and frozen migration test fixtures; the fixtures
remain part of the native regression tests.

Feature parity is still being completed; [native/PARITY.md](native/PARITY.md)
records remaining gameplay and presentation work. See [MODDING.md](MODDING.md)
for the Rust architecture and extension points.


Surface textures use mipmaps and linear/anisotropic filtering to reduce shimmer
while panning. Buildings occupied by your own infantry reveal their roofs and upper walls, restoring
them when the occupants leave. Destroyed airborne helicopters tumble and burn
on their way down, then explode on ground impact and leave a persistent wreck.

The battlefield is 2048 x 1024 metres on new operations. Tanks reload for 4.5 seconds.
Captures unlock after two minutes and require 25 uncontested seconds with armed infantry;
enemy posts must be neutralized before securing. Armour can contest but cannot capture.
All posts must stay uncontested for a 90-second victory hold. The AI rallies infantry
and armour, keeping support weapons behind the assault. Enemy building interiors stay
hidden even without fog. Buildings show cumulative chipped walls, torn roofs and rubble
before collapse. Facades and roofs reuse supplied textures with varied colour tints;
rockets and vehicle destruction use the supplied animated fireball and smoke sequences.

The visual refresh adds continuous building facades, pitched facility roofs,
roof timbers and footprint-shaped rubble. Ground vehicles have rebuilt track
assemblies, layered turrets, armoured transport bodies and launcher racks, with
matte camouflage. Infantry walking uses planted steps timed to travel distance.
Collapse fragments fall under gravity; textured dust, wind-driven mushroom smoke
and animated wreck flames reuse the supplied VFX sequences.

Editable review libraries and previews of the exact runtime geometry are in
[assets/art_refresh](assets/art_refresh/README.md). The native geometry remains
parametric so damage, roof cutaways, animation joints and weapon sockets stay
integrated with gameplay.

The A-10 now flies cannon strafing sorties with a rapid 60-round burst, nose-muzzle
tracers, armor-contact flashes, ricochet sparks, dust and debris. Each pass has
one continuous 1.2-second burst and one rotary-cannon audio cue, then airfield rearming. Both aircraft
have smoother bodies and expanded mechanical detail. Direct attacks stay locked
to the ordered target, and ground dust uses a separate earth-coloured fade rather
than dark, rotated smoke.

A-10 flight now anticipates waypoint turns and limits turn acceleration, roll and
pitch changes. The final approach uses a continuous descent. Both maps are now
1024 x 1024 metres. The full base has a larger layout, resized facilities, service
roads and perimeter. Its 244-metre runway uses a compact forward-base layout,
with separate arrival/departure taxiways and landing priority. Older saved base
layouts and their parked aircraft are upgraded together.

The aircraft now use photographic references for their silhouettes: a smaller
A-10 canopy, hollow shaped engine intakes and thin tapered wings; an angular
Apache cockpit, slim tail boom and tapered rotor blades. Paint and glass use
subdued matte shading. References are recorded in
[the aircraft review kit](assets/art_refresh/REFERENCES.md).

The base compound is now 20% smaller in each horizontal dimension, with barracks,
hospital and supply grouped beside the vehicle depot. Service drives meet the
facility entrances. Blue unit symbols identify Field Command; red symbols
identify Iron Front, and markers only appear for units visible to the player.

The bottom command panel has permanent **Infantry / Barracks**, **Vehicles / Depot**
and **Aircraft / Airfield** tabs. Each selects an owned production building and
shows its compatible recruits. Click unit cards to queue up to five units per
building. Recruits train in order; separate buildings work independently. The
panel shows queue counts and progress, and **Cancel Last** refunds supply.
Aircraft queued for production reserve compatible stand capacity.

The A-10 cruises at 40 metres per second and strafes at 36. It fires one continuous
60-round burst in about 1.2 seconds from farther away, then immediately pitches
up and banks onto an escape leg. It breaks off before crossing the target even
if it cannot obtain a firing alignment. This also applies on the training range.


The Warthog accepts a replacement target during taxi, climb and approach, then
locks the active target for the final five seconds before cannon fire. Queued
orders remain queued for a later sortie. Its dive and pull-up follow continuous
curved flight paths; aircraft pitch follows the actual climb or descent.
Helicopters spool their rotors on the pad before lifting. Existing dust effects
blow outward during startup, low hovering, takeoff and landing, fading with height.

## Infantry classes and aircraft countermeasures

Barracks recruit four infantry classes: **Assault**, a standard rifleman;
**Sniper**, a scoped rifle specialist with 110m weapon range and 4.5s reload;
**Medic**, the existing unarmed healer and casualty rescuer; and **Support**,
a rifleman carrying a shoulder-fired anti-air launcher. Support also retains
field construction, repairs and emplacement tools. Commanders remain starting
leadership units, and gun emplacements are built equipment.

Support locks a visible airborne enemy within 160m for 1.25 seconds before
launching. It cannot launch from inside a building, in transport, while pinned,
or under hold-fire/move/retreat orders. An explicit attack order takes priority.
Missiles track their designated aircraft; they do not explode on ground units.
The launcher reloads for 12 seconds while the rifle remains available against
ground targets. Snipers and ordinary rifles cannot shoot airborne aircraft.

Each missile rolls once on launch for a **60% hit chance**, reduced to
**20% against active flares**. Aircraft therefore evade 40% of ordinary missiles
and 80% with flares. Every confirmed hit destroys the aircraft regardless of
remaining health; misses visibly pass clear of the airframe and airburst. The A-10 and helicopter each carry **three flare
salvos**, automatically deployed when a missile launches. Protection lasts 3s;
the dispenser has a 12s cooldown. Servicing at the airfield restores all salvos.
Flare counts and cooldowns appear in aircraft details; support shows launcher
readiness. Both factions and the training range use these rules. Rolls, locks,
missiles, flare stocks and timers persist in saves and use the deterministic
simulation, so reloading a save does not reroll an interception.

Infantry use the existing skinned Blender rig with class equipment attached to
its bones: a scoped, extended sniper rifle and camouflage shoulder drape; medical
crosses and unarmed medic work; an olive support launch tube that moves from the
pack to the shoulder, with matching muzzle and arm poses. The 7,000-triangle
limit includes the equipment. Legacy rifle/recon/engineer save IDs are retained
as assault/sniper/support, preserving orders and squads.

Menus and command buttons use embedded, nine-sliced enamel-and-metal raster art
with worn brass edges, native text and hover/pressed states. The original files
and generation prompts are in `native/assets/ui/`.

Unit symbols are shared between overhead markers, recruitment cards, roster and selection: assault uses three cartridges, medic a plus, support an open wrench head, sniper a scope reticle. Vehicles use distinct tracked silhouettes, with separate A-10 and helicopter silhouettes. Blue and red preserve faction identification without revealing hidden enemies.


Both teams now receive a fixed **1 supply every 5 simulated seconds**, without
mine or settlement income stacking. Pausing freezes this clock. Nearby infantry
coordinate covering fire, bounds and flanks using visible enemies and local force
strength. Heavy suppression, injury, a lost covering partner or being badly
outnumbered interrupts an advance. Explicit move, retreat and hold-fire commands
remain authoritative; selected-unit details show the active tactical role.

The A-10 uses a longer approach and straight lineup before its single cannon
burst, including targets behind the takeoff direction or close to the airfield.
It then pulls away and returns for servicing.


Troop carriers now take six passengers through walking boarding orders. Select
infantry and right-click a friendly carrier, or select a carrier and use **Load
Troops [L]**; **U** unloads at clear positions around it. Passenger counts appear
in selected-unit details. Helicopters use machine-gun bursts against infantry and
rocket salvos against vehicles, choose visible threats, break away between attacks
and evade missiles. Infantry seeks shelter from helicopters while available
support soldiers establish anti-air defence. Rocket exhaust trails are broader,
paler and longer-lived. New battles use one connected 52-house town with three
capture squares; existing saves and training practice houses retain their layouts.


Infantry attack orders stop at firing range once there is a clear shot; blocked
shots seek a lateral firing position. This also applies to tactical advances and
queued attacks. Grass, earth and gravel blend across the battlefield and training
ground, with softer field edges and varied, clustered woodland. Vehicles already
overlapping a runway building can drive clear while retaining their orders.
