"""Matched scenery detail captures and the existing camera stress route."""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import sys

import check_world_detail as qa

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.data/render-checks/lod-distance'

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--stage', choices=['before', 'after'], required=True)
    parser.add_argument('--stress', action='store_true')
    parser.add_argument('--stress-only', action='store_true')
    args = parser.parse_args()
    qa.OUT = OUT
    exe = OUT / f'{args.stage}-bin/rule-of-war.exe'
    sha = hashlib.sha256(exe.read_bytes()).hexdigest()
    if args.stage == 'after':
        assert sha == hashlib.sha256((ROOT / 'native/target/debug/rule-of-war.exe').read_bytes()).hexdigest()
    path = OUT / 'verification.json'
    results = json.loads(path.read_text()) if path.exists() else {}
    cases = {'ground-high': ['--organic-map', '--graphics', 'high']}
    if args.stage == 'after':
        cases['training-balanced'] = ['--training', '--training-tools', '--graphics', 'balanced']
        cases['training-low'] = ['--training', '--training-tools', '--graphics', 'low']
    if args.stress_only:
        cases = {}
    for label, flags in cases.items():
        name = f'{args.stage}-{label}'
        row = qa.run(name, exe, flags)
        r = row['report']
        assert r['sceneryStreaming']['peakUploadBytesPerFrame'] <= 1024 * 1024
        assert r['sceneryStreaming']['inactiveCacheBytes'] <= 64 * 1024 * 1024
        row['exeSha256'] = sha
        results[name] = row
        path.write_text(json.dumps(results, indent=2))
        print('VERIFIED', name, flush=True)
    if args.stress or args.stress_only:
        assert args.stage == 'after'
        png = OUT / 'camera.png'
        for stale in [png, Path(str(png) + '.json')]:
            if stale.exists(): stale.unlink()
        subprocess.run([sys.executable, str(ROOT / '.data/battle-bench/run_benchmark.py'),
            '--exe', str(exe), '--name', 'lod-distance-camera', '--timeout', '240', '--',
            '--camera-stress', '--graphics', 'high', '--upscaler', 'quality',
            '--vfx-quality', 'high', '--profile', '--offscreen', '--smoke-test', str(png)],
            cwd=ROOT, check=True)
        run = json.loads((ROOT / '.data/battle-bench/lod-distance-camera.run.json').read_text())
        r = json.loads(Path(str(png) + '.json').read_text())
        errors = (ROOT / '.data/battle-bench/lod-distance-camera.stderr').read_text()
        assert run['status'] == 'completed' and run['exitCode'] == 0
        assert not any(text in errors for text in ['panicked', 'ERROR', 'ShaderError', 'Validation Error'])
        assert r['cameraStress']['complete'] and r['terrainReady'] and r['terrainError'] is None
        assert r['cameraStress']['wheelEvents'] > 100 and r['cameraStress']['dragFrames'] > 50
        assert r['sceneryStreaming']['peakUploadBytesPerFrame'] <= 1024 * 1024
        assert r['sceneryStreaming']['inactiveCacheBytes'] <= 64 * 1024 * 1024
        results['after-camera'] = {'run': run, 'report': r, 'exeSha256': sha}
        path.write_text(json.dumps(results, indent=2))
        print('VERIFIED camera', json.dumps(r['timings']['frame']), flush=True)

if __name__ == '__main__': main()
