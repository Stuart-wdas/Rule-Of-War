"""Author a packed, UV-mapped matte service livery on the existing A-10.
Run with Blender --background a10.blend --python this_script.py.
Geometry and reference calibration are preserved.
"""
from pathlib import Path
import math, sys, shutil
import bpy
import numpy as np
ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parents[2]
backup = ROOT / 'a10_pre_livery.blend'
if not backup.exists(): shutil.copy2(ROOT / 'a10.blend', backup)
S = 512
rng = np.random.default_rng(1882)
atlas = np.ones((S*2, S*3, 4), np.float32)
glyphs = {
 'F':['11111','10000','10000','11110','10000','10000','10000'],
 'T':['11111','00100','00100','00100','00100','00100','00100'],
 '0':['01110','11011','11011','11011','11011','11011','01110'],
 '8':['01110','11011','11011','01110','11011','11011','01110'],
 '2':['01110','11011','00011','00110','01100','11000','11111'],
 'N':['10001','11001','11001','10101','10011','10011','10001'],
 'O':['01110','11011','11011','11011','11011','11011','01110'],
 'S':['01111','11000','11000','01110','00011','00011','11110'],
 'E':['11111','10000','10000','11110','10000','10000','11111'],
 'P':['11110','10001','10001','11110','10000','10000','10000'],
 ' ':['00000']*7,
}
def lettering(tile, text, x, y, scale=3, color=(.26,.30,.31)):
    for char in text:
        for row,bits in enumerate(glyphs[char]):
            for col,bit in enumerate(bits):
                if bit=='1': tile[y+(6-row)*scale:y+(7-row)*scale,x+col*scale:x+(col+1)*scale,:3]=color
        x += 6*scale

for index in range(6):
    tile=atlas[index//3*S:(index//3+1)*S,index%3*S:(index%3+1)*S]
    yy,xx=np.mgrid[:S,:S]
    grain=rng.normal(0,.004,(S,S))
    broad=.012*np.sin(xx*.023+np.sin(yy*.017))+.008*np.cos(yy*.031)
    base=np.array((.58,.625,.64)) if index in (0,3,4) else np.array((.62,.655,.66))
    tile[:,:,:3]=base+(grain+broad)[:,:,None]
    # Fine recessed panels, restrained edge wear and fasteners; no woven texture.
    for x in [48,148,248,348,448]:
        tile[:,x:x+1,:3]*=.60
        tile[:,x+1:x+2,:3]*=1.035
        for y in range(25,500,18): tile[y:y+2,x+3:x+5,:3]*=.64
    for y in [92,214,356,455]:
        tile[y:y+1,:,:3]*=.63
        tile[y+1:y+2,:,:3]*=1.025
    if index==0:
        tile[:95,:,:3]*=.89
        # Radome and anti-glare paint ahead of the canopy.
        tile[170:350,466:,:3]=(.22,.26,.27)
        tile[195:310,352:451,:3]*=.72
        lettering(tile,'082',155,242,3)
    if index==1:
        tile[120:430,222:290,:3]*=.72
        for x in [105,375]:
            tile[200:206,x-27:x+28,:3]=(.32,.36,.37)
            tile[178:228,x-9:x+10,:3]=(.32,.36,.37)
        lettering(tile,'NO STEP',302,352,2)
    if index==3:
        tile[423:452,25:485,:3]=(.25,.36,.40)
        tile[420:423,25:485,:3]=(.76,.77,.72)
        lettering(tile,'FT',164,310,9)
        lettering(tile,'082',155,226,7)
    if index==4:
        tile[:,65:96,:3]*=.72
        tile[:,415:,:3]*=.70
        tile[:,410:414,:3]*=.83
    tile[:,:,:3]=np.clip(tile[:,:,:3],0,1)

image=bpy.data.images.new('A10 | service grey / panels / stencils',width=S*3,height=S*2,alpha=True)
image.pixels.foreach_set(atlas.ravel())
image.filepath_raw=str(REPO/'materials/a10_livery.png')
image.file_format='PNG'; image.save(); image.pack()
mat=bpy.data.materials.get('A10 | Service grey livery') or bpy.data.materials.new('A10 | Service grey livery')
mat.use_nodes=True
bsdf=mat.node_tree.nodes.get('Principled BSDF')
bsdf.inputs['Roughness'].default_value=.78
bsdf.inputs['Metallic'].default_value=.08
bsdf.inputs['Specular IOR Level'].default_value=.22
tex=mat.node_tree.nodes.new('ShaderNodeTexImage');tex.image=image
mat.node_tree.links.new(tex.outputs['Color'],bsdf.inputs['Base Color'])
shape_order=['A10Body','A10Wing','A10Tail','A10Fin','JetNacelle','A10Canopy']
for obj in [o for o in bpy.data.objects if o.get('runtime_shape') in shape_order]:
    index=shape_order.index(obj['runtime_shape']); mesh=obj.data
    uv=mesh.uv_layers.active or mesh.uv_layers.new(name='Service livery')
    local=obj['runtime_shape'] in ('A10Fin','JetNacelle')
    coords=[v.co.copy() if local else obj.matrix_world @ v.co for v in mesh.vertices]
    points=np.array([[v.x,v.z,-v.y] for v in coords])
    low=points.min(axis=0); span=np.maximum(points.max(axis=0)-low,.00001)
    for poly in mesh.polygons:
        for li in poly.loop_indices:
            at=points[mesh.loops[li].vertex_index]
            if index in (1,2,5): u,v=(at[0]-low[0])/span[0],(at[2]-low[2])/span[2]
            elif index==0 and abs(poly.normal.z)>.65:
                u,v=(at[2]-low[2])/span[2],(at[0]-low[0])/span[0]
            else: u,v=(at[2]-low[2])/span[2],(at[1]-low[1])/span[1]
            if index==3 and poly.normal.x>0: u=1-u
            # Eight-pixel gutters keep mip filtering inside each surface tile.
            uv.data[li].uv=((index%3+(8+u*496)/S)/3,(index//3+(8+v*496)/S)/2)
    if index!=5:
        mesh.materials.clear();mesh.materials.append(mat)

scene=bpy.context.scene
scene['livery_notes']='Matte service grey, countershading, anti-glare radome, panel wear, FT 082 tail and low-visibility wing markings. Packed atlas exported with corner UVs.'
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type=='VIEW_3D':
            area.spaces.active.shading.type='MATERIAL'
            area.spaces.active.shading.show_xray=False
            area.spaces.active.overlay.show_overlays=False
scene.camera=bpy.data.objects['BEAUTY']
scene.render.engine='CYCLES';scene.cycles.samples=24;scene.cycles.use_denoising=True
scene.render.resolution_x=1280;scene.render.resolution_y=960;scene.render.resolution_percentage=100
scene.view_settings.view_transform='AgX'
bpy.context.preferences.filepaths.save_version=0
bpy.data.texts['EXPORT_A10.py'].clear();bpy.data.texts['EXPORT_A10.py'].write((ROOT/'tools/export_a10.py').read_text(encoding='utf-8'))
bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/'a10.blend'))
sys.path.insert(0,str(ROOT/'tools'))
from export_a10 import export
export()
for name,cam in [('a10_livery','BEAUTY'),('a10_livery_side','REFERENCE_SIDE')]:
    scene.camera=bpy.data.objects[cam];scene.render.filepath=str(ROOT/'previews'/f'{name}.png')
    bpy.ops.render.render(write_still=True)
print('A10_LIVERY_COMPLETE',flush=True)
