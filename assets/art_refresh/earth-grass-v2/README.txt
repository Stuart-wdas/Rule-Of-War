EARTHY GRASS VARIATIONS — APPLIED TO RULE OF WAR

The two new generated ground textures are active in ordinary matches, training
and the terrain demonstration. Restart the game to load the new assets.

Runtime material sets:
  materials/pbr/earth_grass_olive_v2_{albedo,normal,orm}.png
  materials/pbr/earth_grass_stony_v2_{albedo,normal,orm}.png
Original generated colour/height images:
  materials/pbr/earth_grass_v2/
Exact built-in image generation prompts and provenance: generation.json.
Final albedos were calibrated with built-in image generation against the existing
game sunlight. That edit preserved the material layout and shade orientation;
its exact prompts are also in colour-calibration.json. The earlier brighter
sources remain as olive-initial.png / stony-initial.png alongside the final maps.
Material repeats, hashes and technical settings: materials.json.

Olive grass repeats at 8m; the stonier scrub repeats at 6m. Broader irregular
world-anchored patches decide where each appears. Translation-only stochastic
sampling breaks repetition, with sharper blend weights to retain grass/stone
detail instead of averaging it into a haze. All colour/normal/ORM samples share
the same offsets. No rotation or mirroring changes the baked shade direction.
Linear colour means for contrast preservation are read from the asset catalog.

Lighting: short contact shadows were requested with sun from image bottom-left
and shadows towards image upper-right. This corresponds to the existing Bevy
sun ray (+X, -Y, -Z), approximately 51 degrees above the horizon. Large rocks,
shrubs, trees and vehicles retain dynamic geometric shadows. Creeping plants
in these textures are only shallow ground detail, not collision/cover objects.
The height maps are AI approximations, not measured scans; they generate shallow
normal detail. The existing 90m meadow macro normal/height remains in register.

Asphalt/concrete keep separate materials. Terrain colour stays nonmetallic and
matte. Grass leaves are toned down to muted olives to connect the 3D vegetation
to the ground. Navigation, weapon rules, camera lens and explosion bakes remain
unchanged. Existing tracks still flatten/darken natural ground.

View Photoreal Ground Demo.cmd starts the latest close comparison scene.
View Current Ground Demo.cmd shows the legacy palette in the same scene.
Use --legacy-ground-materials on a normal launch for the previous ground look.
Graphics quality controls still select 512/1024/2048 runtime terrain arrays.

Build technical maps: tools/build_earth_grass_maps.py
Fast shader helper validation: tools/check_ground_shader_helpers.py
Native rendering/variation/quality checks: tools/check_earth_grass.py
