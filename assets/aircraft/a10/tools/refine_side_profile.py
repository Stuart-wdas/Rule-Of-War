"""Refine the existing editable source against the supplied side-view feedback.

Keeps the original reference image and other source collections intact. This is
an authoring operation, not part of the regular save/export workflow.
"""
from pathlib import Path
import json
import sys
import bpy

ROOT=Path(__file__).resolve().parents[1]
bpy.ops.wm.open_mainfile(filepath=str(ROOT/"a10.blend"))
if bpy.context.scene.get("side_refinement_version",0)>=1:
    print("Side refinement already applied; use export_a10.py for normal exports.",flush=True)
    sys.exit(0)
# Reuse the Blender mesh construction helpers without running the bootstrap.
namespace={"__file__":str(ROOT/"tools/build_reference_a10.py")}
exec((ROOT/"tools/build_reference_a10.py").read_text().split('if "--sync-details" in sys.argv:')[0],namespace)
namespace["surfaces"]=bpy.data.collections["SURFACES | editable game source"]

def replace(old_name,new):
    old=bpy.data.objects[old_name]
    materials=list(old.data.materials)
    old.data=new.data
    old.data.materials.clear()
    for mat in materials:old.data.materials.append(mat)
    bpy.data.objects.remove(new,do_unlink=True)

replace("Fuselage",namespace["loft"]("Refined fuselage",[
    (-2.13,.035,.53,.63),(-1.90,.075,.52,.67),(-1.50,.14,.47,.85),
    (-.70,.215,.43,.89),(.10,.23,.42,.90),(.85,.23,.42,.93),
    (1.45,.225,.40,.91),(1.94,.19,.42,.82),(2.23,.135,.44,.65),(2.40,.05,.445,.52)]))
stations=json.loads(bpy.context.scene["canopy_stations"])
stations=[[round(z+.05,5),w,b,max(b+.015,t-.02)] for z,w,b,t in stations]
replace("Canopy",namespace["loft"]("Refined canopy",stations,12,True))
bpy.context.scene["canopy_stations"]=json.dumps(stations)
# The horizontal stabilizer belongs near the lower part of the fins; the old
# surface crossed too high through the tail and the entire tail ran too far aft.
tail=bpy.data.objects["Horizontal tail"]
for v in tail.data.vertices:
    v.co.y-=.13
    v.co.z-=.13
for name in ["Fin.R","Fin.L"]:bpy.data.objects[name].location.y-=.13
for name in ["Engine.R","Engine.L"]:bpy.data.objects[name].location.z-=.02

# Calibrate the side reference using the barrel outlet, not the fuselage tip.
# The original low-resolution drawing is preserved, including attribution.
side=next(o for o in bpy.data.collections["REFERENCES | user three-view, not exported"].objects if o.name.startswith("SIDE"))
s=4.69/180
side.location.y=-2.4785-29*s
side.location.z=-(386-292)*s
bpy.context.scene["side_alignment_note"]="Side image aligned to GAU-8 outlet and main tire baseline; original views have small drafting discrepancies."
feedback=bpy.data.images.load(str(ROOT/"references/user_side_feedback.png"),check_existing=True)
feedback.pack()
for obj in bpy.data.objects:
    obj.select_set(False)
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type=="VIEW_3D":
            space=area.spaces.active
            space.region_3d.view_rotation=bpy.data.objects["REFERENCE_SIDE"].rotation_euler.to_quaternion()
            space.region_3d.view_perspective="ORTHO"
            space.region_3d.view_location=(0,0,.65)
            space.region_3d.view_distance=5.8
            space.overlay.show_extras=True
bpy.context.scene["side_refinement_version"]=1
for obj in bpy.data.objects:
    if obj.type in {"CAMERA","LIGHT"} or (obj.type=="EMPTY" and obj.empty_display_type!="IMAGE"):
        obj.hide_set(True)
bpy.context.preferences.filepaths.save_version=0
bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/"a10.blend"))
namespace["export"]()
print("A10_SIDE_REFINED",flush=True)
