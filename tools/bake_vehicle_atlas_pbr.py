"""Bake existing physical surface detail into the new UVs, keeping AI colour untouched."""
from pathlib import Path
import bpy,json,math
from mathutils import Vector
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'exports/vehicle_atlases_2026-10-26'
KINDS={'long':'Support gun','xtralong':'Rocket artillery','troopCarrier':'Troop carrier','antiAir':'Anti-air vehicle','helicopter':'Apache','plane':'A-10'}
FINISH={'Team':'armour_green','Dark':'dark_fittings','Steel':'steel','Cloth':'canvas','Brass':'brass','Rust':'rust','Rubber':'rubber','A10Livery':'a10_livery'}
DATA=json.loads((OUT/'source/runtime_kit.json').read_text())
META={s['name']:s for s in json.loads((OUT/'source/materials/runtime_materials.json').read_text())['sets']}
bpy.ops.wm.open_mainfile(filepath=str(OUT/'vehicles_uv.blend'))
scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.device='CPU';scene.cycles.samples=1
scene.render.threads_mode='FIXED';scene.render.threads=2;scene.render.bake.margin=4
def key(p):return tuple(round(float(v),5) for v in p)
def xyz(p):return (p[0],-p[2],p[1])
def facekey(points):return tuple(sorted(key(p) for p in points))
for kind,label in KINDS.items():
    folder=OUT/kind;objects=list(bpy.data.collections[label].objects)
    for obj in bpy.data.objects:obj.hide_render=True;obj.select_set(False)
    lookup={}
    asset=next(a for a in DATA['assets'] if a['name']==kind)
    for batch in asset['batches']:
        for i in range(0,len(batch['indices']),3):
            indices=batch['indices'][i:i+3];points=[xyz(batch['vertices'][j]) for j in indices]
            lookup[(batch['finish'],facekey(points))]={key(p):xyz(batch['normals'][j]) for p,j in zip(points,indices)}
    mats=[]
    for obj in objects:
        obj.hide_render=False;obj.select_set(True)
        mat=obj.data.materials[0];finish=mat.name.split('_',1)[1].split('.')[0]
        # Restore exact exported split normals after welding the authoring copy.
        normals=[]
        for poly in obj.data.polygons:
            points=[obj.data.vertices[obj.data.loops[i].vertex_index].co for i in poly.loop_indices]
            record=lookup[(finish,facekey(points))];normals.extend(record[key(p)] for p in points);poly.use_smooth=True
        obj.data.normals_split_custom_set(normals)
        tree=mat.node_tree;tree.nodes.clear()
        out=tree.nodes.new('ShaderNodeOutputMaterial');bsdf=tree.nodes.new('ShaderNodeBsdfPrincipled');tree.links.new(bsdf.outputs['BSDF'],out.inputs['Surface'])
        atlas=tree.nodes.new('ShaderNodeUVMap');atlas.uv_map='Atlas'
        original=tree.nodes.new('ShaderNodeUVMap');original.uv_map='Original'
        name=FINISH.get(finish,'steel')
        if finish=='AircraftPaint':name='helicopter_green' if kind=='helicopter' else 'aircraft_grey'
        metadata=META[name];matrix=metadata['uvMatrix'];translation=metadata['uvTranslation']
        scale=tree.nodes.new('ShaderNodeVectorMath');scale.operation='MULTIPLY';scale.inputs[1].default_value=(matrix[0][0],-matrix[1][1],1)
        add=tree.nodes.new('ShaderNodeVectorMath');add.operation='ADD';add.inputs[1].default_value=(translation[0],1-translation[1],0)
        tree.links.new(original.outputs[0],scale.inputs[0]);tree.links.new(scale.outputs[0],add.inputs[0])
        def texture(path,uv,data=False):
            node=tree.nodes.new('ShaderNodeTexImage');node.image=bpy.data.images.load(str(path),check_existing=True)
            if data:node.image.colorspace_settings.name='Non-Color'
            tree.links.new(uv,node.inputs['Vector']);return node
        colour=texture(folder/'albedo.png',atlas.outputs[0]);tree.links.new(colour.outputs['Color'],bsdf.inputs['Base Color'])
        physical_name=name if (OUT/'source/materials'/name/'normal.png').exists() else 'aircraft_grey'
        source_normal=texture(OUT/'source/materials'/physical_name/'normal.png',add.outputs[0],True)
        normal=tree.nodes.new('ShaderNodeNormalMap');normal.uv_map='Original';tree.links.new(source_normal.outputs['Color'],normal.inputs['Color']);tree.links.new(normal.outputs[0],bsdf.inputs['Normal'])
        source_orm=texture(OUT/'source/materials'/physical_name/'orm.png',add.outputs[0],True)
        bake=tree.nodes.new('ShaderNodeTexImage');bake.name='BAKE_TARGET';tree.nodes.active=bake
        mats.append((mat,bsdf,out,atlas,normal,source_orm,bake))
    bpy.context.view_layer.objects.active=objects[0]
    for suffix in ['normal','base_orm']:
        if (folder/(suffix+'.png')).exists():continue
        image=bpy.data.images.new(kind+'_'+suffix,2048,2048,alpha=False);image.colorspace_settings.name='Non-Color'
        image.filepath_raw=str(folder/(suffix+'.png'));image.file_format='PNG'
        for mat,bsdf,out,atlas,normal,source_orm,bake in mats:
            bake.image=image;mat.node_tree.nodes.active=bake
            if suffix=='base_orm':
                emit=mat.node_tree.nodes.new('ShaderNodeEmission');mat.node_tree.links.new(source_orm.outputs['Color'],emit.inputs['Color']);mat.node_tree.links.new(emit.outputs[0],out.inputs['Surface'])
        bpy.ops.object.bake(type='NORMAL' if suffix=='normal' else 'EMIT');image.save()
    for mat,bsdf,out,atlas,normal,source_orm,bake in mats:
        tree=mat.node_tree;tree.links.new(bsdf.outputs[0],out.inputs['Surface'])
        n=tree.nodes.new('ShaderNodeTexImage');n.image=bpy.data.images.load(str(folder/'normal.png'),check_existing=True);tree.links.new(atlas.outputs[0],n.inputs['Vector'])
        normal.uv_map='Atlas';tree.links.new(n.outputs['Color'],normal.inputs['Color'])
    print('PBR_BAKED',kind,flush=True)
for obj in bpy.data.objects:obj.hide_render=False
bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'vehicles_textured.blend'))
print('ALL_PBR_BAKES_READY',flush=True)
