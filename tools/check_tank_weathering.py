"""Matched native captures of worn armour, running gear and material quality paths."""
from pathlib import Path
import argparse, json
from PIL import Image, ImageDraw
import check_world_detail as qa

ROOT = Path(__file__).resolve().parents[1]
qa.OUT = ROOT / '.data/render-checks/tank-weathering'
CASES = {
    'tank': ['--tank-model', '--graphics', 'balanced'],
    'support': ['--vehicle-model', 'long', '--graphics', 'balanced'],
    'carrier': ['--vehicle-model', 'troopCarrier', '--graphics', 'balanced'],
    'low': ['--tank-model', '--graphics', 'low'],
    'dlss': ['--tank-model', '--graphics', 'high', '--upscaler', 'quality'],
}

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--before', action='store_true')
    parser.add_argument('--only', nargs='+')
    args = parser.parse_args()
    version = 'before' if args.before else 'after'
    path = qa.OUT / f'{version}-verification.json'
    results = json.loads(path.read_text()) if path.exists() else {}
    exe = qa.OUT / 'before-rule-of-war.exe' if args.before else qa.CURRENT
    for name, flags in CASES.items():
        if args.only and name not in args.only:
            continue
        result = qa.run(f'{version}-{name}', exe, flags)
        report = result['report']
        assert report['worldLighting']['modelContactCacheMatches'] > 0
        if name == 'dlss':
            assert report['dlssSupported'] and report['graphics']['upscaler'] == 'quality'
            assert any('dlss_super_resolution' in key for key in report['gpuPassesMs'])
        baseline = qa.OUT / f'before-{name}.png.json'
        if not args.before and baseline.exists():
            before = json.loads(baseline.read_text())
            for key in ['cameraCenter', 'cameraZoom', 'cameraFovDegrees',
                        'cameraHeadingDegrees', 'cameraElevationDegrees', 'worldLighting']:
                assert before[key] == report[key], (name, key)
        results[name] = result
        path.write_text(json.dumps(results, indent=2))
        print('VERIFIED', version, name, flush=True)
    if not args.before:
        for name in ['tank', 'support']:
            before = qa.OUT / f'before-{name}.png'
            after = qa.OUT / f'after-{name}.png'
            if not before.exists() or not after.exists():
                continue
            comparison = Image.new('RGB', (1600, 680), (24, 28, 27))
            draw = ImageDraw.Draw(comparison)
            for i, path in enumerate([before, after]):
                image = Image.open(path).convert('RGB').crop((560, 180, 1360, 820))
                comparison.paste(image, (i * 800, 40))
                draw.text((i * 800 + 16, 12), ['BEFORE', 'WEATHERED'][i], fill='white')
            comparison.save(ROOT / f'assets/art_refresh/weathered-{name}-comparison.png')
