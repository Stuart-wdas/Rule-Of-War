# Hottot-les-Bagues — separate real-world map prototype

Launch **Play Real World Prototype.cmd** in the repository root (or **Play.cmd**
in this folder). It opens this map directly using its own packaged executable.
If that executable is absent it builds it first. Normal
**Play Native.cmd** still opens the ordinary game and Forward Base.

To refresh the prototype executable after editing source, run
`& ".\Play Real World Prototype.cmd" --rebuild` in PowerShell. The isolated executable uses the
native renderer without the optional DLSS feature. Existing materials and model
assets remain shared with the repository; keep this folder inside the project.

This is a playable, offline GIS prototype of present-day Hottot-les-Bagues,
Normandy, France. It is not a reconstruction of the village in 1944.

## Use

- **Space:** start the battle or pause it. The prototype starts paused for inspection.
- **WASD / drag / wheel:** normal pan and zoom. The opening view frames the village.
- Select friendly troops or vehicles and right-click to move/attack.
- Right-click a house with infantry to enter; **G** finds a nearby house; **U** leaves.
- **F2:** rebuild this prototype, including intact houses and starting forces.
- **F5 / F9:** save/load `native-gis-hottot.json`, separate from all other modes.
- **Escape:** resume, restart this prototype, continue its save, settings, or exit.
- Fog starts off so the geographic layout and both deployments are visible.

Both sides have infantry, recon, an engineer, a medic, a tank and a troop carrier.
Small recruitment facilities and three command posts are scenario additions.
The ordinary native movement, combat, garrison, damage and capture systems run
on the imported map. Combat and sight use the existing battlefield scale.

## Geographic content

The playable area is **2048 × 1024 metres**, centred at **49.1515° N,
0.6495° W**, north at negative Z, east at positive X. Horizontal and vertical
distances retain metre scale. A local WGS84 tangent approximation converts
latitude/longitude; it is appropriate to this small patch, not a global CRS.

- Real road alignments, mapped land-use polygons, ponds, woods and hedge lines
  come from the cached OpenStreetMap extract.
- 256 house sites retain source positions and approximate orientation. Existing
  one-storey RoW houses on 10 m plots stand in for the source buildings. Closely
  packed source buildings that would overlap those plots are omitted and listed
  in `map.json`. Roofs, facades, heights and interiors are not surveyed replicas.
- Actual elevation comes from Mapzen/AWS Terrain Tiles at zoom 12 (about 25 m
  tile pixel spacing here), resampled to an 8 m native source grid. Resampling
  adds no surveyed detail. The current source collar spans about 49–144 m above
  its source vertical datum. `map.json` records the offset used by the game.
- Real elevation extends **two kilometres beyond every playable edge**. Land-use
  and road features extend through a 300 m source collar; beyond it the scenery
  has neutral ground cover. Roads use continuous meshes and foliage exclusions.
- Only individual house foundations are graded. Deployment facilities and
  objectives are gameplay additions; no large artificial airbases are inserted.
- OSM polygon ways are supported in this first import; multipolygon relations
  and explicit road bridges/tunnels are not reconstructed. Unmapped vegetation
  and small surface details remain approximations.

`map-preview.png` is a north-up map of the imported features. Native render
captures are saved under `review/` when the prototype is verified.

## Source and build files

- `sources/osm.json`: original OSM response, including tags and geometries.
- `sources/osm-query.txt`: exact extraction query.
- `sources/terrarium-*.png`: untouched source elevation tiles.
- `map.json`: conversion metadata, source hashes, roads, footprints and exclusions.
- `hottot.terrain`: native `ROWTERR1` terrain, including the scenery collar.
- `cover.rgb`: 2 m RGB mask: land-use code, road exclusion, building exclusion.
- `../../tools/build_gis_prototype.py`: repeatable data export and map preview.
- `../../native/src/gis_prototype.rs`: isolated runtime loading and geographic rendering.
- `../../native/src/gis_prototype_sim.rs`: scenario and gameplay regression checks.

The pipeline implements the elevation + OSM workflow discussed for BlenderGIS
directly, without requiring Blender, an API key, or a live network connection to
play. Source files can also be used in a GIS/Blender authoring workflow later.

Rebuild offline with Python 3, numpy and Pillow:

```powershell
python tools/build_gis_prototype.py
```

Add `--fetch` to download missing cached sources. Existing cached inputs are
preserved; removing a cache intentionally is required to change the snapshot.

Native verification / captures from the repository root:

```powershell
cargo test --locked --no-default-features gis_ -- --nocapture
cargo run --locked --no-default-features -- --gis-prototype
cargo run --locked --no-default-features -- --gis-prototype --gis-overview --offscreen --compact --smoke-test "C:\Code\Rule Of War\prototypes\real-world-hottot\review\overview.png"
cargo run --locked --no-default-features -- --gis-prototype --gis-close --offscreen --compact --smoke-test "C:\Code\Rule Of War\prototypes\real-world-hottot\review\village.png"
```

## Attribution and data licences

**Map data © OpenStreetMap contributors**, available under the
[Open Database License (ODbL) 1.0](https://opendatacommons.org/licenses/odbl/1-0/).
See [OpenStreetMap copyright and attribution](https://www.openstreetmap.org/copyright).
The OSM source extract and OSM-derived geographic database in this folder are
provided under ODbL 1.0. This attribution does not change the licence of RoW's
game code or independently authored art assets.

**Elevation: Mapzen Terrain Tiles, hosted on AWS Open Data.** Europe terrain
produced using Copernicus data and information funded by the European Union —
EU-DEM layers; global SRTM/GMTED terrain data courtesy of the U.S. Geological
Survey. Dataset and provider details:

- https://registry.opendata.aws/terrain-tiles/
- https://github.com/tilezen/joerd/blob/master/docs/attribution.md
- https://github.com/tilezen/joerd/blob/master/docs/formats.md

This map uses no commercial satellite imagery or proprietary map tiles.
