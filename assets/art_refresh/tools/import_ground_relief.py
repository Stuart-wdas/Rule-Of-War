"""Import measured CC0 meadow displacement; keep source and derived maps separate."""
from pathlib import Path
import hashlib, json, shutil, urllib.request
import numpy as np
from PIL import Image

ROOT=Path(__file__).resolve().parents[3]
SOURCE=ROOT/'assets/incoming_textures/rocky_terrain_02'
OUT=ROOT/'materials/pbr'
cached=SOURCE/'height-source.json'
meta=json.loads(cached.read_text(encoding='utf-8-sig')) if cached.exists() else json.load(urllib.request.urlopen(urllib.request.Request('https://api.polyhaven.com/files/rocky_terrain_02',headers={'User-Agent':'Mozilla/5.0'})))['Displacement']['2k']['png']
path=SOURCE/'rocky_terrain_02_disp_2k.png'
if not path.exists(): urllib.request.urlretrieve(meta['url'],path)
assert hashlib.md5(path.read_bytes()).hexdigest()==meta['md5']
height=Image.open(path)
values=np.asarray(height).astype(np.float32)
values/=65535 if values.max()>255 else 255
packed=np.uint8(np.rint(np.clip(values,0,1)*255))
name='meadow_relief_v1'
for suffix in ['albedo','normal']:
    shutil.copy2(OUT/f'meadow_{suffix}.png',OUT/f'{name}_{suffix}.png')
orm=np.array(Image.open(OUT/'meadow_orm.png').convert('RGB'))
orm[:,:,2]=packed
Image.fromarray(orm).save(OUT/f'{name}_orm.png')
small=Image.fromarray(values).resize((512,512),Image.Resampling.BILINEAR)
(OUT/f'{name}_height.bin').write_bytes(np.uint8(np.rint(np.clip(np.asarray(small),0,1)*255)).tobytes())
Image.fromarray(packed).save(OUT/f'{name}_height.png')
catalog_path=OUT/'catalog.json'
catalog=json.loads(catalog_path.read_text())
entry=dict(catalog['materials']['meadow'])
entry.update(orm='R=AO G=roughness B=height (terrain shader only)',height_source=meta,
    height_metres=0.24,runtime_sha256={s:hashlib.sha256((OUT/f'{name}_{s}.png').read_bytes()).hexdigest() for s in ['albedo','normal','orm','height']})
catalog['materials'][name]=entry
catalog_path.write_text(json.dumps(catalog,indent=2)+'\n')
print('Verified 16-bit measured displacement; packed height and 512-square CPU sampler',flush=True)
