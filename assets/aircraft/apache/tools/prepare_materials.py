"""Compile restrained technical response maps; keep generated diffuse artwork unchanged."""
from pathlib import Path
import json,hashlib,shutil,struct
import numpy as np
from PIL import Image,ImageFilter
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'assets/aircraft/apache'
albedo=Image.open(OUT/'albedo.png').convert('RGB');size=albedo.size
ids=np.asarray(Image.open(OUT/'material_data.png').resize(size,Image.Resampling.NEAREST),dtype=np.float32)/255
colour=np.asarray(albedo,dtype=np.float32)/255
grey=Image.fromarray(np.uint8(colour.mean(2)*255));blur=np.asarray(grey.filter(ImageFilter.GaussianBlur(.8)),dtype=np.float32)/255
height=(np.asarray(grey,dtype=np.float32)/255-blur)*.23
dy,dx=np.gradient(height);covered=np.asarray(Image.fromarray(np.uint8(ids[:,:,3]*255)).filter(ImageFilter.MinFilter(5)))>254
normal=np.dstack([-dx*2,dy*2,np.ones_like(dx)]);normal/=np.linalg.norm(normal,axis=2,keepdims=True);normal[~covered]=[0,0,1]
Image.fromarray(np.uint8(np.clip(normal*.5+.5,0,1)*255+.5)).save(OUT/'normal.png')
rough=np.clip(ids[:,:,0]+height*.4,.28,.96);metal=ids[:,:,1];mask=ids[:,:,2]
orm=np.dstack([np.ones_like(rough),rough,metal,mask]);Image.fromarray(np.uint8(np.clip(orm,0,1)*255+.5)).save(OUT/'orm.png')
for detail,limit in [('low',512),('balanced',1024),('high',2048)]:
    dest=ROOT/'materials/vehicle_atlases'/detail
    for suffix in ['albedo','normal','orm']:
        image=Image.open(OUT/(suffix+'.png'));target=dest/f'helicopter_{suffix}.png'
        if max(image.size)>limit:
            image.thumbnail((limit,limit),Image.Resampling.LANCZOS)
            if suffix=='normal':
                v=np.asarray(image.convert('RGB'),dtype=np.float32)/127.5-1;v/=np.maximum(np.linalg.norm(v,axis=2,keepdims=True),.001);image=Image.fromarray(np.uint8(np.clip(v*.5+.5,0,1)*255+.5))
            image.save(target)
        else:shutil.copy2(OUT/(suffix+'.png'),target)
# Keep the existing five vehicle installations and update only helicopter entries.
catalog_path=ROOT/'materials/vehicle_atlases/catalog.json';catalog=json.loads(catalog_path.read_text())
for rec in catalog['maps']:
    if Path(rec['file']).name.startswith('helicopter_'):
        p=catalog_path.parent/rec['file'];rec['sha256']=hashlib.sha256(p.read_bytes()).hexdigest();rec['size']=list(Image.open(p).size)
catalog['apacheRevision']=2;catalog['apacheSource']='assets/aircraft/apache';catalog_path.write_text(json.dumps(catalog,indent=2))
print('APACHE_MATERIALS_READY',size,flush=True)
