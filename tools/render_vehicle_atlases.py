"""Make packed Blender delivery and six lit 3D proofs using the new atlas maps."""
from pathlib import Path
import bpy,json,math
from mathutils import Vector
ROOT=Path(__file__).resolve().parents[1];OUT=ROOT/'exports/vehicle_atlases_2026-10-26'
KINDS={'long':'Support gun','xtralong':'Rocket artillery','troopCarrier':'Troop carrier','antiAir':'Anti-air vehicle','helicopter':'Apache','plane':'A-10'}
DATA=json.loads((OUT/'source/runtime_kit.json').read_text())
bpy.ops.wm.open_mainfile(filepath=str(OUT/'vehicles_textured.blend'))
scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.device='CPU';scene.cycles.samples=24;scene.cycles.use_denoising=True
scene.render.threads_mode='FIXED';scene.render.threads=4
scene.render.resolution_x=1024;scene.render.resolution_y=768;scene.render.resolution_percentage=100
scene.view_settings.view_transform='AgX';scene.render.image_settings.file_format='PNG'
scene.world.use_nodes=True;scene.world.node_tree.nodes.get('Background').inputs[0].default_value=(.38,.43,.51,1)
scene.world.node_tree.nodes.get('Background').inputs[1].default_value=.45
def xyz(p):return (p[0],-p[2],p[1])
for kind,label in KINDS.items():
    folder=OUT/kind;collection=bpy.data.collections[label]
    offset=next(iter(collection.objects)).location.copy()
    for obj in list(collection.objects):
        mat=obj.data.materials[0];tree=mat.node_tree;tree.nodes.clear()
        output=tree.nodes.new('ShaderNodeOutputMaterial');bsdf=tree.nodes.new('ShaderNodeBsdfPrincipled');tree.links.new(bsdf.outputs[0],output.inputs['Surface'])
        uv=tree.nodes.new('ShaderNodeUVMap');uv.uv_map='Atlas'
        def texture(name,data=False):
            n=tree.nodes.new('ShaderNodeTexImage');n.image=bpy.data.images.load(str(folder/(name+'.png')),check_existing=True)
            if data:n.image.colorspace_settings.name='Non-Color'
            tree.links.new(uv.outputs[0],n.inputs['Vector']);n.label=name;return n
        colour=texture('albedo');tree.links.new(colour.outputs['Color'],bsdf.inputs['Base Color'])
        normal=texture('normal',True);nmap=tree.nodes.new('ShaderNodeNormalMap');nmap.uv_map='Atlas'
        tree.links.new(normal.outputs['Color'],nmap.inputs['Color']);tree.links.new(nmap.outputs[0],bsdf.inputs['Normal'])
        orm=texture('orm',True);split=tree.nodes.new('ShaderNodeSeparateColor');tree.links.new(orm.outputs['Color'],split.inputs[0])
        tree.links.new(split.outputs['Green'],bsdf.inputs['Roughness']);tree.links.new(split.outputs['Blue'],bsdf.inputs['Metallic'])
        obj['atlas_kind']=kind
    asset=next(a for a in DATA['assets'] if a['name']==kind)
    for i,batch in enumerate(asset['batches']):
        if batch['finish'] not in {'Glass','AircraftGlass','Light'}:continue
        mesh=bpy.data.meshes.new(f'{kind}_retained_{i}');mesh.from_pydata([xyz(p) for p in batch['vertices']],[],[batch['indices'][j:j+3] for j in range(0,len(batch['indices']),3)]);mesh.update()
        obj=bpy.data.objects.new(f'{kind}_{batch["finish"]}_{i}',mesh);collection.objects.link(obj);obj.location=offset
        mat=bpy.data.materials.new(obj.name);mat.diffuse_color=(.04,.075,.09,1);mat.use_nodes=True
        bsdf=mat.node_tree.nodes.get('Principled BSDF');bsdf.inputs['Base Color'].default_value=(.025,.055,.068,1)
        bsdf.inputs['Roughness'].default_value=.16;bsdf.inputs['Metallic'].default_value=.3;bsdf.inputs['Coat Weight'].default_value=.45
        if batch['finish']=='Light':bsdf.inputs['Base Color'].default_value=(.7,.64,.43,1)
        mesh.materials.append(mat)
        mesh.normals_split_custom_set([xyz(batch['normals'][j]) for j in batch['indices']])
        for poly in mesh.polygons:poly.use_smooth=True
        obj['atlas_kind']=kind;obj['retained_material']=batch['finish'];obj['runtime_joint']=batch['joint']
bpy.ops.object.camera_add();camera=bpy.context.object;camera.name='Atlas review camera';camera.data.type='ORTHO';scene.camera=camera
bpy.ops.object.light_add(type='SUN');sun=bpy.context.object;sun.name='Warm fixed sunlight';sun.data.energy=3.0;sun.data.angle=math.radians(4);sun.data.color=(1,.87,.7)
sun.rotation_euler=Vector((-1,-1,-1.7)).to_track_quat('-Z','Y').to_euler()
bpy.ops.mesh.primitive_plane_add(size=200);ground=bpy.context.object;ground.name='Review ground'
mat=bpy.data.materials.new('Neutral warm review ground');mat.use_nodes=True;bsdf=mat.node_tree.nodes.get('Principled BSDF');bsdf.inputs['Base Color'].default_value=(.12,.135,.105,1);bsdf.inputs['Roughness'].default_value=.95;ground.data.materials.append(mat)
for kind,label in KINDS.items():
    objects=list(bpy.data.collections[label].objects)
    for obj in bpy.data.objects:
        if obj.type=='MESH' and obj!=ground:obj.hide_render=obj not in objects
    points=[obj.matrix_world@Vector(p) for obj in objects for p in obj.bound_box]
    low=Vector(tuple(min(p[i] for p in points) for i in range(3)));high=Vector(tuple(max(p[i] for p in points) for i in range(3)));center=(low+high)/2
    ground.location=(center.x,center.y,low.z-.035)
    direction=Vector((1,-1,1.7)).normalized();camera.location=center+direction*60;camera.rotation_euler=(center-camera.location).to_track_quat('-Z','Y').to_euler()
    inv=camera.matrix_world.inverted();bpy.context.view_layer.update();inv=camera.matrix_world.inverted();projected=[inv@p for p in points]
    width=max(p.x for p in projected)-min(p.x for p in projected);height=max(p.y for p in projected)-min(p.y for p in projected)
    camera.data.ortho_scale=max(width,height*4/3)*1.24
    scene.render.filepath=str(OUT/kind/'render.png');bpy.ops.render.render(write_still=True)
    print('VEHICLE_RENDER_READY',kind,flush=True)
for obj in bpy.data.objects:obj.hide_render=False
ground.hide_render=True
for image in list(bpy.data.images):
    if image.users==0:bpy.data.images.remove(image)
bpy.ops.file.pack_all()
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type=='VIEW_3D':
            area.spaces.active.shading.type='MATERIAL';area.spaces.active.region_3d.view_location=(22,12,0);area.spaces.active.region_3d.view_distance=55
bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'vehicles_textured.blend'))
print('ALL_VEHICLE_PROOFS_READY',flush=True)
