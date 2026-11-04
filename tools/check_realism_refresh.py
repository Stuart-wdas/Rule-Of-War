"""Capture the shipping world and measure a battle before/after the art refresh.

Run from the repository root with the bundled Python runtime. Captures exercise
the real renderer; this deliberately does not replace gameplay/unit tests.
"""
from pathlib import Path
import json
import argparse
import subprocess
import sys
import statistics

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.data/render-checks/realism-refresh'
EXE = ROOT / 'native/target/debug/rule-of-war.exe'
BENCH = ROOT / '.data/battle-bench/run_benchmark.py'


def run(name, flags, exe=EXE):
    png = OUT / f'{name}.png'
    subprocess.run([
        sys.executable, str(BENCH), '--exe', str(exe),
        '--name', 'realism-' + name, '--timeout', '180', '--',
        *flags, '--vfx-quality', 'balanced', '--profile', '--compact',
        '--offscreen', '--smoke-test', str(png),
    ], cwd=ROOT, check=True)
    record = ROOT / '.data/battle-bench' / ('realism-' + name)
    result = json.loads(Path(str(record) + '.run.json').read_text())
    assert result['exitCode'] == 0 and result['status'] == 'completed', result
    errors = Path(str(record) + '.stderr').read_text(encoding='utf8')
    assert not any(x in errors for x in ['panicked', 'ERROR', 'ShaderError', 'Validation Error']), errors[-4000:]
    report = json.loads(Path(str(png) + '.json').read_text())
    assert report['terrainReady'] and report['terrainError'] is None, report
    assert report['cameraPinned'] and abs(report['cameraElevationDegrees'] - 50.1944) < .01
    return dict(name=name, capture=str(png), run=result, report=report)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--only', nargs='+', help='Run only these named cases.')
    selected = parser.parse_args().only
    OUT.mkdir(exist_ok=True, parents=True)
    results = []
    cases = [
        ('after-town', ['--environment-town']),
        ('after-ground', ['--organic-map', '--ground-close']),
        ('after-models', ['--models']),
        ('after-infantry', ['--infantry-models']),
        ('after-training', ['--training', '--training-tools']),
        ('after-blast', ['--visual-demo', '--demo-scale', '1', '--demo-effect', '2', '--demo-age', '0.55']),
        ('after-impact', ['--visual-demo', '--demo-effect', '2', '--demo-age', '0.70']),
    ]
    for name, flags in cases:
        if selected and name not in selected:
            continue
        results.append(run(name, flags))
        (OUT / ('verification-final.json' if selected else 'verification.json')).write_text(json.dumps(results, indent=2))
        print('VERIFIED', name, flush=True)
    before = OUT / 'before-rule-of-war.exe'
    for label, exe in [('before', before), ('after', EXE)]:
        if selected and label + '-battle' not in selected:
            continue
        results.append(run(label + '-battle', ['--mock-battle', 'combined', '--battle-seconds', '30'], exe))
        (OUT / ('verification-final.json' if selected else 'verification.json')).write_text(json.dumps(results, indent=2))
        print('VERIFIED', label + '-battle', flush=True)
    summarize(results)


def summarize(results):
    battles = {r['name']: r for r in results if r['name'].endswith('-battle')}
    if len(battles) == 2:
        summary = {}
        for name, result in battles.items():
            report = result['report']
            hardware = ROOT / '.data/battle-bench' / f'realism-{name}.hardware.jsonl'
            samples = [json.loads(line) for line in hardware.read_text().splitlines()[1:]]
            end = result['run']['wallSeconds']
            active = [s for s in samples if end - 30 < s['elapsed'] < end - 2]
            summary[name] = {
                'frame': report['timings']['frame'],
                'effects': report['timings']['effects.update'],
                'scene': report['timings']['scene.sync'],
                'gpuPassesMs': report['gpuPassesMs'],
                'medianCpuMachinePercent': statistics.median(s['cpuMachinePercent'] for s in active),
                'medianProcessGpu3DPercent': statistics.median(sum(v for k, v in (s['gpuEngines'] or {}).items() if k.endswith('engtype_3D')) for s in active),
                'peakWorkingMiB': max(s['workingMiB'] for s in active),
                'peakEffects': report['peakEffects'],
            }
        summary['measurement'] = 'Sequential 30-second combined-arms battles; dev executable; Balanced VFX; 1100 x 850 offscreen rendering. Includes scene changes and reinforcements.'
        (OUT / 'performance-comparison.json').write_text(json.dumps(summary, indent=2))
        print('PERFORMANCE', json.dumps({k: v['frame'] for k, v in summary.items() if k.endswith('-battle')}), flush=True)


if __name__ == '__main__':
    main()
