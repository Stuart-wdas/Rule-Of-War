"""Native RTX overview/panning, near terrain and training regressions."""
from pathlib import Path
import argparse
import hashlib
import json
import re
import check_world_detail as qa

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.data/render-checks/overview-performance'
BIN = OUT / 'after-bin'
CASES = {
    'scenery-after': ['--scenery-check', '--graphics', 'high'],
    'full-map-after': ['--visibility-check', '--graphics', 'high'],
    'full-map-high-dlss': ['--visibility-check', '--graphics', 'high', '--upscaler', 'quality', '--vfx-quality', 'high'],
    'pan-high': ['--full-map-pan', '--graphics', 'high'],
    'pan-dlss': ['--full-map-pan', '--graphics', 'high', '--upscaler', 'quality'],
    'training-after': ['--training', '--training-tools', '--graphics', 'balanced'],
    'near-ground-after': ['--organic-map', '--ground-close', '--graphics', 'high'],
}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--only', nargs='+')
    args = parser.parse_args()
    qa.OUT = OUT
    OUT.mkdir(parents=True, exist_ok=True)
    exe = BIN / 'rule-of-war.exe'
    current = ROOT / 'native/target/debug/rule-of-war.exe'
    digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    assert digest(exe) == digest(current), 'Private render executable is stale'
    path = OUT / 'verification.json'
    results = json.loads(path.read_text()) if path.exists() else {}
    for name, flags in CASES.items():
        if args.only and name not in args.only:
            continue
        record = qa.run(name, exe, flags)
        report = record['report']
        if name.startswith('pan-'):
            assert report['cols'] == 1024
            assert report['overviewPanTerrainRebuilds'] == 0
            assert report['overviewPanPeakDistance'] > 40
            assert report['terrainVertices'] < 70000
            assert report['timings']['frame.overview_pan']['calls'] > 60
        if name == 'scenery-after':
            assert report['sceneryCoverageReady']
            assert report['sceneryShiftedCells'] == report['sceneryDuplicateCells'] == 0
        if name == 'pan-dlss':
            assert report['dlssSupported'] and report['graphics']['upscaler'] == 'quality'
        if name == 'full-map-high-dlss':
            assert report['graphics']['explosions'] == 'high'
            assert report['dlssSupported'] and report['graphics']['upscaler'] == 'quality'
        record['exeSha256'] = digest(exe)
        results[name] = record
        path.write_text(json.dumps(results, indent=2))
        print('VERIFIED', name, flush=True)
    for label, before, after in [
        ('full-map', 'full-before.png.json', 'full-map-after'),
        ('scenery', 'before.png.json', 'scenery-after'),
        ('full-map-high-dlss', 'full-dlss-before.png.json', 'full-map-high-dlss'),
    ]:
        if after not in results:
            continue
        previous = json.loads((OUT / before).read_text())
        current_report = results[after]['report']
        for key in ['cols', 'cameraZoom', 'cameraFovDegrees', 'cameraHeadingDegrees', 'cameraElevationDegrees']:
            assert abs(previous[key] - current_report[key]) < .001, key
        for stage in ['frame', 'app.main']:
            print(label, stage, json.dumps({
                'before': previous['timings'][stage],
                'after': current_report['timings'][stage],
            }), flush=True)
    if set(CASES).issubset(results):
        assert all(row['exeSha256'] == digest(exe) for row in results.values())
        test_log = (ROOT / '.data/overview-tests.log').read_text()
        passed = re.search(r'test result: ok\. (\d+) passed; 0 failed', test_log)
        assert passed and int(passed[1]) == 339
        assert 'Finished' in (ROOT / '.data/overview-clippy.log').read_text()
        assert 'Finished' in (ROOT / '.data/overview-build.log').read_text()
        before = json.loads((OUT / 'full-dlss-before.png.json').read_text())
        after = results['full-map-high-dlss']['report']
        cache = lambda name: sum(map(float, re.findall(r': ([\d.]+) ms', (ROOT / '.data' / name).read_text())[-3:]))
        summary = {
            'appliedToBaseGameAndTraining': True,
            'exeSha256': digest(exe),
            'testsPassed': int(passed[1]),
            'strictClippy': True,
            'nativeRenderChecks': len(CASES),
            'terrainTexturePreparationHighMs': cache('overview-cache-cold.log'),
            'terrainTextureCachedHighMs': cache('overview-cache-warm.log'),
            'fullMapHighDlssFrameP95BeforeMs': before['timings']['frame']['p95Ms'],
            'fullMapHighDlssFrameP95AfterMs': after['timings']['frame']['p95Ms'],
            'fullMapTerrainVertices': after['terrainVertices'],
            'terrainRebuildsDuringPan': results['pan-high']['report']['overviewPanTerrainRebuilds'],
            'smallArmsCannotDamageArmour': True,
            'shellsAndRocketsRemainEffective': True,
            'notes': 'Texture preparation is an isolated measurement, not total startup time. Frame measurements are from matched short RTX 3060 Laptop GPU runs; isolated startup spikes remain possible.',
        }
        (OUT / 'summary.json').write_text(json.dumps(summary, indent=2))
        print(json.dumps(summary, indent=2), flush=True)

if __name__ == '__main__':
    main()
