"""Native checks for the applied two-texture earthy terrain/foliage palette."""
from pathlib import Path
import argparse
import hashlib
import json
import shutil
import numpy as np
from PIL import Image
import check_world_detail as qa

ROOT = Path(__file__).resolve().parents[1]
PUBLIC = ROOT / 'assets/art_refresh/earth-grass-v2'
qa.OUT = ROOT / '.data/render-checks/earth-grass-v2'
qa.OUT.mkdir(parents=True, exist_ok=True)
CASES = {
    'before-wide': ['--ground-demo','--ground-baseline','--graphics','high'],
    'after-wide': ['--ground-demo','--graphics','high'],
    'before-close': ['--ground-demo','--ground-baseline','--ground-close','--graphics','high'],
    'after-close': ['--ground-demo','--ground-close','--graphics','high'],
    'variation-mask': ['--ground-demo','--terrain-debug','6','--graphics','balanced'],
    'low': ['--ground-demo','--graphics','low'],
    'dlss': ['--ground-demo','--graphics','high','--upscaler','quality'],
    'training': ['--training','--training-tools','--graphics','balanced'],
    'main-game': ['--graphics','balanced'],
    'tracks': ['--training','--training-tools','--track-study','--graphics','balanced'],
}

def main():
    p=argparse.ArgumentParser();p.add_argument('--only',nargs='+');args=p.parse_args()
    path=PUBLIC/'validation.json'
    records=json.loads(path.read_text()) if path.exists() else {}
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        record=qa.run('earth-grass-'+name,qa.CURRENT,flags)
        report=record['report']
        assert report['photographicGroundMaterials']==('--ground-baseline' not in flags)
        assert report['groundTextureLayers']==5
        assert report['vehicleDetailMaps']==16
        expected = [1/8,1/90,1/8] if '--ground-baseline' not in flags else None
        if expected:
            assert np.allclose(report['groundPhysicalRepeats'][0][:3],expected)
            assert abs(report['groundPhysicalRepeats'][1][0]-1/6)<.00001
        if name=='dlss' and report['dlssSupported']:
            assert 'render/dlss_super_resolution/elapsed_gpu' in report['gpuPassesMs']
        if name=='tracks':
            assert report['groundTrackStamps']>12 and report['trackMaskPixels']>50
        png=qa.OUT/f'earth-grass-{name}.png'
        if name=='variation-mask':
            pixels=np.asarray(Image.open(png).convert('RGB')).astype(np.int16)
            # Filmic display conversion mixes some blue into both debug colours.
            # Identify dominant channels rather than expecting raw zero-blue RGB.
            red=(pixels[:,:,0]>pixels[:,:,1]*1.7)&(pixels[:,:,0]>100)&(pixels[:,:,0]>pixels[:,:,2]*1.7)
            green=(pixels[:,:,1]>pixels[:,:,0]*1.7)&(pixels[:,:,1]>100)&(pixels[:,:,1]>pixels[:,:,2]*1.7)
            assert red.mean()>.04 and green.mean()>.04, (red.mean(),green.mean())
            report['visibleOliveDominantFraction']=float(red.mean())
            report['visibleStonyDominantFraction']=float(green.mean())
        records[name]=record
        path.write_text(json.dumps(records,indent=2))
        shutil.copyfile(png,PUBLIC/f'{name}.png')
        print('VERIFIED',name,flush=True)
    for distance in ['wide','close']:
        if all(f'{kind}-{distance}' in records for kind in ['before','after']):
            before,after=[records[f'{k}-{distance}']['report'] for k in ['before','after']]
            for key in ['cameraZoom','cameraCenter','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees','worldLighting']:
                assert before[key]==after[key],(distance,key,before[key],after[key])
    mats=json.loads((PUBLIC/'materials.json').read_text())
    for label,material in mats.items():
        for suffix,digest in material['runtime_sha256'].items():
            assert hashlib.sha256((ROOT/f'materials/pbr/{label}_{suffix}.png').read_bytes()).hexdigest()==digest
    page='''<!doctype html><meta charset="utf-8"><title>Rule Of War — Earthy grass variations</title>
<style>body{background:#171a14;color:#e7e2d1;font:16px system-ui;margin:28px;max-width:1800px}p{max-width:1050px;line-height:1.6}h1{font-size:28px}a{color:#bad49c}.compare{position:relative;aspect-ratio:16/9;overflow:hidden}.compare img{position:absolute;width:100%;inset:0}.slider{width:100%;margin:16px 0}.links{display:flex;gap:24px}.maps{display:grid;grid-template-columns:1fr 1fr;gap:18px}.maps img{width:100%}figure{margin:0}figcaption{padding:10px;background:#24291d}</style>
<h1>Earthy grass, stones and low scrub — applied to the game</h1>
<p>Two new overhead textures vary between denser olive meadow and drier stony scrub. Dark soil, straw, embedded stones and creeping plants add small-scale separation. The foliage palette is less bright and more muted. This is active in ordinary matches, the training ground and the demo.</p>
<p>Matched native 1920×1080 captures use the same camera and lighting. Baked short ground shadows follow the existing sun direction; texture variation uses translations, without rotation or mirroring. Larger objects and vehicles keep live shadows. Existing macro relief, paved surfaces, tracks and collision rules remain in place.</p>'''
    for distance in ['close','wide']:
        page+=f'''<h2>{distance.title()} comparison</h2><p>Drag: previous terrain on the left; applied earthy grass blend on the right.</p><input class="slider" type="range" min="0" max="100" value="50" aria-label="{distance} comparison" oninput="document.getElementById('old-{distance}').style.clipPath='inset(0 '+(100-this.value)+'% 0 0)'"><div class="compare"><img src="after-{distance}.png" alt="Applied grass variations"><img id="old-{distance}" src="before-{distance}.png" alt="Previous terrain" style="clip-path:inset(0 50% 0 0)"></div><p class="links"><a href="before-{distance}.png">Previous full-resolution view</a><a href="after-{distance}.png">Applied full-resolution view</a></p>'''
    page+='''<h2>Generated colour maps</h2><div class="maps"><figure><img src="../../../materials/pbr/earth_grass_olive_v2_albedo.png"><figcaption>Olive meadow — 8m repeat</figcaption></figure><figure><img src="../../../materials/pbr/earth_grass_stony_v2_albedo.png"><figcaption>Stony grass and creeping scrub — 6m repeat</figcaption></figure></div><p>Restart the game to load the new textures. The existing terrain demo launchers also show the latest blend and legacy comparison. Graphics quality remains adjustable. AI companion heights provide approximate shallow normals; they are not measured scans.</p><p><a href="generation.json">Exact generation prompts</a> · <a href="materials.json">Material settings</a> · <a href="validation.json">Native checks</a> · <a href="variation-mask.png">Live variation mask: olive red, stony green</a></p>'''
    (PUBLIC/'comparison.html').write_text(page,encoding='utf-8')

if __name__=='__main__':main()
