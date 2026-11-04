"""Render actual Blender geometry over the packed, unmodified reference image."""
from pathlib import Path
import math
import bpy
from mathutils import Vector

ROOT=Path(__file__).resolve().parents[1]
bpy.ops.wm.open_mainfile(filepath=str(ROOT/"a10.blend"))
scene=bpy.context.scene
overlay=bpy.data.collections.new("OVERLAY | temporary render")
scene.collection.children.link(overlay)
ink=bpy.data.materials.new("Overlay ink")
ink.use_nodes=True
nodes=ink.node_tree.nodes;nodes.clear()
out=nodes.new("ShaderNodeOutputMaterial")
em=nodes.new("ShaderNodeEmission");em.inputs[0].default_value=(.02,.32,.48,1)
ink.node_tree.links.new(em.outputs[0],out.inputs[0])
source_collections=[bpy.data.collections["SURFACES | editable game source"]]
if bpy.data.collections.get("MECHANICAL | native pivots and sockets"):
    bpy.data.collections["MECHANICAL | native pivots and sockets"].hide_render=True
for col in source_collections:
    for obj in col.objects:
        if obj.type!="MESH":continue
        mesh=obj.data.copy();mesh.materials.clear();mesh.materials.append(ink)
        wire=bpy.data.objects.new(obj.name+"_overlay",mesh)
        overlay.objects.link(wire);wire.matrix_world=obj.matrix_world.copy()
        mod=wire.modifiers.new("Wire outline","WIREFRAME")
        mod.thickness=.0018;mod.use_replace=True
    col.hide_render=True

picture=bpy.data.materials.new("Original reference image")
picture.use_nodes=True
n=picture.node_tree.nodes;n.clear()
out=n.new("ShaderNodeOutputMaterial");em=n.new("ShaderNodeEmission")
tex=n.new("ShaderNodeTexImage")
tex.image=next(im for im in bpy.data.images if im.name.startswith("user_three_view"))
picture.node_tree.links.new(tex.outputs["Color"],em.inputs[0])
picture.node_tree.links.new(em.outputs[0],out.inputs[0])
planes=[]
for empty in bpy.data.collections["REFERENCES | user three-view, not exported"].objects:
    w=empty.empty_display_size;h=w*386/474
    mesh=bpy.data.meshes.new("Reference card")
    mesh.from_pydata([(0,0,0),(w,0,0),(w,h,0),(0,h,0)],[],[(0,1,2,3)])
    uv=mesh.uv_layers.new()
    for i,co in enumerate([(0,0),(1,0),(1,1),(0,1)]):uv.data[i].uv=co
    mesh.materials.append(picture)
    plane=bpy.data.objects.new(empty.name+"_card",mesh)
    overlay.objects.link(plane);plane.matrix_world=empty.matrix_world.copy()
    plane.hide_render=True
    planes.append(plane)
scene.render.engine="CYCLES";scene.cycles.samples=8
scene.render.resolution_x=1400;scene.render.resolution_y=1050
scene.view_settings.view_transform="Standard"
for name,cam,scale in [("top","REFERENCE_TOP",7.0),("side","REFERENCE_SIDE",5.7),("front","REFERENCE_FRONT",6.0)]:
    for plane in planes:plane.hide_render=not plane.name.lower().startswith(name)
    scene.camera=bpy.data.objects[cam];scene.camera.data.ortho_scale=scale
    scene.render.filepath=str(ROOT/"previews"/("reference_"+name+".png"))
    bpy.ops.render.render(write_still=True)
    print("REFERENCE_OVERLAY",name,flush=True)
# Render-only cards and wireframe copies are deliberately not saved into source.
