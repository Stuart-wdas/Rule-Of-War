"""Blender-authored bevels and weighted normals for native military kits."""
from pathlib import Path
import bpy,bmesh,json,math
from mathutils import Vector
ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'assets/art_refresh/refined_kit.json'
bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
result={}
def export(name,vertices,faces,bevel=0,smooth=False):
    mesh=bpy.data.meshes.new(name);mesh.from_pydata(vertices,[],faces);mesh.update()
    bm=bmesh.new();bm.from_mesh(mesh)
    bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(mesh);bm.free()
    obj=bpy.data.objects.new(name,mesh);bpy.context.collection.objects.link(obj)
    bpy.context.view_layer.objects.active=obj;obj.select_set(True)
    if bevel:
        mod=obj.modifiers.new('Machined edge radii','BEVEL');mod.width=bevel;mod.segments=3
        mod.affect='EDGES';mod.limit_method='ANGLE';mod.angle_limit=.15
        bpy.ops.object.modifier_apply(modifier=mod.name)
    for p in obj.data.polygons:p.use_smooth=True
    if not smooth:
        mod=obj.modifiers.new('Area weighted panel normals','WEIGHTED_NORMAL')
        mod.keep_sharp=True;mod.weight=50
        bpy.ops.object.modifier_apply(modifier=mod.name)
    m=obj.data;m.calc_loop_triangles()
    p,n,indices=[],[],[]
    for tri in m.loop_triangles:
        for index in tri.loops:
            loop=m.loops[index]
            p.append([round(v,7) for v in m.vertices[loop.vertex_index].co])
            n.append([round(v,7) for v in m.corner_normals[index].vector.normalized()])
            indices.append(len(indices))
    result[name]={'vertices':p,'normals':n,'indices':indices}
    print(name,len(indices)//3,'triangles',flush=True)
    obj.select_set(False)
foot=[(-.38,-.5),(.38,-.5),(.5,-.34),(.5,.24),(.30,.5),(-.30,.5),(-.5,.24),(-.5,-.34)]
for name,cabin in [('turret',False),('cabin',True)]:
    levels=[(-.5,.94,0),(-.18,1,0),(.5,.83,-.07)] if cabin else [(-.5,.87,0),(-.12,1,0),(.5,.70,-.09)]
    verts=[(x*s,y,z*s+shift) for y,s,shift in levels for x,z in foot]
    faces=[tuple(reversed(range(8))),tuple(range(16,24))]
    for r in range(2):
        for i in range(8):faces.append((r*8+i,r*8+(i+1)%8,(r+1)*8+(i+1)%8,(r+1)*8+i))
    export(name,verts,faces,.012 if cabin else .016)
profile=[(-.49,.30),(-.50,.36),(-.475,.415),(-.43,.46),(-.35,.49),(-.25,.50),
         (.25,.50),(.35,.49),(.43,.46),(.475,.415),(.50,.36),(.49,.30)]
segments=32
verts=[(math.sin(i*math.tau/segments)*r,y,math.cos(i*math.tau/segments)*r) for y,r in profile for i in range(segments)]
faces=[]
for row in range(len(profile)-1):
    for i in range(segments):faces.append((row*segments+i,row*segments+(i+1)%segments,(row+1)*segments+(i+1)%segments,(row+1)*segments+i))
export('tyre',verts,faces,smooth=True)
OUT.write_text(json.dumps(result,separators=(',',':')))
# Keep the authoring meshes and modifiers reproducible and inspectable.
for index,obj in enumerate([o for o in bpy.data.objects if o.type=='MESH']):
    obj.location.x=index*1.4
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type=='VIEW_3D':
            area.spaces.active.region_3d.view_location=(1.4,0,0)
            area.spaces.active.region_3d.view_distance=5
bpy.ops.wm.save_as_mainfile(filepath=str(OUT.with_suffix('.blend')))
print('REFINED_KIT_SAVED',OUT,flush=True)
