"""Verified 2K CC0 concrete sources; keep originals and physical map scale."""
from pathlib import Path
import hashlib, json, sys, zipfile
import numpy as np
from PIL import Image

ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'.data/vfx-tools/python-libs'))
import png
OUT=ROOT/'materials/pbr'
INCOMING=ROOT/'assets/incoming_textures'

def load(data):
    width,height,rows,info=png.Reader(bytes=data).asRGBA()
    values=np.vstack([np.asarray(row) for row in rows]).reshape(height,width,4)[:,:,:3]
    return values.astype(np.float32)/(2**info['bitdepth']-1)

def save(name,colour,normal,orm,strength):
    normal=normal*2-1
    normal[:,:,:2]*=strength
    normal/=np.maximum(np.linalg.norm(normal,axis=2,keepdims=True),1e-6)
    orm[:,:,2]=0
    hashes={}
    for suffix,values in [('albedo',colour),('normal',normal*.5+.5),('orm',orm)]:
        path=OUT/f'{name}_{suffix}.png'
        Image.fromarray(np.uint8(np.rint(np.clip(values,0,1)*255))).save(path)
        hashes[suffix]=hashlib.sha256(path.read_bytes()).hexdigest()
    return hashes

catalog=json.loads((OUT/'catalog.json').read_text())
source=INCOMING/'hangar_concrete_floor'
metadata=json.loads((source/'files.json').read_text())
maps={}
for key in ['Diffuse','nor_gl','arm']:
    m=metadata[key]['2k']['png'];data=(source/m['url'].split('/')[-1]).read_bytes()
    assert hashlib.md5(data).hexdigest()==m['md5']
    maps[key]=load(data)
archive=next(INCOMING.rglob('concrete_floor_worn_001_2k_PNG_PBR.zip'))
entry=catalog['materials']['concrete']
assert hashlib.sha256(archive.read_bytes()).hexdigest()==entry['source_archive_sha256']
with zipfile.ZipFile(archive) as z:
    worn_colour=load(z.read(next(n for n in z.namelist() if '_diff_2k.png' in n)))
def to_linear(v):return np.where(v<=.04045,v/12.92,((v+.055)/1.055)**2.4)
def to_srgb(v):return np.where(v<=.0031308,v*12.92,1.055*np.maximum(v,0)**(1/2.4)-.055)
# The hangar scan supplies pits and scuffs; retain the lighter outdoor concrete
# reflectance from the supplied worn slab, blending in linear light.
colour=to_srgb(to_linear(worn_colour)*.65+to_linear(maps['Diffuse'])*.35)
hashes=save('apron_concrete',colour,maps['nor_gl'],maps['arm'],.65)
catalog['materials']['apron_concrete']={
    'repeat_m':[6.,6.],'source_repeat_m':[2.,2.],'size':[2048,2048],
    'detail_scale_note':'Scan detail enlarged 3x for legibility at the fixed RTS camera distance.',
    'source':'https://polyhaven.com/a/hangar_concrete_floor',
    'author':'Dimitrios Savva / Poly Haven','license':'CC0 1.0 Universal',
    'normal':'OpenGL +Y','normal_strength':.65,'orm':'R=AO G=roughness B=0',
    'runtime_sha256':hashes,'source_maps':{key:metadata[key]['2k']['png'] for key in maps},
    'colour_blend':{'space':'linear RGB','concrete_floor_worn_001':.65,'hangar_concrete_floor':.35},
    'usage':'Ground-facing concrete only; world-space slab joints and sparse wear are shader layers.'}

with zipfile.ZipFile(archive) as z:
    def read(part):return load(z.read(next(n for n in z.namelist() if f'_{part}_2k.png' in n)))
    orm=np.dstack([read('ao')[:,:,0],read('rough')[:,:,0],np.zeros((2048,2048),dtype=np.float32)])
    entry['runtime_sha256']=save('concrete',read('diff'),read('nor_gl'),orm,.75)
entry['size']=[2048,2048]
catalog['runtime_max_size']=max(2048,catalog.get('runtime_max_size',0))
(OUT/'catalog.json').write_text(json.dumps(catalog,indent=2)+'\n')
print('Imported full 2K apron and architectural concrete maps; source checksums verified.')
