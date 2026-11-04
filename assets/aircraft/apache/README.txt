RULE OF WAR — REBUILT APACHE AIRFRAME

Original authored AH-64-inspired geometry: 17,712 triangles, with curved fuselage
and engine housings, stepped tandem cockpit, slender tail boom, swept rotor blades,
scissor tail rotor, detailed rotor head, recessed rocket tubes, missile fins, nose
optics, fixed undercarriage and an independently aiming chin gun.

Open apache.blend to inspect the editable mesh and packed materials. The face
attribute runtime_joint identifies the main rotor (30), tail rotor (33), aiming gun
(20) and stationary airframe (0). apache_geometry.json is the native source payload.
Atlas UVs use V-down in the native JSON and V-up in the Blender file. The game flips
the normal-map green channel when using the native UVs.

albedo.png is the unmodified built-in image-generation result. generation.json
contains its exact prompt and provenance. normal.png and orm.png are technical,
approximate surface-response maps; the ORM alpha channel identifies painted parts
for masked faction tinting. Camouflage colour is not used as large-scale height.
Low, Balanced and High runtime variants are in materials/vehicle_atlases.

The native model retains the original gameplay scale and fits the existing airfield
stand. Tail-rotor radius has a 10% clearance correction, preserving the texture UVs.

REPRODUCTION
Run tools/build_apache.py with Blender 5.2 in background mode. It includes the
clearance correction. After artwork changes, run tools/prepare_materials.py with
ordinary Python, then tools/finish_apache.py with Blender. Export the exact runtime
kit using the rebuilt native executable's --export-art option into runtime/, then
run tools/register_runtime_uv.py and bake the helicopter contacts with
assets/art_refresh/tools/bake_model_contacts.py -- runtime/runtime_kit.json --only helicopter.

INSPIRATION / REFERENCE PHOTOGRAPHS
https://www.boeing.com/defense/military-rotorcraft/ah-64-apache
https://www.army.mod.uk/learn-and-explore/equipment/aircraft/ah-64e-apache-attack-helicopter/
Photographs in references/ are retained as design references; their copyright
belongs to the respective source. They are not used as game textures.
