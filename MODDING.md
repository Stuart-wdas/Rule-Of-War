# Modding and Architecture Guide

This project is being separated gradually so each step remains playable. The
current structure is intentionally simple JavaScript modules with no build
step.

## Where things live

| File | Responsibility |
| --- | --- |
| `game-rules.js` | Unit stats, building stats, defaults, and configuration limits. |
| `maps/map-catalog.js` | Map-selection metadata and optional polygon tile definitions. |
| `maps/runtime-map.js` | Runtime polygons, centroids, bounds, topology, and deployment ordering. |
| `controllers/ai-controller.js` | AI decisions: economy, movement, target selection, and recruitment weights. |
| `controllers/air-controller.js` | Bomber interception, payload, casualty, return, and landing resolution. |
| `controllers/movement-controller.js` | Movement collisions, animation, and position updates. |
| `controllers/combat-controller.js` | Attack snapshots, damage, animation timeouts, and casualty removal. |
| `controllers/production-controller.js` | Completing construction and spawning recruits. |
| `controllers/phase-controller.js` | Movement/combat phase orchestration and round cleanup. |
| `state/game-state.js` | Serializable piece factories and fresh campaign creation. |
| `core/board-utils.js` | Circular distance, coordinates, and deployment ordering. |
| `core/battlefield-view-index.js` | Per-render occupancy, visibility, order, and highlight indexes. |
| `core/navigation-graph.js` | Shared-edge tile adjacency and movement reachability. |
| `config/developer-config.js` | Developer-setting validation and persistence. |
| `network/snapshot-utils.js` | Canonical snapshots, hashes, cloning, and seeded randomness. |
| `network/multiplayer-state.js` | Empty connection state and WebSocket address creation. |
| `storage/saved-match-store.js` | Local saved-human-match persistence. |
| `ui/dom-elements.js` | Central registry of every DOM selector used by the game. |
| `game.js` | Application coordinator: DOM UI, player input, order validation, and multiplayer events. |
| `battlefield3d.js` | Three.js scene, piece models, camera, and animations. |
| `server.cjs` | Static files, WebSockets, multiplayer sessions, and checkpoints. |
| `styles.css` / `index.html` | Interface appearance and markup. |

The important boundary is: **controllers choose actions; `game.js` validates
and resolves them**. For example, the AI calls `queueOrder()` instead of moving
a unit directly. This keeps AI and human players on the same rules.

## Add a selectable map

Add an entry to `MAP_CATALOG` in `maps/map-catalog.js`. The `id` must be
unique, `cols` and `rows` must stay within `CONFIG_LIMITS`, and `preview`
selects a CSS preview treatment in `styles.css`. To create a mixed map, add a
`createTiles()` factory whose entries have a unique logical `x`/`y`, an `id`,
and three or more world-space `vertices`. Include `tileCount` so the selection
screen can describe the map without generating it. Polygon maps still need
valid `cols` and `rows`
for saved-game configuration validation. Map selection only overrides
dimensions; developer-configured forces, economy, ranges, and fog remain in
effect. Human hosts send those dimensions through the existing campaign
snapshot, so joining players do not select a separate map.

## Change unit balance

Open `game-rules.js` and edit an entry in `UNIT_TYPES`:

```js
scout: {
  label: "SCOUT",
  price: 6,
  actionCost: 1,
  range: 4,
  hitChance: 4 / 6,
  lives: 2,
  damage: 1,
  mark: "SC",
  movementRange: 7,
},
```

Common fields:

- `price`: recruitment cost.
- `actionCost`: supply spent on each move or attack.
- `range`: circular attack and fog-reveal radius.
- `movementRange`: circular world-space radius measured between tile centres.
- `hitChance`: number from `0` to `1`.
- `lives`: maximum health.
- `damage`: damage from a successful hit.
- `totalAttackActions`: optional lifetime ammunition limit. All remaining shots may be queued in one attack phase; defaults to the normal one attack per round when omitted.
- `recruitable: false`: hides a special unit from barracks.
- `productionBuilding`: changes the required producer from the default `barracks`.
- `airUnit: true`: permits occupied destinations and enables the airborne render layer.
- `automaticAttack: true`: removes ordinary attack orders; the air controller resolves the payload.

