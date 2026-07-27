# Rule of War

A browser-based 3D turn strategy game built with Three.js. Command against the Iron Front AI or play a synchronized two-player campaign, invest in supply mines, recruit specialized units, and destroy every enemy troop and barracks.

For file responsibilities, extension points, and recipes for adding units,
buildings, or AI behavior, see [MODDING.md](MODDING.md).

![Rule of War command view](game-home.png)

## Run locally

```bash
npm install
npm start
```

Open `http://127.0.0.1:4173`.

Quality checks:

```bash
npm test
npm run check
npm run benchmark:scale
```

## World campaign

Choose **World Campaign** from the main menu to open the persistent strategic
map. Each sector has its own deterministic terrain seed, objective, owner,
supply state, and sparse terrain-modification layer. Entering a sector launches
the familiar tactical battle; winning updates territorial control without
adding manual inventory or convoy management.

Supply is intentionally automatic. A sector is supplied when it has a
contiguous chain of friendly sectors back to its faction edge.

## Human multiplayer

Choose **VS Human**, create a campaign, and share the six-character match code. The second player opens the same server address and joins with that code. For another device on your local network, use the host computer's LAN address, for example `http://192.168.1.20:4173`.

After selecting AI or human play, choose a battlefield. In human
play, the host's map is included in the campaign checkpoint; joining and
resuming players receive it automatically. A staged loading screen yields
between navigation, force deployment, UI, fog, and renderer setup.

The Node server validates ownership, phases, command counts, targets, prices,
and resources before relaying compact plan deltas. Each phase advances only
after both players report ready. Divergent phase results are rejected and
restored to the server-held pre-resolution checkpoint. Match state is saved in
both browsers and persisted by SQLite in `.data/rule-of-war.sqlite`; **Save &
Leave** keeps the resume token in that browser.

If the server or a player disconnects, the campaign pauses. Restart the server with `npm start` and use **Resume Campaign** to reconnect to the saved state.

## Developer tools

Open the in-game campaign configurator with `F12` (often `Fn` + `F12` on laptops), `Ctrl` + `Shift` + `D`, or the **DEV** button in the header.

The configurator can change:

- Map columns and rows.
- Starting supply, mines, barracks, and every unit count independently for both factions.
- Mine income, fog of war, and each unit type's firing range.

Applying settings begins a new campaign so existing pieces and orders cannot conflict with the rebuilt map. Valid settings persist in the current browser. **Restore Defaults** only stages the original values until they are applied.

## Rules

- Each round progresses through Deploy, Orders, Attack, and Resolve.
- Strategic world sectors launch deterministic tactical battlefields, while
  standalone presets include 128², 512², and 1024² fronts.
- Broken Front demonstrates a playable mixed topology of square and triangular tiles.
- Honeycomb Front is a complete regular-hex map with six connected movement directions per interior tile.
- Both forces begin with one commander at the outer map edge; starting counts for all four recruitable unit types are configurable.
- Select units and destination tiles directly on the ray-cast 3D battlefield; the semantic grid is retained as the WebGL fallback.
- Large regular-square maps use mathematical navigation and a moving detailed tile window instead of allocating or drawing the full board. Developer map dimensions may be configured up to 1000 × 1000.
- Fogged tiles outside the current tactical window are represented by the board base, while revealed and interactive tiles are streamed into a capped instanced batch.
- Left-drag rotates the camera, the mouse wheel zooms, and right-drag pans within constrained battlefield bounds.
- Large maps automatically thin the edge labels and expand the viewport; use **Fit Map** to restore the full-board camera view.
- Terrain uses separate deterministic elevation and moisture fields, configurable elevation bands, flat connected lakes/seas, and eight-neighbour downhill rivers with merging flow. Every terrain cell carries movement cost and passability, while units, buildings, route markers, and projectile targets follow the displaced surface.
- Buildings occupy nine tiles in a 3 × 3 footprint. Bridges use a linear start-to-end footprint, can only be commissioned by a surviving commander, and cost three supply per metre of straight-line displacement rounded to the nearest whole supply. Construction and recruits become available after the round resolves.
- Ground movement follows the lowest-cost passable land route and navigates around deep water, shallow water, and rivers unless an operational bridge spans the water cell. A thin full path and small flags split the journey into one movement-budget segment per round. Each successful segment costs one movement action that turn; later turns are never charged in advance. Commanders move up to eight movement points per round and carry three shots for the entire campaign, all of which may be fired in one turn.
- A building centre may be placed within two tiles of an existing building footprint, while a surviving commander also permits construction anywhere within its movement radius.
- Movement animates and resolves before the Attack phase. An unfinished ground route remains queued for later rounds and advances only to its next flag, so every shot still uses the unit's current position.
- Units that successfully move are committed for the round and cannot also fire. Bombers are the exception: an Airfield launches one bomber to an occupied or empty target square, every hostile unit whose range touches its route attempts interception, and a survivor drops its payload before returning to the Airfield.
- Short and medium units fire visible direct projectiles; long and extra-long artillery launch arcing shells with explosive impacts.
- Fog of war reveals squares only within the combined firing ranges of surviving friendly units.
- Multiple units can fire on the same square; each shot rolls independently and adds its own damage.
- By default, short, medium, long, and extra-long units fire up to 5, 8, 14, and 28 blocks respectively.
- Mines cost 25 supply and produce 5 supply per round by default.
- Win by destroying the opposing barracks and all opposing units.
- Alternatively, capture every command post and hold them for two consecutive
  resolutions.
- Medics and Field Hospitals heal automatically. Scouts extend vision,
  Engineers share field-construction controls, and Anti-Air formations
  automatically intercept bombers.
