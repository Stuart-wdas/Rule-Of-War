"""Blender: unwrap exact runtime vehicle batches and bake UV-aligned source maps.

No game geometry is replaced. Exported per-triangle UVs are transferred back to
the exact original vertex/index streams, preserving rigid joints and contacts.
"""
from pathlib import Path
import bpy,bmesh,json,math,struct,sys
from collections import defaultdict

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'exports/vehicle_atlases_2026-10-26'
SOURCE=OUT/'source'
KINDS=['long','xtralong','troopCarrier','antiAir','helicopter','plane']
LABELS={'long':'Support gun','xtralong':'Rocket artillery','troopCarrier':'Troop carrier','antiAir':'Anti-air vehicle','helicopter':'Apache','plane':'A-10'}
SKIP={'Glass','AircraftGlass','Light'}
FINISH={'Team':'armour_green','Dark':'dark_fittings','Steel':'steel','Cloth':'canvas','Brass':'brass','Rust':'rust','Rubber':'rubber','A10Livery':'a10_livery'}
DATA=json.loads((SOURCE/'runtime_kit.json').read_text())
MATERIALS={s['name']:s for s in json.loads((SOURCE/'materials/runtime_materials.json').read_text())['sets']}
CONTACTS=json.loads((ROOT/'assets/art_refresh/model_contacts.json').read_text())
SIZE=2048

def signature(batch):
    value=14695981039346656037
    for field in ['vertices','normals']:
        for v in batch[field]:
            for n in v:
                for byte in struct.pack('<f',n):value=((value^byte)*1099511628211)&0xffffffffffffffff
    return f'{value:016x}'
def xyz(v):return (v[0],-v[2],v[1])
def key(v):return tuple(round(float(x),6) for x in v)
def face_key(points):return tuple(sorted(key(v) for v in points))
def new_image(name,path,data=False):
    image=bpy.data.images.new(name,SIZE,SIZE,alpha=True)
    image.colorspace_settings.name='Non-Color' if data else 'sRGB'
    image.filepath_raw=str(path);image.file_format='PNG'
    return image
def texture_material(kind,finish,target,technical=False):
    mat=bpy.data.materials.new(kind+'_'+finish);mat.use_nodes=True
    tree=mat.node_tree;tree.nodes.clear()
    output=tree.nodes.new('ShaderNodeOutputMaterial')
    emit=tree.nodes.new('ShaderNodeEmission');tree.links.new(emit.outputs[0],output.inputs['Surface'])
    label=FINISH.get(finish,'steel')
    if finish=='AircraftPaint':label='helicopter_green' if kind=='helicopter' else 'aircraft_grey'
    record=MATERIALS[label]
    uv=tree.nodes.new('ShaderNodeUVMap');uv.uv_map='Original'
    scale=tree.nodes.new('ShaderNodeVectorMath');scale.operation='MULTIPLY'
    matrix=record['uvMatrix'];translation=record['uvTranslation']
    assert abs(matrix[0][1])+abs(matrix[1][0])<.00001
    scale.inputs[1].default_value=(matrix[0][0],-matrix[1][1],1)
    add=tree.nodes.new('ShaderNodeVectorMath');add.operation='ADD'
    add.inputs[1].default_value=(translation[0],1-translation[1],0)
    tree.links.new(uv.outputs['UV'],scale.inputs[0]);tree.links.new(scale.outputs[0],add.inputs[0])
    image=tree.nodes.new('ShaderNodeTexImage')
    image.image=bpy.data.images.load(str(SOURCE/'materials'/label/'albedo.png'),check_existing=True)
    image.extension='REPEAT';tree.links.new(add.outputs[0],image.inputs['Vector'])
    tree.links.new(image.outputs['Color'],emit.inputs['Color'])
    bake=tree.nodes.new('ShaderNodeTexImage');bake.name='BAKE_TARGET';bake.image=target
    tree.nodes.active=bake
    return mat,emit,image