For a completely new unit, also add its starting-count configuration fields to
`DEFAULT_GAME_CONFIG` and `CONFIG_LIMITS`, add it in `createStartingForces()` in
`game.js`, and create a visual branch in `_buildUnit()` in `battlefield3d.js`.
If it needs a unique projectile, extend `_animateSingleAttack()` there as well.

## Add a building

Add its stat entry to `BUILDINGS` in `game-rules.js`, then:

1. Add a construction button with `data-build="yourType"` in `index.html`.
2. Add its model branch in `_buildStructure()` in `battlefield3d.js`.
3. Add any special economy or gameplay effect near helpers such as
   `incomeFor()` in `game.js`.
4. Add starting-count configuration only if it should spawn initially.

Placement legality is centralized in `isValidBuildTile()` in `game.js`.
Every current building uses `footprintSize: 3` and stores its nine occupied
tile coordinates in serializable `footprint` state. Collision, targeting,
construction, fog visibility, and spawning all consume that footprint. Regular
square and hex maps use a logical 3 × 3 block; mixed-topology maps claim the
centre plus the eight nearest edge-connected tiles.

## Bomber and Airfield flow

The `plane` catalog entry is presented as **BOMBER** and declares
`productionBuilding: "airfield"`. Each Airfield supports one living or queued
bomber. Production places it on the Airfield with `homeAirfieldId`, and that
intentional overlap is legal because air units use a separate occupancy layer.

During Orders, a bomber move is a complete sortie. `MovementController` flies
it to the selected square even when a ground unit or building occupies that
square and stores its outbound `sortie`. At Resolve, `AirController`:

1. Finds every hostile ground unit whose circular firing range touches the
   outbound line segment and gives it one automatic interception shot.
2. Cancels the payload if interception destroys the bomber.
3. Rolls the bomber payload against the hostile ground piece under it.
4. Returns and lands every survivor at its original Airfield.

Normal units still cannot move and attack in one round. The bomber is the
explicit exception because its attack belongs to the sortie rather than the
ordinary attack-order queue.

## Create square, triangle, hexagon, or mixed boards

Movement is topology-based. Each logical tile has a unique `id`, a unique
piece position (`x`, `y`), and polygon `vertices` in clockwise or
counter-clockwise order:

```js
import { createNavigationGraph } from "./core/navigation-graph.js";

const navigation = createNavigationGraph([
  {
    id: "square-a",
    x: 0,
    y: 0,
    vertices: [
      { x: 0, y: 0 }, { x: 1, y: 0 },
      { x: 1, y: 1 }, { x: 0, y: 1 },
    ],
  },
  {
    id: "triangle-b",
    x: 1,
    y: 0,
    vertices: [
      { x: 1, y: 0 }, { x: 2, y: 0.5 }, { x: 1, y: 1 },
    ],
  },
]);
```

Those example tiles are neighbours because they share the complete edge from
`(1, 0)` to `(1, 1)`. Partial collinear edge overlaps also connect, which lets
one large edge touch multiple smaller tiles. A single shared corner does not
connect tiles. `navigation.reachable(tile, steps)` enumerates destinations and
`navigation.isConnected(from, to)` verifies that a shared-edge route exists.
The game then uses circular centroid distance to validate movement range.

`createRuntimeMap()` installs either the selected polygon list or a generated
square grid. `Battlefield3D` builds and ray-casts the polygon meshes directly.
Player movement, AI movement, commander construction reach, and recruit
spawning consume the shared-edge graph. Attack and fog ranges use polygon
centroids so they remain circular world-distance rules.

Honeycomb Front is created with `createHexMapPreset()` in
`maps/map-catalog.js`. Edit its `cols`, `rows`, and `tileRadius` values in the
catalog entry; the generator, displayed tile count, configuration dimensions,
saves, and multiplayer identity update together. Its centres use a
`1.5 × radius` column step, a `sqrt(3) × radius` row step, and a half-row offset
on alternating columns. Interior hexes therefore share six complete edges
without manually declaring their neighbours.

## Large-map performance

