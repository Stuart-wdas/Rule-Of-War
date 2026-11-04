# A-10 Blender source

The latest livery is packed into `a10.blend`: matte service grey, panel seams,
restrained wear, anti-glare nose paint, low-visibility wing markings and FT 082
tail codes. Its atlas is also saved as `materials/a10_livery.png`. Geometry and
the reference fit are preserved; the exporter carries the authored corner UVs
through the native renderer. Mechanical details retain the shared aircraft paint.
`previews/a10_livery.png` and `a10_livery_side.png` show the new finish. The
pre-paint Blender file is retained as `a10_pre_livery.blend`.

`tools/paint_livery.py` records the initial UV/paint authoring operation. Regular
manual paint edits only require saving the atlas and running `export_a10.py`;
rerunning the authoring script replaces its generated paint and UV layout.

`a10.blend` contains the editable fuselage, canopy, wing, horizontal tail,
shared twin-fin mesh and shared engine nacelle mesh. These surfaces are authored
in Blender and exported to `a10_geometry.json`, which the native game embeds at
compile time. The mechanical detail collection shows the game's landing gear,
fans, hardpoints and cannon, with their rigid animation pivots and muzzle marker.
Those details remain authored in `native/src/aircraft_models.rs`.
The complete assembled aircraft, including mechanical detail, is 6,852 triangles.

The original user-supplied three-view image is packed into the file and retained
unchanged in `references/user_three_view.png`, including its attribution strip.
Three image empties align its top, side and front views independently. They are
modeling references only and are excluded from the game export. The drawing is
small and its views differ slightly in scale; the side view is calibrated to
the cannon outlet and main-tire baseline. The follow-up side screenshot is also
packed and retained in `references/user_side_feedback.png`; the model follows its silhouettes
at the established aircraft scale. Manufacturer and photographic
references are recorded in `assets/art_refresh/REFERENCES.md`.

Open the **SURFACES** collection to edit geometry. Fin and engine counterparts
share mesh data. The **REFERENCE_TOP**, **REFERENCE_SIDE**, and
**REFERENCE_FRONT** cameras provide orthographic inspection views. The initial
viewport opens in side view with the image behind the model. The **REFERENCES**
collection can be hidden to inspect the model alone. Packed text blocks describe
the workflow and include the exporter.

After editing and saving `a10.blend`, export and rebuild from the repo root:

```powershell
& 'C:/Program Files/Blender Foundation/Blender 5.2/blender.exe' --background assets/aircraft/a10/a10.blend --python assets/aircraft/a10/tools/export_a10.py
cargo build --locked
```

`Engine.R` and `Fin.R` locations are exported as attachment coordinates. Their
left counterparts share the same mesh and mirrored attachment. Canopy rail
stations are stored on the scene. Mechanical detail is a separate native review;
editing its imported meshes does not alter those native parts.
Apply object rotation and scale to fin and engine meshes before exporting;
their object translations provide the attachment coordinates.

To refresh the mechanical context and beauty previews after rebuilding:

```powershell
& 'native/target/debug/rule-of-war.exe' --export-art assets/art_refresh
& 'C:/Program Files/Blender Foundation/Blender 5.2/blender.exe' --background --python assets/aircraft/a10/tools/build_reference_a10.py -- --sync-details
& 'C:/Program Files/Blender Foundation/Blender 5.2/blender.exe' --background --python assets/aircraft/a10/tools/reference_overlays.py
```

`previews/` contains Blender beauty views and wireframe comparisons with the
reference. The overlay tool uses the packed original image inside Blender; it
does not modify it. The reference cards and wireframe duplicates are temporary
and are not saved into the source file.

`build_reference_a10.py` without `--sync-details` is the initial authoring
bootstrap. It replaces the `.blend`; do not rerun it after making manual edits.

The side-profile refinement reshapes the nose, moves the canopy forward, shortens the
rear fuselage and lowers the horizontal stabilizer. Native landing-gear and
cannon placement follow the same profile. `refine_side_profile.py` records that
first authoring operation; `fit_side_silhouette.py` records the final profile
fit. These are authoring scripts rather than the regular export command.
