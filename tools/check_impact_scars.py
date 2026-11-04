"""Matched native render checks of persistent projectile ground damage."""
import argparse
import hashlib
import json
import re
import shutil
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont
import check_world_detail as qa
from check_cloud_foliage import world_visible

ROOT = Path(__file__).resolve().parents[1]
qa.OUT = ROOT / '.data/impact-scars'
OUT = ROOT / 'assets/art_refresh/impact-scars'
CASES = {
    'flat': ['--graphics', 'high', '--cloud-time', '0'],
    'slope': ['--scar-slope', '--graphics', 'high', '--cloud-time', '0'],
    'cloud': ['--graphics', 'high', '--cloud-time', '120'],
    'low': ['--graphics', 'low', '--cloud-time', '0'],
}


def publish_validation(results):
    names = ['after-' + case for case in CASES]
    if not all(name in results for name in names):
        return
    for name in names:
        report = results[name]['report']
        assert report['impactScarMeshCount'] == 5
        assert len(report['impactScars']) == 5
    keys = ['cameraCenter', 'cameraZoom', 'cameraFovDegrees',
            'cameraHeadingDegrees', 'cameraElevationDegrees']
    for key in keys:
        assert results['after-flat']['report'][key] == results['after-cloud']['report'][key]
        if 'before-flat' in results:
            assert results['before-flat']['report'][key] == results['after-flat']['report'][key]
    manifest = json.loads((OUT / 'bake.json').read_text())
    for entry in manifest['files']:
        actual = hashlib.sha256((ROOT / entry['path']).read_bytes()).hexdigest()
        assert actual == entry['sha256'], entry['path']
    flat = np.asarray(Image.open(qa.OUT / 'after-flat.png').convert('RGB')).astype('float32')
    cloud = np.asarray(Image.open(qa.OUT / 'after-cloud.png').convert('RGB')).astype('float32')
    region = np.s_[300:755, 600:1280]
    changed = np.abs(flat[region] - cloud[region]).max(2) > 3
    assert changed.sum() > 1000, 'Cloud shade did not reach the reviewed ground'
    proof = dict(
        nativeScenarios=list(CASES),
        marksPerScenario=5,
        weaponSizesMetres=[1.65, 3.25, 3.9],
        matchingCameras=True,
        verifiedPbrHashes=manifest['files'],
        changedPixelsUnderCloudShade=int(changed.sum()),
        captures={name: hashlib.sha256((qa.OUT / (name + '.png')).read_bytes()).hexdigest()
                  for name in names},
    )
    log = qa.OUT / 'tests.log'
    if log.exists():
        matches = re.findall(r'test result: (\w+)\. (\d+) passed; (\d+) failed', log.read_text())
        if matches:
            status, passed, failed = matches[-1]
            proof['unitTests'] = dict(status=status, passed=int(passed), failed=int(failed))
    fmt = qa.OUT / 'fmt.log'
    proof['formatCheckPassed'] = fmt.exists() and not fmt.read_text().strip()
    (OUT / 'validation.json').write_text(json.dumps(proof, indent=2) + '\n')
    print('PUBLISHED IMPACT VALIDATION', json.dumps(proof), flush=True)

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--baseline', action='store_true')
    parser.add_argument('--only', nargs='+')
    args = parser.parse_args()
    qa.OUT.mkdir(parents=True, exist_ok=True)
    OUT.mkdir(parents=True, exist_ok=True)
    stage = 'before' if args.baseline else 'after'
    executable = qa.OUT / (stage + '.exe')
    shutil.copy2(qa.CURRENT, executable)
    record = qa.OUT / 'verification.json'
    results = json.loads(record.read_text()) if record.exists() else {}
    for case, flags in CASES.items():
        if args.only and case not in args.only:
            continue
        name = stage + '-' + case
        flags = ['--impact-scar-check', '--hide-ui', '--vfx-quality', 'low', *flags]
        result = qa.run(name, executable, flags)
        path = qa.OUT / (name + '.png')
        if not world_visible(path):
            result = qa.run(name, executable, flags)
        assert world_visible(path), 'Opaque world surfaces did not render'
        if not args.baseline:
            scars = result['report']['impactScars']
            assert len(scars) == 5, scars
            assert len({s['radius'] for s in scars}) == 3, scars
            assert sum(s['excavated'] for s in scars) == 4, scars
            assert result['report']['impactScarMeshCount'] == 5
        results[name] = result
        record.write_text(json.dumps(results, indent=2))
        Image.open(path).convert('RGB').save(OUT / (name + '.jpg'), quality=95)
        print('VERIFIED IMPACT SCARS', name, flush=True)
    font = ImageFont.truetype('C:/Windows/Fonts/arial.ttf', 23)
    board = Image.new('RGB', (1920, 1160), (24, 29, 25))
    draw = ImageDraw.Draw(board)
    for i, (name, title) in enumerate([
        ('before-flat', 'BEFORE / REPEATED ATLAS AND ROUND DIRT LUMPS'),
        ('after-flat', 'AFTER / VARIABLE SOIL SCARS AND BROKEN EJECTA'),
        ('after-slope', 'SLOPING GROUND / OVERLAPPING ARTILLERY HITS'),
        ('after-cloud', 'SAME MARKS UNDER MOVING CLOUD SHADE'),
    ]):
        path = qa.OUT / (name + '.png')
        if path.exists():
            x, y = i % 2 * 960, i // 2 * 580
            # Identical, closer crops make the ground-contact result reviewable.
            crop = Image.open(path).convert('RGB').crop((600, 300, 1280, 755))
            board.paste(crop.resize((960, 540), Image.Resampling.LANCZOS), (x, y + 40))
            draw.text((x + 12, y + 8), title, font=font, fill='white')
    board.save(OUT / 'review.jpg', quality=95)
    if not args.baseline:
        publish_validation(results)
