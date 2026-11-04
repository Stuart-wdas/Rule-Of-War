"""Summarise shader, render and diagnostic regression evidence."""
from pathlib import Path
import hashlib,json
import numpy as np
from PIL import Image,ImageFilter

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'assets/art_refresh/terrain-swirl-fix'
CHECKS=ROOT/'.data/render-checks/terrain-swirl'
records=json.loads((OUT/'validation.json').read_text())
assert len(records)==8
tests=(CHECKS/'tests.log').read_text()
clippy=(CHECKS/'clippy.log').read_text()
assert '328 passed; 0 failed' in tests
assert 'Finished' in clippy and 'error:' not in clippy
def band(name):
    image=Image.open(OUT/f'{name}.png').convert('L').crop((650,580,1120,970)).filter(ImageFilter.MedianFilter(5))
    return float(np.std(np.asarray(image.filter(ImageFilter.GaussianBlur(3)),dtype=float)-np.asarray(image.filter(ImageFilter.GaussianBlur(16)),dtype=float)))
a,b=band('flat-before'),band('flat-after')
assert b<a*.75,(a,b)
measurement={'before':a,'after':b,'ratio':b/a,'roi':[650,580,1120,970],
    'scope':'Unlit constant-texture diagnostic including unchanged 3D props; not an overall image quality score'}
(OUT/'band-regression.json').write_text(json.dumps(measurement,indent=2)+'\n')
catalog=json.loads((ROOT/'materials/pbr/catalog.json').read_text())
preserved=[]
for name in ['earth_grass_olive_v2','earth_grass_stony_v2','meadow_relief_v1','photo_shoulder_v1','asphalt']:
    for suffix in ['albedo','normal','orm']:
        path=ROOT/f'materials/pbr/{name}_{suffix}.png'
        assert hashlib.sha256(path.read_bytes()).hexdigest()==catalog['materials'][name]['runtime_sha256'][suffix]
    preserved.append(name)
summary={'cause':'High-frequency warped procedural colour/blend noise on the soil coverage',
    'fixedInDefaultRenderer':True,'unitTests':{'passed':328,'failed':0},'strictClippy':True,
    'nativeChecks':list(records),'materialSetsPreserved':preserved,'bandRegression':measurement,
    'dlssActive':records['dlss']['report']['dlssSupported'] and 'render/dlss_super_resolution/elapsed_gpu' in records['dlss']['report']['gpuPassesMs']}
(OUT/'validation-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary,indent=2))
