"""Save the delivered terrain provenance and verification summary."""
from pathlib import Path
import hashlib
import json

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'assets/art_refresh/earth-grass-v2'
CHECKS=ROOT/'.data/render-checks/earth-grass-v2'

generation=json.loads((OUT/'generation.json').read_text())
generation['colourCalibration']=json.loads((OUT/'colour-calibration.json').read_text())
generation['activeAlbedos']={n:f'materials/pbr/earth_grass_v2/{n}.png' for n in ['olive','stony']}
(OUT/'generation.json').write_text(json.dumps(generation,indent=2)+'\n')

tests=(CHECKS/'tests.log').read_text()
clippy=(CHECKS/'clippy.log').read_text()
records=json.loads((OUT/'validation.json').read_text())
assert '328 passed; 0 failed' in tests
assert 'Finished' in clippy and 'error:' not in clippy
assert len(records)==10
materials=json.loads((OUT/'materials.json').read_text())
for label,mat in materials.items():
    for suffix,digest in mat['runtime_sha256'].items():
        assert hashlib.sha256((ROOT/f'materials/pbr/{label}_{suffix}.png').read_bytes()).hexdigest()==digest
catalog=json.loads((ROOT/'materials/pbr/catalog.json').read_text())
macro=ROOT/'materials/pbr/meadow_relief_v1_normal.png'
assert hashlib.sha256(macro.read_bytes()).hexdigest()==catalog['materials']['meadow_relief_v1']['runtime_sha256']['normal']
summary={'appliedTo':['main game','training ground','ground demonstration'],
    'unitTests':{'passed':328,'failed':0},'strictClippy':True,
    'nativeRenderChecks':list(records),
    'meadowMacroNormalPreserved':True,
    'variationCoverage':{key:records['variation-mask']['report'][key] for key in ['visibleOliveDominantFraction','visibleStonyDominantFraction']},
    'dlssQualityActive':records['dlss']['report']['dlssSupported'] and 'render/dlss_super_resolution/elapsed_gpu' in records['dlss']['report']['gpuPassesMs']}
(OUT/'validation-summary.json').write_text(json.dumps(summary,indent=2)+'\n')

page=OUT/'comparison.html'
html=page.read_text(encoding='utf-8')
if 'Colour calibration prompts</a>' not in html:
    html=html.replace('Exact generation prompts</a>', 'Exact generation prompts</a> · <a href="colour-calibration.json">Colour calibration prompts</a>')
page.write_text(html,encoding='utf-8')

old=ROOT/'assets/art_refresh/ground-demo/comparison.html'
html=old.read_text(encoding='utf-8')
if 'earth-grass-v2/comparison.html' not in html:
    html=html.replace('<h1>', '<p><b>Earlier study.</b> The latest terrain variations are applied to the main game and training ground. <a href="../earth-grass-v2/comparison.html">Open the latest comparison.</a></p><h1>',1)
old.write_text(html,encoding='utf-8')
readme=ROOT/'assets/art_refresh/ground-demo/README.txt'
text=readme.read_text()
if 'LATEST APPLIED MATERIALS' not in text:
    readme.write_text('LATEST APPLIED MATERIALS: ../earth-grass-v2/README.txt\nThe newer two-texture blend is active in ordinary matches and training.\nThis file describes the earlier opt-in study.\n\n'+text)
print(json.dumps(summary,indent=2))
