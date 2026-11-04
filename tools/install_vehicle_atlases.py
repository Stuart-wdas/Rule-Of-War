"""Install reviewed colour maps unchanged; pack technical paint mask into ORM alpha."""
from pathlib import Path
import json,shutil,hashlib,subprocess,sys
import numpy as np
from PIL import Image
ROOT=Path(__file__).resolve().parents[1];SOURCE=ROOT/'exports/vehicle_atlases_2026-10-26';OUT=ROOT/'materials/vehicle_atlases'
OUT.mkdir(parents=True,exist_ok=True)
manifest=json.loads((SOURCE/'manifest.json').read_text());shutil.copy2(SOURCE/'uv_mapping.json',OUT/'uv_mapping.json')
records=[]
for detail,size in [('low',512),('balanced',1024),('high',2048)]:
    folder=OUT/detail;folder.mkdir(exist_ok=True)
    for vehicle in manifest['vehicles']:
        kind=vehicle['kind'];source=SOURCE/kind
        apache=ROOT/'assets/aircraft/apache'
        revised=kind=='helicopter' and (apache/'albedo.png').exists() and (apache/'runtime/runtime_kit.json').exists()
        if revised:source=apache
        for suffix in ['albedo','normal','orm']:
            image=Image.open(source/(suffix+'.png'))
            target=folder/f'{kind}_{suffix}.png'
            if suffix=='orm' and not revised:image=image.convert('RGBA');image.putalpha(Image.open(source/'paint_mask.png').convert('L'))
            if max(image.size)>size:
                image.thumbnail((size,size),Image.Resampling.LANCZOS)
                if suffix=='normal':
                    v=np.array(image.convert('RGB'),dtype=np.float32)/127.5-1
                    v/=np.maximum(np.linalg.norm(v,axis=2,keepdims=True),.001)
                    image=Image.fromarray(np.uint8(np.clip(v*.5+.5,0,1)*255+.5))
            if detail=='high' and suffix in {'albedo','normal'}:shutil.copy2(source/(suffix+'.png'),target)
            else:image.save(target)
            records.append({'file':str(target.relative_to(OUT)),'size':list(image.size),'sha256':hashlib.sha256(target.read_bytes()).hexdigest()})
(OUT/'catalog.json').write_text(json.dumps({'revision':1,'source':str(SOURCE.relative_to(ROOT)),
    'ormChannels':{'R':'occlusion','G':'roughness','B':'metallic','A':'faction paint mask'},
    'normalFlipY':True,'maps':records},indent=2))
if (ROOT/'assets/aircraft/apache/runtime/runtime_kit.json').exists():
    subprocess.run([sys.executable,str(ROOT/'assets/aircraft/apache/tools/register_runtime_uv.py')],check=True,cwd=ROOT)
print('Installed',len(records),'maps across three texture quality settings')
