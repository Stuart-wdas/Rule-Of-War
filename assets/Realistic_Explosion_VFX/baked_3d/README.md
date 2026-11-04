# Baked 3D explosions

Gameplay randomly chooses among all four preview effects: the mushroom cloud, the **combined fireball + smoke-offshoot blast**, the original debris fireburst and the original spiky blast. The combined blast uses the original debris fireburst from the left of the reference screenshot and all eleven curved smoke trajectories from the right, playing together at one position, scale, rotation and time. Its smoke roots are softened so the central fireburst stays visible.

Run `View 3D Explosions.cmd` at the repository root. It opens the combined effect by default and repeats it every eight seconds.

- **1:** mushroom; **2:** combined effect.
- **3:** original fireburst reference; **4:** original spiky reference.
- **5:** compare the mushroom and combined gameplay effects.
- **Enter:** replay; **F6:** Low / Balanced / High; **Space:** pause.
- **Alt + middle drag:** orbit; **wheel:** zoom.

`combined.blend` is the editable Blender scene for the merged effect. Space plays its cached particle motion; rendered shading or F12 shows the volume materials. `mushroom.blend`, `debris.blend` and `../3d/spiky_explosion/spiky_explosion.blend` retain the source studies. These are animated 3D volume particles and ballistic meshes, not camera-facing explosion sprite sheets. Original force-integrated trajectories are reused; no runtime fluid solver is needed.

`combined.json` has 239 tracks: 72 original fire/dust lobes, 24 ballistic fragments, 132 smoke samples distributed over eleven trajectories, and eleven brief burning tips following the original ballistic heads. Motion is sampled at 24 FPS. The hot core expands first, then the smoke arms spread outward and fade within about 2.7 seconds. Compact glowing tips burn out before the smoke clears. Solid debris retains its longer lifetime. All components share one native blast event and the same scene-depth clipping against buildings and terrain.

| Preset | Merged core lobes | Cached fragments | Smoke samples (all 11 offshoots) | Burning tips | Total tracks | Ray steps | Global particle cap |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Low | 20 | 8 | 132 | 11 | 171 | 8 | 208 |
| Balanced | 48 | 12 | 132 | 11 | 203 | 12 | 320 |
| High | 72 | 24 | 132 | 11 | 239 | 20 | 480 |

Every preset keeps all smoke samples so the offshoots stay connected. Lower presets reduce core detail and ray steps instead of leaving gaps along the trails.

Transparent volume proxies opt out of GPU instance batching so their back-to-front draw order is preserved. Opaque debris remains batched. The raymarch sampling phase is attached to the particle seed, keeping density detail stable as a plume moves across screen pixels.

The mushroom and merged effect fit together in the preview at every quality. Low's global cap is 208 to accommodate the eleven added heads alongside the mushroom; Balanced/High retain their previous caps. Whole-effect admission remains bounded. Every gameplay explosion makes one random choice among all four preview effects, each with a 25% chance, using a stable event seed so both teams and replay clients see the same selection.

Recreate the combined scene and cache from the repository root:

```powershell
& 'C:/Program Files/Blender Foundation/Blender 5.2/blender.exe' --background --python assets/Realistic_Explosion_VFX/tools/merge_3d_blasts.py
```

The merger reads the existing source caches without modifying them. To regenerate the original mushroom/debris physics, run `tools/bake_3d_particles.py` first, then run the merger. Rebuild the native game after rebaking because the caches are embedded; do not rewrite them during Cargo compilation.

For automated native previews use `--vfx-showcase --merged --vfx-age 0.9` (default merged), `--mushroom`, `--original-debris`, `--spiky-remake`, or `--compare-vfx`. Add `--vfx-quality low`, `balanced` or `high`; `--blast-side` changes the view angle. The existing offline gallery remains available through `View Explosion Preview.cmd`.

Render-regression controls require `--smoke-test`: `--vfx-frozen-age 1.0` holds the exact baked pose and `--vfx-view-yaw 0.3` fixes the camera angle. `--vfx-legacy-batching` and `--vfx-legacy-jitter` reproduce the previous renderer for comparisons. They are ignored by normal gameplay and the interactive viewer.
