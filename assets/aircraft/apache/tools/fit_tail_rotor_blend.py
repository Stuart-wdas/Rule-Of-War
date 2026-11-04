"""Apply the same UV-preserving clearance correction to the editable Blender mesh."""
from pathlib import Path
import bpy
from mathutils import Vector
OUT=Path(__file__).resolve().parents[1];bpy.ops.wm.open_mainfile(filepath=str(OUT/'apache.blend'))
obj=bpy.data.objects['Apache authored airframe'];mesh=obj.data
assert not obj.get('tail_rotor_fit_applied')
selected={mesh.loops[i].vertex_index for p in mesh.polygons if mesh.attributes['runtime_joint'].data[p.index].value==33 for i in p.loop_indices}
pivot=Vector((-.15,3.28,1.43))
normals=[n.vector.copy() for n in mesh.corner_normals]
for i in selected:
    p=mesh.vertices[i].co;p.y=pivot.y+(p.y-pivot.y)*.9;p.z=pivot.z+(p.z-pivot.z)*.9
for i,loop in enumerate(mesh.loops):
    if loop.vertex_index in selected:normals[i].y/=.9;normals[i].z/=.9;normals[i].normalize()
mesh.normals_split_custom_set(normals);obj['tail_rotor_fit_applied']=True
bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'apache.blend'))
print('BLENDER_CLEARANCE_FIT_READY')
