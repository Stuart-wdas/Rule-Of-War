# Building and vehicle review kit

This kit contains editable Blender libraries exported from the actual native
game geometry. Runtime models are authored parametrically in
`native/src/building_models.rs`, `military_models.rs`, `vehicle_models.rs`,
`aircraft_models.rs` and `aircraft_geometry.rs`; changing a review `.blend` does
not change the game. The A-10's main surfaces now have a separate authoritative
Blender source at [`../aircraft/a10/a10.blend`](../aircraft/a10/a10.blend), with
an exporter feeding the native renderer. See its [workflow](../aircraft/a10/README.md).

- `buildings.blend`: four house plans, representative damage and ruin stages,
  barracks, depot, hospital, mine, watchtower and airfield.
- `vehicles.blend`: rebuilt tank, artillery, anti-air vehicle, armoured transport
  and rocket launcher, plus the upgraded A-10 and Apache-style helicopter.
- `runtime_kit.json`: canonical metre-scale mesh data, normals, UVs, material
  names, rigid animation pivots, muzzle markers and roof/wall cutaway tags.
- `armour_0.png`, `armour_1.png`: the game's olive and desert armour textures.
- `previews/`: rendered views of the exported models.

The files open with one model visible. Enable other asset collections in the
Outliner to inspect them. Materials and textures are packed. Rigid animation
parts have parent controls; muzzle markers identify the barrel exits. The studio
collection is for review only. Coordinates are converted from native Y-up,
Z-forward to Blender Z-up, negative-Y-forward without changing dimensions.

Buildings retain the game's doors, windows and floor plans. Roof components and
outward-facing wall tags support friendly garrison cutaways. Damaged buildings
use clustered holes and retained material colours, with exposed framing and
debris; collapse leaves a persistent ruin following the original footprint.

Ground vehicles use track shoes, road wheels, hubs, sloped armour, stowage and
role-specific weapons. Turret rotation, cannon recoil, wheel motion and muzzle
sockets remain connected to the existing game animation and projectile systems.
The aircraft are rebuilt around photographic references, documented in
[REFERENCES.md](REFERENCES.md). The A-10 has a smaller canopy, a shaped fuselage,
thin tapered airfoils with dihedral, curved inlet lips with recessed fans, shaped
twin fins and matte grey paint. Its cannon and empty hardpoints remain functional.
The Apache has flat window panes, painted cockpit roof sections, a tapered tail
boom, shaped engine fairings, slim swept rotor blades and a smaller radar dome.
Both use a separate subdued glass material. `aircraft_paint.png` is the neutral
wear texture exported from the same material builder used by the live renderer.

Rebuild from the repository root:

```powershell
cargo build --locked
& 'native/target/debug/rule-of-war.exe' --export-art assets/art_refresh
& 'C:/Program Files/Blender Foundation/Blender 5.2/blender.exe' --background --factory-startup --python assets/art_refresh/tools/review_in_blender.py
```

The native exporter writes the exact mesh assemblies, including representative
damage states, without opening a game window. The Blender script rebuilds both
libraries and renders previews. Review shading approximates the in-game palette;
the live game remains the reference for terrain lighting, faction colours and VFX.

For an aircraft-only preview update, add `-- --aircraft-review` to the Blender
command. This updates `vehicles.blend`, opens the A-10 by default, and renders
isometric and side views of both aircraft while retaining all vehicle collections.
