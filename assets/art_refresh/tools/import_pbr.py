"""Build reproducible game maps from the user's untouched first delivery."""
from pathlib import Path
import hashlib, io, json, sys, zipfile
import numpy as np
from PIL import Image

root=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(root/'.data/vfx-tools/python-libs'))
import png

names=['grass','grass_dry','soil','asphalt','gravel','concrete','plaster','brick','roof_terracotta','roof_slate']
incoming=root/'assets/incoming_textures'
out=root/'materials/pbr'
out.mkdir(parents=True,exist_ok=True)
packs={}
for manifest in incoming.rglob('manifest.json'):
    for pack in json.loads(manifest.read_text())['packs']:
        packs[pack['index']]=pack

def load(data):
    if data[24]==16:
        width,height,rows,info=png.Reader(bytes=data).asRGBA()
        values=np.vstack([np.asarray(row,dtype=np.uint16) for row in rows]).reshape(height,width,4)
        return values.astype(np.float32)/65535
    return np.asarray(Image.open(io.BytesIO(data)).convert('RGBA'),dtype=np.float32)/255

def resize(data,size):
    return np.stack([np.asarray(Image.fromarray(data[:,:,i]).resize(size,Image.Resampling.LANCZOS)) for i in range(data.shape[2])],axis=2)

catalog=json.loads((out/'catalog.json').read_text()) if (out/'catalog.json').exists() else {'revision':1,'runtime_max_size':1024,'materials':{}}
for index,name in enumerate(names,1):
    pack=packs[index]
    archive=next(incoming.rglob(Path(pack['archive']).name))
    assert hashlib.sha256(archive.read_bytes()).hexdigest()==pack['archive_sha256'],archive
    with zipfile.ZipFile(archive) as z:
        def read(terms):
            entry=next(m for m in pack['maps'] if any(t in m['file'].lower() for t in terms))
            key=next(f for f in z.namelist() if Path(f).name==entry['file'])
            data=z.read(key)
            assert hashlib.sha256(data).hexdigest()==entry['sha256'],key
            return load(data)
        color=read(['_color.','_diff_'])[:,:,:3]
        size=(1024,round(1024*color.shape[0]/color.shape[1]))
        linear=np.where(color<=.04045,color/12.92,((color+.055)/1.055)**2.4)
        linear=np.maximum(resize(linear,size),0)
        color=np.where(linear<=.0031308,linear*12.92,1.055*linear**(1/2.4)-.055)
        normal=read(['_normalgl.','_nor_gl_'])[:,:,:3]*2-1
        normal=resize(normal,size)
        normal[:,:,:2]*=.55 if index<=5 else .75
        normal/=np.maximum(np.linalg.norm(normal,axis=2,keepdims=True),1e-6)
        normal=normal*.5+.5
        ao=resize(read(['_ambientocclusion.','_ao_'])[:,:,:1],size)[:,:,0]
        rough=resize(read(['_roughness.','_rough_'])[:,:,:1],size)[:,:,0]
        orm=np.dstack([ao,rough,np.zeros_like(rough)])
        hashes={}
        for suffix,data in [('albedo',color),('normal',normal),('orm',orm)]:
            path=out/f'{name}_{suffix}.png'
            Image.fromarray(np.uint8(np.rint(np.clip(data,0,1)*255))).save(path)
            hashes[suffix]=hashlib.sha256(path.read_bytes()).hexdigest()
    catalog['materials'][name]=dict(repeat_m=[pack['repeat_width_m'],pack['repeat_height_m']],
        size=list(size),source=pack['source_url'],author=pack['author'],license=pack['licence'],
        source_archive_sha256=pack['archive_sha256'],runtime_sha256=hashes,
        normal='OpenGL +Y',orm='R=AO G=roughness B=0',normal_strength=.55 if index<=5 else .75)
    print(name,size,'validated and packed',flush=True)
(out/'catalog.json').write_text(json.dumps(catalog,indent=2))
(out/'README.txt').write_text('Runtime derivatives of the original CC0 packs under assets/incoming_textures.\n'
    'Rebuild: Python assets/art_refresh/tools/import_pbr.py\n'
    'Source archives and PNGs remain untouched. See catalog.json for provenance, hashes, repeat sizes and channel layout.\n'
    'Base colour is sRGB; normal/ORM are linear. All ten materials are dielectric.\n'
    'Brick keeps its rectangular aspect. Roofs remain opaque geometry; source opacity is retained in the original ZIP.\n')
print('ALL TEN PBR SETS IMPORTED',flush=True)
