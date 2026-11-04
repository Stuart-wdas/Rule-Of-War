"""Matched native before/after checks and flat-texture isolation proof."""
from pathlib import Path
import json,shutil,subprocess,sys
import numpy as np
from PIL import Image,ImageFilter
import check_world_detail as qa

ROOT=Path(__file__).resolve().parents[1]
BASE=ROOT/'.data/render-checks/terrain-swirl'
qa.OUT=BASE/'fixed'
qa.OUT.mkdir(parents=True,exist_ok=True)
PUBLIC=ROOT/'assets/art_refresh/terrain-swirl-fix'
PUBLIC.mkdir(parents=True,exist_ok=True)
EXE=BASE/'bin/rule-of-war.exe'
EXE.parent.mkdir(parents=True,exist_ok=True)
shutil.copyfile(qa.CURRENT,EXE)
for dll in qa.CURRENT.parent.glob('*.dll'):shutil.copyfile(dll,EXE.parent/dll.name)
common=['--organic-map','--hide-ui']
CASES={
    'before':common+['--graphics','high','--legacy-terrain-noise'],
    'after':common+['--graphics','high'],
    'colour-before':common+['--graphics','high','--terrain-debug','1','--legacy-terrain-noise'],
    'colour-after':common+['--graphics','high','--terrain-debug','1'],
    'low':common+['--graphics','low'],
    'dlss':common+['--graphics','high','--upscaler','quality'],
}
records={}
for name,flags in CASES.items():
    record=qa.run('swirl-fix-'+name,EXE,flags)
    assert record['report']['legacyTerrainBlendNoise']==('--legacy-terrain-noise' in flags)
    assert record['report']['photographicGroundMaterials']
    if name=='dlss' and record['report']['dlssSupported']:
        assert 'render/dlss_super_resolution/elapsed_gpu' in record['report']['gpuPassesMs']
    records[name]=record
    shutil.copyfile(qa.OUT/f'swirl-fix-{name}.png',PUBLIC/f'{name}.png')
    (PUBLIC/'validation.json').write_text(json.dumps(records,indent=2))
    print('VERIFIED',name,flush=True)
for legacy in [True,False]:
    subprocess.run([sys.executable,str(ROOT/'tools/probe_terrain_sampling.py'),
        '--only','flat-all','--exe',str(EXE),*(['--legacy'] if legacy else [])],check=True,cwd=ROOT)
    src=BASE/('flat-all-legacy.png' if legacy else 'flat-all.png')
    name='flat-before' if legacy else 'flat-after'
    records[name]={'report':json.loads(Path(str(src)+'.json').read_text())}
    shutil.copyfile(src,PUBLIC/f'{name}.png')
    (PUBLIC/'validation.json').write_text(json.dumps(records,indent=2))
for a,b in [('before','after'),('colour-before','colour-after'),('flat-before','flat-after')]:
    for key in ['cameraZoom','cameraCenter','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees','worldLighting']:
        assert records[a]['report'][key]==records[b]['report'][key],(a,b,key)
def band_variation(name):
    # Fixed open-field ROI: suppress tiny props, then measure the spatial band
    # occupied by the unwanted ripples rather than the broad soil patches.
    image=Image.open(PUBLIC/f'{name}.png').convert('L').crop((650,580,1120,970)).filter(ImageFilter.MedianFilter(5))
    return float(np.std(np.asarray(image.filter(ImageFilter.GaussianBlur(3)),dtype=float)
        -np.asarray(image.filter(ImageFilter.GaussianBlur(16)),dtype=float)))
before_band,after_band=band_variation('flat-before'),band_variation('flat-after')
assert after_band<before_band*.75,(before_band,after_band)
(PUBLIC/'band-regression.json').write_text(json.dumps({'before':before_band,'after':after_band,
    'ratio':after_band/before_band,'roi':[650,580,1120,970],
    'scope':'Unlit constant-texture diagnostic, including unchanged 3D props; not an overall image quality score'},indent=2))
page='''<!doctype html><meta charset="utf-8"><title>Rule Of War — terrain swirl correction</title>
<style>body{background:#181b16;color:#e9e5d8;font:16px system-ui;margin:28px;max-width:1800px}p{max-width:1000px;line-height:1.6}a{color:#c4daa7}.compare{position:relative;aspect-ratio:16/9}.compare img{position:absolute;inset:0;width:100%}.slider{width:100%;margin:15px 0}</style>
<h1>Terrain swirl correction</h1><p>The pale wavy bands survived unlit rendering with all ground textures filtered to constant colours. That isolated the cause to procedural colour/blend noise, rather than the meadow normal map. Open-ground soil blending now uses its broad terrain coverage without the high-frequency warped perturbation. Fine global colour modulation was removed; small boundary variation remains on road edges. The existing colour assets, meadow relief and physical sun are unchanged.</p>'''
for title,a,b in [('Normal game rendering','before','after'),('Colour only, terrain lighting bypassed','colour-before','colour-after'),('Proof: flat colours, no terrain lighting','flat-before','flat-after')]:
    page+=f'''<h2>{title}</h2><p>Drag to compare: before on the left, corrected on the right.</p><input class="slider" type="range" min="0" max="100" value="50" aria-label="{title}" oninput="document.getElementById('{a}').style.clipPath='inset(0 '+(100-this.value)+'% 0 0)'"><div class="compare"><img src="{b}.png"><img id="{a}" src="{a}.png" style="clip-path:inset(0 50% 0 0)"></div>'''
page+='<p><a href="validation.json">Native render checks</a></p>'
(PUBLIC/'comparison.html').write_text(page,encoding='utf-8')