refresh='--refresh-turrets' in sys.argv
if refresh:
    KINDS=['long','xtralong','antiAir']
    bpy.ops.wm.open_mainfile(filepath=str(OUT/'vehicles_uv.blend'))
    for kind in KINDS:
        collection=bpy.data.collections[LABELS[kind]]
        for obj in list(collection.objects):bpy.data.objects.remove(obj,do_unlink=True)
        bpy.data.collections.remove(collection)
else:
    bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.device='CPU';scene.cycles.samples=1
scene.render.threads_mode='FIXED';scene.render.threads=2
scene.render.bake.margin=3;scene.render.bake.use_clear=True
scene.view_settings.view_transform='Standard'
all_mapping=json.loads((OUT/'uv_mapping.json').read_text()) if refresh else {'format':1,'uvConvention':'Bevy UV0: U right, V down; triangles in original index order','models':{}}
for kind in KINDS:
    folder=OUT/kind;folder.mkdir(parents=True,exist_ok=True)
    asset=next(a for a in DATA['assets'] if a['name']==kind)
    collection=bpy.data.collections.new(LABELS[kind]);scene.collection.children.link(collection)
    objects=[];sources=[];materials=[]
    albedo=new_image(kind+'_source',folder/'source_albedo.png')
    for batch_index,batch in enumerate(asset['batches']):
        if batch['finish'] in SKIP:continue
        verts=[xyz(v) for v in batch['vertices']]
        faces=[batch['indices'][i:i+3] for i in range(0,len(batch['indices']),3)]
        mesh=bpy.data.meshes.new(f'{kind}_{batch_index}');mesh.from_pydata(verts,[],faces);mesh.update()
        uv=mesh.uv_layers.new(name='Original')
        for poly,face in zip(mesh.polygons,faces):
            for loop,index in zip(poly.loop_indices,face):uv.data[loop].uv=batch['uv'][index]
        obj=bpy.data.objects.new(f'{batch_index}_{batch["finish"]}_joint{batch["joint"]}',mesh)
        collection.objects.link(obj)
        mat,emit,image=texture_material(kind,batch['finish'],albedo)
        mesh.materials.append(mat);materials.append((mat,emit,image,batch['finish']))
        # Weld only in the authoring copy so curved surfaces unwrap as connected
        # islands. Original runtime topology and custom normals remain untouched.
        bm=bmesh.new();bm.from_mesh(mesh)
        bmesh.ops.remove_doubles(bm,verts=list(bm.verts),dist=.00001)
        bm.to_mesh(mesh);bm.free();mesh.update()
        mesh.uv_layers.new(name='Atlas');mesh.uv_layers.active_index=1
        mesh.uv_layers['Atlas'].active_render=True
        objects.append(obj);sources.append((batch_index,batch))
    bpy.ops.object.select_all(action='DESELECT')
    for obj in objects:obj.select_set(True)
    bpy.context.view_layer.objects.active=objects[0]
    material_order=[obj.data.materials[0] for obj in objects]
    bpy.ops.object.join()
    bpy.ops.object.mode_set(mode='EDIT');bpy.ops.mesh.select_all(action='SELECT')
    bpy.ops.uv.smart_project(angle_limit=math.radians(66),island_margin=.005,
        area_weight=.25,correct_aspect=True,scale_to_bounds=True)
    bpy.ops.uv.average_islands_scale()
    bpy.ops.object.mode_set(mode='OBJECT')
    joined=bpy.context.object
    for poly in joined.data.polygons:
        paint=joined.data.materials[poly.material_index].name.split('_',1)[1].split('.')[0] in {'Team','AircraftPaint','A10Livery'}
        factor=2.5 if paint else .65
        for loop in poly.loop_indices:joined.data.uv_layers['Atlas'].data[loop].uv*=factor
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.uv.pack_islands(rotate=True,margin=.0008)
    bpy.ops.mesh.separate(type='MATERIAL')
    bpy.ops.object.mode_set(mode='OBJECT')
    by_material={obj.data.materials[0].name:obj for obj in bpy.context.selected_objects}
    objects=[by_material[mat.name] for mat in material_order]
    bpy.ops.object.bake(type='EMIT');albedo.save()
    bpy.ops.object.mode_set(mode='EDIT')
    bpy.ops.uv.export_layout(filepath=str(folder/'uv_layout.svg'),export_all=True,
        modified=False,mode='SVG',size=(SIZE,SIZE),opacity=.12)
    bpy.ops.object.mode_set(mode='OBJECT')
    # Deterministic PBR/material identity underlay. RGB = roughness, metallic,
    # paint mask. Keep it linear; the separate final ORM compiler adds micro wear.
    ids=new_image(kind+'_material_data',folder/'material_data.png',True)
    for mat,emit,image,finish in materials:
        for link in list(emit.inputs['Color'].links):mat.node_tree.links.remove(link)
        rough=.65 if finish in {'Team','AircraftPaint','A10Livery'} else .42
        metallic=.02 if finish in {'Team','AircraftPaint','A10Livery'} else .95
        if finish in {'Rubber','Cloth'}:rough=.94;metallic=0
        if finish=='Rust':rough=.94;metallic=.05
        paint=float(finish in {'Team','AircraftPaint','A10Livery'})
        emit.inputs['Color'].default_value=(rough,metallic,paint,1)
        mat.node_tree.nodes['BAKE_TARGET'].image=ids
    bpy.ops.object.bake(type='EMIT');ids.save()
    batches=[]
    for obj,(batch_index,batch) in zip(objects,sources):
        lookup={}
        uv=obj.data.uv_layers['Atlas'].data
        for poly in obj.data.polygons:
            points=[tuple(obj.data.vertices[obj.data.loops[i].vertex_index].co) for i in poly.loop_indices]
            lookup[face_key(points)]={key(p):tuple(uv[i].uv) for p,i in zip(points,poly.loop_indices)}
        mapped=[];missing=0
        for i in range(0,len(batch['indices']),3):
            points=[xyz(batch['vertices'][index]) for index in batch['indices'][i:i+3]]
            chart=lookup.get(face_key(points))
            if chart is None:missing+=1;continue
            for p in points:
                u,v=chart[key(p)];mapped.append([round(u,8),round(1-v,8)])
        assert missing==0,(kind,batch_index,'unmapped triangles',missing)
        assert len(mapped)==len(batch['indices'])
        batches.append({'finish':batch['finish'],'joint':batch['joint'],
            'geometryHash':signature(batch),'sourceVertexCount':len(batch['vertices']),
            'cornerUv':mapped})
    all_mapping['models'][kind]={'label':LABELS[kind],'batches':batches,
        'triangles':sum(len(b['cornerUv'])//3 for b in batches)}
    # Restore colour preview nodes and display all six models in one authoring file.
    for mat,emit,image,finish in materials:
        mat.node_tree.links.new(image.outputs['Color'],emit.inputs['Color'])
        atlas_uv=mat.node_tree.nodes.new('ShaderNodeUVMap');atlas_uv.uv_map='Atlas'
        mat.node_tree.links.new(atlas_uv.outputs['UV'],mat.node_tree.nodes['BAKE_TARGET'].inputs['Vector'])
    for obj in objects:
        obj.hide_render=True
    print('UV_ATLAS_READY',kind,len(batches),all_mapping['models'][kind]['triangles'],flush=True)
(OUT/'uv_mapping.json').write_text(json.dumps(all_mapping,separators=(',',':')))
for n,collection in enumerate([c for c in bpy.data.collections if c.name in LABELS.values()]):
    for obj in collection.objects:
        obj.hide_render=False;obj.location.x=(n%3)*22;obj.location.y=(n//3)*24
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type=='VIEW_3D':
            area.spaces.active.region_3d.view_location=(22,12,0)
            area.spaces.active.region_3d.view_distance=55
bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'vehicles_uv.blend'))
print('ALL_VEHICLE_UV_ATLASES_READY',flush=True)
