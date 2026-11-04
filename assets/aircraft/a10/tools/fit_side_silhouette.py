"""Second side-profile authoring pass; absolute fuselage/canopy stations."""
from pathlib import Path
import json
import bpy
ROOT=Path(__file__).resolve().parents[1]
bpy.ops.wm.open_mainfile(filepath=str(ROOT/"a10.blend"))
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
replace("Fuselage",namespace["loft"]("Side silhouette fuselage",[
    (-2.13,.035,.53,.63),(-1.90,.075,.53,.70),(-1.50,.14,.53,.90),
    (-.70,.215,.54,.95),(.10,.23,.54,.98),(.85,.23,.54,1.02),
    (1.45,.225,.54,1.00),(1.94,.19,.54,.92),(2.23,.135,.54,.81),(2.40,.05,.55,.73)]))
stations=[[1.02,.055,1.015,1.03],[1.18,.115,1.01,1.055],[1.45,.13,.985,1.13],
          [1.64,.12,.955,1.17],[1.82,.09,.915,1.08],[2.00,.04,.89,.905]]
replace("Canopy",namespace["loft"]("Side silhouette canopy",stations,12,True))
bpy.context.scene["canopy_stations"]=json.dumps(stations)
bpy.context.scene["side_refinement_version"]=2
for obj in bpy.data.objects:obj.select_set(False)
bpy.context.preferences.filepaths.save_version=0
bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/"a10.blend"))
namespace["export"]()
print("A10_SILHOUETTE_FIT",flush=True)
