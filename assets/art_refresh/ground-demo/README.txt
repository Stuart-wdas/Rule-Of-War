LATEST APPLIED MATERIALS: ../earth-grass-v2/README.txt
The newer two-texture blend is active in ordinary matches and training.
This file describes the earlier opt-in study.

RULE OF WAR — PHOTOGRAPHIC GROUND DEMONSTRATION

Launch from the project folder:
  View Photoreal Ground Demo.cmd
  View Current Ground Demo.cmd

Both use the normal pinned semi-orthographic camera, game sunlight, live
vehicle models, procedural scenery and dynamic shadows. The scene is paused
for comparison. Mouse wheel zooms; WASD / arrow keys pan; F10 restores the HUD;
Escape opens the menu. Graphics remain adjustable in the ordinary menu.
Both launchers start at the closer comparison distance; zoom out for the wider
view shown in comparison.html.

This demonstration is isolated behind --ground-demo. Ordinary launches retain
the current terrain. --ground-baseline selects the current materials in the
same comparison scene; --ground-close starts closer to the ground.

NEW MATERIALS
Two overhead base-colour images were made with the built-in image generation
tool: dry stony meadow and compacted gravel shoulder. Each has a generated
grayscale companion height map, converted numerically to shallow normal data
and gentle local cavity/roughness data. The original generated albedos were
copied byte for byte, without retouching. These are artistic approximations,
not measured photogrammetry/PBR scans. Exact prompts: generation.json.

The terrain shader blends three consistently offset samples of each new PBR
material to break grid repetition while retaining colour/normal alignment.
Contrast compensation uses the source textures' linear RGB averages. The
90-metre measured macro-relief layer remains in use. The training road mask
adds a gravel verge beyond its asphalt core in this opt-in demonstration.

Small stones and grass crevices can carry subtle baked shading. Vehicles,
aircraft, trees and large rocks remain real geometry with real shadows. There
are no painted vehicles or long moving-object shadows in these ground images.
No new collision relief, pathfinding obstacles or simulation rules were added.

The generated source images live in materials/pbr/ground_demo/. Versioned
runtime maps are materials/pbr/photo_meadow_v1_* and photo_shoulder_v1_*.
Numerical map compilation: tools/build_ground_demo_maps.py.
Render checks: tools/check_ground_demo.py. Comparison: comparison.html.
Fast helper syntax/type check: tools/check_ground_shader_helpers.py (uses the
already built Naga dependency, and checks rejection of the old spelling error).
