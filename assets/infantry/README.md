# Woodland rifleman

An editable, skinned low-poly infantry character built in Blender 5.2.2 for
Rule of War, using the supplied soldier turnaround, M4 reference and camouflage
swatch. The equipped character has **10,834 triangles**, including the rifle,
helmet, plate carrier, backpack, boots and padded pouches. The quality target is
approximately 12,000 triangles, with a 14,000-triangle source ceiling.

## Files

- `infantry_rifleman.blend`: editable mesh, source materials, packed textures,
  humanoid skeleton, grip controls, demonstration actions and isolated studio.
- `infantry_rifleman.glb`: self-contained skinned export with one material,
  embedded base-colour/metallic-roughness textures and baked animation clips.
- `textures/`: 2K base-colour, roughness, metallic, tangent normal and short-range
  ambient-occlusion atlas source maps. The export packs AO/roughness/metallic.
- `previews/exported_rifleman.png`: rendered from the re-imported GLB.
- `previews/rifle_ready.png`, `front_bind.png`, `back_bind.png`, `walk_rifle.png`
  and `crouch_rifle.png`: authoring and deformation views.
- `validation.json`: source geometry, parts, skin weights and reference inventory.
- `export_validation.json`: independently inspected GLB buffers and clip checks.
- `roundtrip_validation.json`: sampled source/export pose comparison.

## Rig

The skeleton has 35 bones, with 28 marked for deformation. All mesh vertices
have normalized weights, with no more than two nonzero influences in the source.
The root is at ground level. Blender uses metres, +Z up and -Y forward; the GLB
uses +Y up and +Z forward. The equipped height is approximately 1.85 metres.

The hierarchy includes pelvis, spine, chest, neck, head, clavicles, upper arms,
forearms, hands, simplified fingers/index/thumb, thighs, shins, feet and toes.
`weapon` controls the rifle; `muzzle` identifies the actual barrel exit.
`magazine` is an attachment marker; magazine geometry currently follows the
weapon, so a separate magazine mesh is needed for an animated removal/reload.

The rifle poses use two-bone arm IK and hand rotation constraints. Move
`hand_ik.L` and `hand_ik.R` to adjust the grips; move `weapon` to reposition the
rifle and both grip controls together. The elbow pole markers are provided for
future animator setup; the current solve uses the bent rest skeleton without
pole constraints. The lower body is an FK rig.

The `A_Pose` action disables the hand constraints for a neutral reference.
`Idle_Rifle`, `Walk_Rifle`, `Fire_Rifle` and `Crouch_Rifle` demonstrate the skin
and grips. Locomotion is in place at 30 fps. These are authored demonstration
clips. The revised walk plants each foot through 62% of its cycle, lifts the
swing foot, bends the knees forward and keeps a narrow stance. The native
animation driver advances the walk by distance travelled, accounting for model
scale, rather than playing a fixed-speed loop. `gait_validation.json` records
the planted stride and sampled knee checks.
The GLB contains sampled joint transforms, so its playback needs no Blender IK.
The model refinement pass adds subtle garment folds and shaped equipment while
taking the equipped mesh to 10,834 triangles. Its baked atlas exports as UV0 to match generated
tangents; the source `.blend` retains both authoring and atlas UVs.

Actions are retained in muted NLA tracks for easy selection. The source opens
with `Idle_Rifle` active. Use the Action Editor to choose another action.
The `STUDIO | not exported` collection contains presentation objects only.

## Resources

The supplied turnaround photograph and M4 photograph were used as visual
references; they are not projected onto the mesh or included in the GLB.
Uniform material samples the fabric area of `uniform materials.webp`, excluding
its garment seam, and webbing reuses `materials/cloth.png`. Both images are
packed in the editable source. New geometry and demonstration motions were
authored specifically for this asset; supplied resources retain their provenance.

## Rebuild and validate

Run from the repository root in PowerShell:

```powershell
& 'C:\Program Files\Blender Foundation\Blender 5.2\blender.exe' --background --factory-startup --python 'assets\infantry\tools\build_infantry.py'
python 'assets\infantry\tools\validate_infantry.py'
& 'C:\Program Files\Blender Foundation\Blender 5.2\blender.exe' --background --factory-startup --python 'assets\infantry\tools\check_roundtrip.py'
```

The builder asserts the complete triangle budget and normalized weights before
exporting. `-- --no-render` skips previews. `-- --no-bake` reuses existing atlas
maps and is only appropriate when geometry, UVs and materials have not changed.
The independent GLB check verifies indices, skin weights, image embedding,
finite animation data, named sockets and zero degenerate triangles. The Blender
round-trip compares sampled joint positions and deformed mesh bounds and checks
the boot grounding in the standing, walking and crouching clips.

The native game uses this GLB through `native/src/infantry.rs`. It is embedded
in the executable, with shared mesh/texture assets and independent skeletons.
The renderer samples and blends the four rifle clips from authoritative actions,
then applies torso aim and derives the current muzzle from the bone hierarchy.
It creates an unarmed mesh variant for medics, working support and shoulder-launcher poses and adjusts
their arm poses. Faction patches, medical crosses and role badges are small
runtime additions. The runtime scales the source by 0.82 before applying the
catalogue's world scale, preserving established game proportions.


## Combat classes

The runtime equips this rig as assault, sniper, medic or support. Sniper optics,
barrel extension and camouflage shoulder cloth follow the weapon/chest bones.
Support carries an eight-sided low-poly launch tube, sight, grip and end caps;
it switches from the backpack to the shoulder while acquiring an aircraft.
The animation driver lowers the rifle and adjusts the arms for this pose.
Medical crosses and unarmed treatment remain attached to the medic rig.
`--infantry-models --offscreen --compact --smoke-test <absolute.png>` counts
all rendered descendant meshes and asserts that every equipped soldier remains
within the 15,000-triangle runtime ceiling including attachments. Add
`--infantry-launcher` to inspect the raised tube.