Regular square battlefields support dimensions up to 1000 × 1000 without
materializing one million gameplay or render objects:

- `VirtualSquareNavigationGraph` resolves tiles, neighbours, connectivity, and
  Manhattan steps mathematically. `runtimeMap.tiles` intentionally remains
  empty for these boards; use `getTile()`, `tilesWithinRadius()`,
  `tilesInBounds()`, or `sampleTiles()` instead.
- `runtimeMap.tilesWithinRadius()` scans only the clipped circular query area,
  so movement, attack, fog, spawning, and AI work are based on rule range, not
  total map area.
- Detailed rendering is capped by `MAX_DETAILED_TILES` in `game.js` (currently
  30,000). `Battlefield3D.getRenderBounds()` covers the camera's visible ground
  footprint with whole render chunks, and a reusable `InstancedMesh` receives
  only that nearby window.
- Virtual square maps publish `renderChunks`, `chunksInBounds()`, and
  `renderChunkSize` from `maps/runtime-map.js`. The default chunk is 64 x 64;
  add `renderChunkSize` to an individual map preset to override it.
- With fog enabled, the window is filtered again so only revealed, highlighted,
  selected, or targeted tiles receive detailed geometry. Fogged tiles do not
  create grid instances.
- The rest of the theater is one opaque strategic surface. A single shader
  composites terrain tone, visibility fog, adaptive grid lines, and analytic
  range circles. These effects never occupy competing coplanar transparent
  meshes, avoiding depth flicker and black bands at shallow camera angles.
- Detailed tile LOD uses projected pixels per tile with separate enter/exit
  thresholds. Orbiting at the same zoom therefore cannot demote the map, and
  the threshold gap prevents rapid LOD toggling. Detail is also withheld when
  the complete camera footprint exceeds the tile budget, preventing a chunk
  boundary from cutting across the visible map.
- Maps above `VIRTUAL_RENDER_THRESHOLD` disable whole-board shadows, cap device
  pixel ratio, and reuse one detailed tile material. Pieces and combat effects
  remain normal because their counts are independent of map dimensions.
- `createBattlefieldViewIndex()` converts serializable state arrays into maps
  and sets once per render. Never add a full-map `find()` or `filter()` loop to
  a phase or camera callback.
- The WebGL fallback is capped separately by `MAX_FALLBACK_TILES`; ordinary
  maps still receive their complete semantic HTML grid.
- Map selection opens a staged loading view and yields between runtime-map,
  force, UI, fog, and Three.js setup so the interface remains responsive.

Authored mixed-polygon maps still use their explicit tile array and shared-edge
graph because arbitrary topology cannot be inferred mathematically. Keep those
maps below roughly 20,000 polygons. To scale a regular hex/triangle topology to
hundreds of thousands of tiles, follow the square-map pattern: add a virtual
navigation class and bounded descriptor provider instead of returning every
polygon from `createTiles()`.

### Procedural terrain controls

Terrain is split into small modules under `terrain/`. Tune
`DEFAULT_TERRAIN_GENERATION_CONFIG` in `terrain/terrain-generator.js` to change
the physical elevation scale, terrain thresholds, elevation/moisture noise,
river count and source rules, depression breaching, or gameplay costs. Values
are expressed in tile/world units, so the same seed and configuration produce
the same terrain regardless of camera or client.

For the elevation and moisture noise sections:

- `broadWavelength` controls the scale of major land masses. Larger values
  make broader regions.
- `detailWavelength` controls local variation. Smaller values add tighter
  detail.
- `warpWavelength` and `warpStrength` bend otherwise regular noise contours.
- `broadWeight` and `detailWeight` choose the balance between the two scales.
- `octaves`, `persistence`, and `lacunarity` under `broadNoise` and
  `detailNoise` control layer count, retained amplitude, and frequency growth.

`waterBodies.minimumFlatCells` decides how large a connected lake or sea must
be before it is leveled. `shorelineOffset`, `maximumSurfaceLift`, and optional
`fixedSurfaceElevation` control its physical water level. The reusable noise
math itself is in `terrain/noise.js`; most map tuning should only require the
generator configuration.

