"""Matched native captures of the opt-in ground materials, plus default regression."""
from pathlib import Path
import json
import shutil
import check_world_detail as qa

ROOT = Path(__file__).resolve().parents[1]
qa.OUT = ROOT / '.data/render-checks/ground-demo'
qa.OUT.mkdir(parents=True, exist_ok=True)
PUBLIC = ROOT / 'assets/art_refresh/ground-demo'

CASES = {
    'baseline-wide': ['--ground-demo', '--ground-baseline', '--graphics', 'high'],
    'photo-wide': ['--ground-demo', '--graphics', 'high'],
    'baseline-close': ['--ground-demo', '--ground-baseline', '--ground-close', '--graphics', 'high'],
    'photo-close': ['--ground-demo', '--ground-close', '--graphics', 'high'],
    'photo-low': ['--ground-demo', '--graphics', 'low'],
    'photo-dlss': ['--ground-demo', '--graphics', 'high', '--upscaler', 'quality'],
    'default-training': ['--training', '--training-tools', '--graphics', 'balanced'],
}

def main():
    import argparse
    p = argparse.ArgumentParser()
    p.add_argument('--only', nargs='+')
    args = p.parse_args()
    path = PUBLIC / 'validation.json'
    records = json.loads(path.read_text()) if path.exists() else {}
    for name, flags in CASES.items():
        if args.only and name not in args.only:
            continue
        result = qa.run('ground-demo-' + name, qa.CURRENT, flags)
        report = result['report']
        assert report['photographicGroundMaterials'] == ('--ground-baseline' not in flags)
        assert report['photographicGroundDemo'] == ('--ground-demo' in flags)
        assert report['vehicleDetailMaps'] == 16
        if '--ground-demo' in flags:
            assert report['hudHidden']
            assert not report['mainMenuVisible']
        records[name] = result
        path.write_text(json.dumps(records, indent=2))
        shutil.copyfile(qa.OUT / ('ground-demo-' + name + '.png'), PUBLIC / (name + '.png'))
        print('VERIFIED', name, flush=True)
    for distance in ['wide', 'close']:
        names = [f'{kind}-{distance}' for kind in ['baseline', 'photo']]
        if all(name in records for name in names):
            before, after = [records[n]['report'] for n in names]
            for key in ['cameraZoom', 'cameraFovDegrees', 'cameraHeadingDegrees', 'cameraElevationDegrees', 'cameraCenter', 'worldLighting']:
                assert before[key] == after[key], (distance, key, before[key], after[key])
    page = '''<!doctype html><meta charset="utf-8"><title>Rule Of War — Ground material study</title>
<style>body{background:#151a16;color:#e9e6da;font:16px system-ui;margin:28px;max-width:1800px}h1{font-size:26px}p{max-width:1000px;line-height:1.5}img{width:100%;display:block}figure{margin:0}figcaption{padding:12px;background:#252c25}.pair{display:grid;grid-template-columns:1fr 1fr;gap:12px;margin:22px 0}a{color:#b8d9a8}@media(max-width:900px){.pair{grid-template-columns:1fr}}</style>
<h1>Photographic ground material study</h1>
<p>Native 1920×1080 captures. Both versions share the same fixed camera, sunlight, scenery, vehicles and dynamic shadows. New meadow and shoulder albedos were made with OpenAI’s built-in image generation; companion generated heights supply approximate shallow normals and cavity shading. The terrain geometry and movement rules are unchanged.</p>
<p>Open <b>View Photoreal Ground Demo.cmd</b> in the project folder to explore. Wheel: zoom. WASD / arrow keys: pan. F10: show the normal HUD. Escape: menu. The comparison scene is paused. Use <b>View Current Ground Demo.cmd</b> for the previous materials. Ordinary game launches retain the current terrain.</p>'''
    page += '''<style>.compare{position:relative;aspect-ratio:16/9;overflow:hidden;margin:16px 0 28px}.compare img{position:absolute;inset:0}.slider{width:100%}.links{display:flex;gap:20px}</style>'''
    for distance in ['wide', 'close']:
        page += f'''<h2>{distance.title()} view</h2>
<p>Drag to compare the same scene: current terrain on the left, generated materials on the right.</p>
<input class="slider" type="range" min="0" max="100" value="50" aria-label="{distance} comparison" oninput="document.getElementById('before-{distance}').style.clipPath='inset(0 '+(100-this.value)+'% 0 0)'">
<div class="compare"><img src="photo-{distance}.png" alt="Generated material study"><img id="before-{distance}" src="baseline-{distance}.png" alt="Current terrain" style="clip-path:inset(0 50% 0 0)"></div>
<p class="links"><a href="baseline-{distance}.png">Open current terrain at full resolution</a><a href="photo-{distance}.png">Open generated material study at full resolution</a></p>'''
    page += '<p>The height maps are artistic approximations rather than measured PBR scans. The diffuse images retain small crevice shading. Large trees and rocks remain geometry; no vehicles or moving-object shadows are painted into the ground.</p><p><a href="generation.json">Exact generation prompts</a> · <a href="materials.json">Material settings</a> · <a href="validation.json">Render validation</a></p>'
    (PUBLIC / 'comparison.html').write_text(page, encoding='utf-8')

if __name__ == '__main__':
    main()
