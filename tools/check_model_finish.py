"""GPU checks for shared daylight reflections and baked vehicle contacts."""
from pathlib import Path
import argparse
import json
from PIL import Image, ImageDraw, ImageFont, ImageOps
import check_world_detail as qa

ROOT = Path(__file__).resolve().parents[1]
qa.OUT = ROOT / '.data/render-checks/model-finish'
CASES = {
    'tank': ['--tank-model'],
    'support': ['--vehicle-model', 'long'],
    'carrier': ['--vehicle-model', 'troopCarrier'],
    'rocket': ['--vehicle-model', 'xtralong'],
    'anti-air': ['--vehicle-model', 'antiAir'],
    'infantry': ['--infantry-models'],
    'apache': ['--aircraft-parking', '--apache-model'],
    'a10': ['--aircraft-parking', '--warthog-model'],
    'town': ['--environment-town'],
    'low': ['--models', '--graphics', 'low'],
    'dlss': ['--tank-model', '--graphics', 'high', '--upscaler', 'quality'],
    'battle': ['--mock-battle', 'combined', '--battle-seconds', '30'],
    'explosion': ['--explosions', '--tank-impact', '--tank-kill'],
}

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--only', nargs='+')
    args = parser.parse_args()
    qa.OUT.mkdir(parents=True, exist_ok=True)
    path = qa.OUT / 'verification.json'
    results = json.loads(path.read_text()) if path.exists() else {}
    for name, flags in CASES.items():
        if args.only and name not in args.only:
            continue
        if '--graphics' not in flags:
            flags = [*flags, '--graphics', 'balanced']
        result = qa.run('finish-after-' + name, qa.CURRENT, flags)
        report = result['report']
        assert report['worldLighting']['environmentProbeIntensity'] == 280
        if name in ['tank', 'support', 'carrier', 'rocket', 'anti-air', 'dlss']:
            assert report['worldLighting']['modelContactCacheMatches'] > 0
        if name == 'dlss':
            assert report['dlssSupported']
            assert report['graphics']['upscaler'] == 'quality'
            assert any('dlss_super_resolution' in key for key in report['gpuPassesMs'])
        if name == 'infantry':
            assert report['skinnedInfantry'] == 6
        if name in ['apache', 'a10']:
            assert len(report['aircraft']) == 4
            assert report['cameraZoom'] == (36 if name == 'apache' else 48)
        if name == 'explosion':
            assert report['blenderHdrVfx'] and report['observedBlenderStyles']
            assert report['wreckKinds'], 'Destruction fixture did not create a vehicle wreck'
        baseline = ROOT / '.data/render-checks/model-overhaul' / ('models-after-' + name + '.png.json')
        if baseline.exists() and name != 'dlss':
            before = json.loads(baseline.read_text())
            for key in ['cameraCenter', 'cameraZoom', 'cameraFovDegrees', 'cameraHeadingDegrees', 'cameraElevationDegrees']:
                assert before[key] == report[key], (name, key, before[key], report[key])
        results[name] = result
        path.write_text(json.dumps(results, indent=2))
        print('VERIFIED model finish', name, flush=True)
    items = [('tank', 'BATTLE TANK'), ('carrier', 'TROOP CARRIER'),
             ('infantry', 'INFANTRY'), ('apache', 'APACHE'),
             ('a10', 'A-10'), ('town', 'ARCHITECTURE')]
    if all(name in results for name, _ in items):
        gallery = Image.new('RGB', (1800, 916), (25, 29, 28))
        draw = ImageDraw.Draw(gallery)
        font = ImageFont.truetype('C:/Windows/Fonts/segoeui.ttf', 22)
        for i, (name, label) in enumerate(items):
            crop = (480, 145, 1440, 845)
            if name in ['apache', 'a10', 'town']:
                crop = (300, 110, 1620, 800)
            im = Image.open(qa.OUT / f'finish-after-{name}.png').convert('RGB').crop(crop)
            im = ImageOps.contain(im, (592, 412), Image.Resampling.LANCZOS)
            x, y = i % 3 * 600, i // 3 * 458
            draw.text((x + 14, y + 9), label, font=font, fill=(231, 222, 199))
            gallery.paste(im, (x + (600 - im.width) // 2, y + 42 + (412 - im.height) // 2))
        gallery.save(ROOT / 'assets/art_refresh/model-finish-gallery.png')