The generator stores large fields in typed arrays and exposes complete cell
records lazily through `cellAtGrid()`, `cellAtMapPoint()`, and `sample()`. Do not
copy these fields into every virtual runtime tile. Saved and multiplayer games
store the terrain seed and regenerate the arrays deterministically.

Open the developer panel with F12 to enable the seven-colour terrain debug
view and optional normalized elevation labels.

### Large-map rendering controls

The main controls are intentionally grouped near the top of
`battlefield3d.js`:

- `VIRTUAL_TILE_LOD.detailExitPixelsPerTile`: lower it to keep detailed tile
  textures visible farther out.
- `VIRTUAL_TILE_LOD.detailEnterPixelsPerTile`: zoom-in threshold for restoring
  detail. Keep it higher than the exit value to retain hysteresis.
- `VIRTUAL_FOG_QUALITY.pixelsPerTile` and `maximumTextureSize`: raise them for
  a smoother fog boundary; texture memory and fog-mask upload cost also rise.
- `VIRTUAL_GRID_QUALITY.targetCellPixels`: raise it to switch to a coarser
  strategic grid sooner. `lineHalfWidthPixels` changes its screen-space width.
- `VIRTUAL_SURFACE_DETAIL.fadeEndDerivative`: raise it to retain the proxy's
  repeating terrain texture farther into a zoom-out. `strength` controls how
  strongly that texture appears at close range.
- `RANGE_OVERLAY_QUALITY`: controls the fill, boundary opacity, and world-space
  width of construction, movement, and attack circles.
- `DEFAULT_RENDER_CHUNK_SIZE` in `maps/runtime-map.js`: changes the project
  default. Prefer a per-preset `renderChunkSize` when tuning only one map.

`MAX_DETAILED_TILES` in `game.js` remains the detailed-instance budget. Keep it
at least as large as one complete render chunk squared.

## Modify the AI

Open `controllers/ai-controller.js`:

- `planDeploymentAndMoves()` controls economy and movement priorities.
- `chooseRecruitType()` controls recruitment weighting.
- `planAttacks()` controls target selection.
- `bestStepToward()` is the current greedy movement strategy.

The constructor receives engine functions instead of importing game state.
This dependency-injection pattern makes the controller easier to test and keeps
it from bypassing rules. A future defensive AI could be another controller
class with the same two public methods:

```js
controller.planDeploymentAndMoves();
controller.planAttacks();
```

## Game phase flow

```text
DEPLOYMENT -> ORDERS -> movement resolution -> ATTACK -> combat resolution
     ^                                                       |
     +----------------------- next round --------------------+
```

`advancePhaseLocally()` delegates transitions to `PhaseController`.
`beginAttackPhase()` resolves all movement before attacks are accepted.
`resolveRound()` plans AI fire, asks `AirController` to resolve sorties before
normal fire, asks `CombatController` to resolve combat, asks
`ProductionController` to complete jobs, awards income, and opens the next
round.

## Safe extension rules

- Queue actions through `queueBuilding()`, `queueRecruit()`, or `queueOrder()`.
- Do not remove destroyed units until all queued attacks have animated; combat
  deliberately snapshots attacks to preserve simultaneous resolution.
- Use the shared `distance()` helper for circular ranges.
- Use the navigation graph for movement or other shared-edge rules.
- Store serializable state only: multiplayer snapshots use JSON cloning.
- Add new configuration fields to both defaults and limits so validation and
  old saved settings receive sensible values.
- Verify rendering changes in the target map and camera angles before sharing
  a mod, because GPU limits and display resolution affect the chosen LOD.

## Remaining application-layer separations

The gameplay core is modular. The remaining large application-layer areas are:

1. `ui/game-ui.js` for DOM rendering and event binding.
2. `network/multiplayer-client.js` for WebSocket lifecycle and reconnection.

Those two areas deliberately remain coordinated by `game.js` because their
callbacks share live browser state. If extracted later, preserve `game.js` as
the composition root that creates controllers and connects their callbacks.

Move one subsystem at a time and verify AI and human multiplayer after each
step. Avoid a single large rewrite because multiplayer resolution depends on
both clients producing identical snapshots.
