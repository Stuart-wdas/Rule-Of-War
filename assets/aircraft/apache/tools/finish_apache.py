"""Apply the reviewed atlas in Blender and render the actual authored model."""
from pathlib import Path
import bpy,math
from mathutils import Vector
OUT=Path(__file__).resolve().parents[1]
bpy.ops.wm.open_mainfile(filepath=str(OUT/'apache.blend'))
for mat in bpy.data.materials:
    if mat.name not in {'AircraftPaint','Steel','Dark','Rubber'}:continue
    tree=mat.node_tree;tree.nodes.clear();bsdf=tree.nodes.new('ShaderNodeBsdfPrincipled');output=tree.nodes.new('ShaderNodeOutputMaterial');tree.links.new(bsdf.outputs[0],output.inputs['Surface'])
    uv=tree.nodes.new('ShaderNodeUVMap');uv.uv_map='Atlas'
    nodes={}
    for suffix in ['albedo','normal','orm']:
        node=tree.nodes.new('ShaderNodeTexImage');node.image=bpy.data.images.load(str(OUT/(suffix+'.png')),check_existing=True)
        if suffix!='albedo':node.image.colorspace_settings.name='Non-Color'
        tree.links.new(uv.outputs[0],node.inputs['Vector']);nodes[suffix]=node
    tree.links.new(nodes['albedo'].outputs['Color'],bsdf.inputs['Base Color'])
    normal=tree.nodes.new('ShaderNodeNormalMap');normal.uv_map='Atlas';tree.links.new(nodes['normal'].outputs['Color'],normal.inputs[1]);tree.links.new(normal.outputs[0],bsdf.inputs['Normal'])
    split=tree.nodes.new('ShaderNodeSeparateColor');tree.links.new(nodes['orm'].outputs['Color'],split.inputs[0]);tree.links.new(split.outputs['Green'],bsdf.inputs['Roughness']);tree.links.new(split.outputs['Blue'],bsdf.inputs['Metallic'])
scene=bpy.context.scene;camera=scene.camera;scene.render.threads_mode='FIXED';scene.render.threads=4
for name,direction in [('textured-review',(1,-1,1.3)),('textured-side',(1,0,.18))]:
    center=Vector((0,.5,1));camera.location=center+Vector(direction).normalized()*20;camera.rotation_euler=(center-camera.location).to_track_quat('-Z','Y').to_euler();camera.data.ortho_scale=7.2
    scene.render.filepath=str(OUT/(name+'.png'));bpy.ops.render.render(write_still=True)
bpy.ops.file.pack_all();bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'apache.blend'))
print('TEXTURED_APACHE_READY',flush=True)
