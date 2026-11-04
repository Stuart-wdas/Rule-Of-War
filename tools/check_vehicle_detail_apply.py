"""Check the approved generated albedos in the native renderer and legacy comparison path."""
from pathlib import Path
import argparse, json
import check_world_detail as qa

ROOT = Path(__file__).resolve().parents[1]
qa.OUT = ROOT / '.data/render-checks/vehicle-detail-apply'
CASES = {
    'tank': ['--tank-model', '--graphics', 'balanced', '--hide-ui'],
    'support': ['--vehicle-model', 'long', '--graphics', 'balanced'],
    'carrier': ['--vehicle-model', 'troopCarrier', '--graphics', 'balanced'],
    'apache': ['--aircraft-parking', '--apache-model', '--graphics', 'balanced'],
    'a10': ['--aircraft-parking', '--warthog-model', '--graphics', 'balanced'],
    'low': ['--tank-model', '--graphics', 'low'],
    'dlss': ['--tank-model', '--graphics', 'high', '--upscaler', 'quality'],
    'battlefield': ['--graphics', 'balanced'],
}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('--only', nargs='+')
    args = parser.parse_args(); path = qa.OUT / 'verification.json'
    results = json.loads(path.read_text()) if path.exists() else {}
    for name, flags in CASES.items():
        if args.only and name not in args.only:
            continue
        if name in ['tank', 'a10']:
            baseline = qa.run('legacy-' + name, qa.CURRENT, [*flags, '--legacy-vehicle-materials'])
            assert baseline['report']['vehicleDetailMaps'] == 0
            results['legacy-' + name] = baseline
        result = qa.run('applied-' + name, qa.CURRENT, flags)
        r = result['report']; assert r['vehicleDetailMaps'] == 16
        if name == 'tank':
            assert r['hudHidden'] and r['visibleTacticalUiRoots'] == 0
        if name in ['tank', 'support', 'carrier', 'low', 'dlss']:
            assert r['worldLighting']['modelContactCacheMatches'] > 0
        if name in ['apache', 'a10']:
            assert len(r['aircraft']) == 4
        if name == 'dlss':
            assert r['dlssSupported'] and r['graphics']['upscaler'] == 'quality'
            assert any('dlss_super_resolution' in key for key in r['gpuPassesMs'])
        if name in ['tank', 'a10']:
            before = results['legacy-' + name]['report']
            for key in ['cameraCenter', 'cameraZoom', 'cameraFovDegrees', 'cameraHeadingDegrees',
                        'cameraElevationDegrees', 'worldLighting']:
                assert before[key] == r[key], (name, key)
        results[name] = result
        path.write_text(json.dumps(results, indent=2))
        print('VERIFIED applied vehicle materials', name, flush=True)
