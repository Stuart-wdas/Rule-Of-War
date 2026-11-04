"""Sustained real-input camera/render stress with per-phase hitch comparisons."""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.data/render-checks/camera-deep'
PHASES = ['camera.full_pan','camera.scroll','camera.near_pan','camera.drag','camera.mixed','camera.settle']

def validate(name, stage, exe):
    report = json.loads((OUT / f'{stage}-{name}.png.json').read_text())
    run = json.loads((ROOT / '.data/battle-bench' / f'camera-deep-{stage}-{name}.run.json').read_text())
    stderr = (ROOT / '.data/battle-bench' / f'camera-deep-{stage}-{name}.stderr').read_text()
    assert run['exitCode'] == 0 and run['status'] == 'completed', run
    assert not any(t in stderr for t in ['panicked','ERROR','ShaderError','Validation Error']), stderr[-4000:]
    assert report['terrainReady'] and report['terrainError'] is None and report['cameraPinned']
    study = report['cameraStress']
    assert study['complete'] and study['cameraTravel'] > 800
    assert study['wheelEvents'] >= 100 and study['dragFrames'] > 50
    assert study['minZoom'] < 50 and study['maxZoom'] > 1000
    assert all(report['timings'][p]['calls'] > 20 for p in PHASES)
    if stage == 'after':
        assert report['renderPipeline']['uploadBudgetBytes'] == 8 * 1024 * 1024
        scenery = report['sceneryStreaming']
        assert scenery['peakUploadBytesPerFrame'] <= scenery['uploadBudgetBytes']
        assert scenery['maxPartVertices'] <= 8192
        assert scenery['inactiveCacheBytes'] <= scenery['inactiveCacheBudgetBytes']
        assert all(key in report['timings'] for key in ['render.extract','render.sync_wait','render.prepare_assets','render.queue','render.submit'])
    return {'report':report,'run':run,'exeSha256':hashlib.sha256(exe.read_bytes()).hexdigest()}

def compare(results):
    summary={}
    for name in ['native','dlss']:
        if not all(f'{stage}-{name}' in results for stage in ['before','after']):continue
        before,after=[results[f'{stage}-{name}']['report'] for stage in ['before','after']]
        for field in ['cols','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:
            assert abs(before[field]-after[field])<.001,(name,field)
        assert before['graphics']==after['graphics']
        rows={}
        for phase in PHASES+['frame']:
            b,a=[report['timings'][phase] for report in [before,after]]
            rows[phase]={
                'beforeP95Ms':b['p95Ms'],'afterP95Ms':a['p95Ms'],
                'beforeP99Ms':b['p99Ms'],'afterP99Ms':a['p99Ms'],
                'beforeWorstMs':b['maxMs'],'afterWorstMs':a['maxMs'],
                'beforeOver50Ms':b['over50Ms'],'afterOver50Ms':a['over50Ms'],
                'beforeOver100Ms':b['over100Ms'],'afterOver100Ms':a['over100Ms'],
                'beforeFrames':b['calls'],'afterFrames':a['calls'],
            }
        summary[name]={'phases':rows,'sceneryStreaming':after['sceneryStreaming'],
            'terrainStreaming':after['terrainStreaming'],'renderPipeline':after['renderPipeline']}
    (OUT/'summary.json').write_text(json.dumps(summary,indent=2))
    return summary

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--stage',choices=['before','after'],default='after')
    parser.add_argument('--only',nargs='+',default=['native','dlss'])
    parser.add_argument('--verify-only',action='store_true')
    args=parser.parse_args()
    OUT.mkdir(parents=True,exist_ok=True)
    exe=OUT/('baseline-bin' if args.stage=='before' else 'after-bin')/'rule-of-war.exe'
    if args.stage=='after':
        assert exe.read_bytes()==(ROOT/'native/target/debug/rule-of-war.exe').read_bytes()
    path=OUT/'verification.json'
    results=json.loads(path.read_text()) if path.exists() else {}
    for name in args.only:
        if not args.verify_only:
            png=OUT/f'{args.stage}-{name}.png'
            for stale in [png,Path(str(png)+'.json')]:
                if stale.exists():stale.unlink()
            subprocess.run([sys.executable,str(ROOT/'.data/battle-bench/run_benchmark.py'),
                '--exe',str(exe),'--name',f'camera-deep-{args.stage}-{name}','--timeout','240','--',
                '--camera-stress','--graphics','high','--upscaler','quality' if name=='dlss' else 'native',
                '--vfx-quality','high','--profile','--offscreen','--smoke-test',str(png)],cwd=ROOT,check=True)
        results[f'{args.stage}-{name}']=validate(name,args.stage,exe)
        path.write_text(json.dumps(results,indent=2))
        print('VERIFIED',args.stage,name,flush=True)
    print(json.dumps(compare(results),indent=2),flush=True)

if __name__=='__main__':main()
