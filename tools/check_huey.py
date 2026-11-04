"""Capture the runtime Huey, its passenger HUD, both factions and low-quality geometry."""
from pathlib import Path
import argparse
import json
import shutil
import check_world_detail as qa

ROOT = Path(__file__).resolve().parents[1]
qa.OUT = ROOT / '.data/render-checks/huey'
CASES = {
    'huey-close': ['--huey-check', '--huey-close', '--hide-ui', '--graphics', 'high'],
    'huey-loaded': ['--huey-check', '--graphics', 'balanced'],
    'huey-unloaded': ['--huey-check', '--unloaded', '--graphics', 'balanced'],
    'huey-flight': ['--huey-check', '--huey-flight', '--hide-ui', '--graphics', 'high'],
    'huey-enemy': ['--huey-check', '--huey-close', '--model-enemy', '--hide-ui', '--graphics', 'balanced'],
    'huey-low': ['--huey-check', '--huey-close', '--hide-ui', '--graphics', 'low'],
}

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--only', nargs='+')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.output:
        qa.OUT = args.output.resolve()
    qa.OUT.mkdir(parents=True, exist_ok=True)
    exe = qa.OUT / 'bin/rule-of-war.exe'
    exe.parent.mkdir(exist_ok=True)
    shutil.copy2(qa.CURRENT, exe)
    for name in ['nvngx_dlss.dll']:
        src = qa.CURRENT.parent / name
        if src.exists():
            shutil.copy2(src, exe.parent / name)
    record_path = qa.OUT / 'verification.json'
    records = json.loads(record_path.read_text()) if record_path.exists() else {}
    for name, flags in CASES.items():
        if args.only and name not in args.only:
            continue
        result = qa.run(name, exe, flags)
        r = result['report']
        assert r['cameraZoom'] < 40
        if name != 'huey-enemy':
            assert r['selectedFriendlyUnits'] == 1
        h = next(c for c in r['carriers'] if c['kind'] == 'huey')
        assert h['capacity'] == 12
        assert h['passengers'] == (0 if name == 'huey-unloaded' else 12)
        a = next(c for c in r['aircraft'] if c['kind'] == 'huey')
        assert a['fuel'] > 0
        if name == 'huey-flight':
            assert a['altitude'] > 5 and a['rotorPower'] > 0.9
        records[name] = result
        (qa.OUT / 'verification.json').write_text(json.dumps(records, indent=2))
        print('VERIFIED', name, flush=True)
