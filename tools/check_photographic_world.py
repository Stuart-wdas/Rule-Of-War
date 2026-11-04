"""Sequential native-render captures and GPU/CPU benchmarks for graphics presets."""
from pathlib import Path
import argparse, json, statistics, subprocess, sys

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'.data/render-checks/photographic-world'
EXE=ROOT/'native/target/debug/rule-of-war.exe'
CASES={
    'training-native':['--training','--training-tools','--graphics','balanced'],
    'training-dlss':['--training','--training-tools','--graphics','high','--upscaler','quality'],
    'town':['--environment-town','--graphics','balanced'],
    'settings':['--graphics-menu'],
    'settings-cycle':['--graphics-menu','--graphics-cycle'],
    'ground-close':['--organic-map','--ground-close','--graphics','high'],
    'armour-sparks':['--explosions','--tank-impact','--blast-early'],
    'mushroom-ring':['--visual-demo','--demo-effect','2','--demo-age','0.3'],
    'mushroom-faded-ring':['--visual-demo','--demo-effect','2','--demo-age','1.3'],
    'dlss-blast':['--visual-demo','--demo-effect','2','--demo-age','0.5','--upscaler','quality'],
    'before-battle':['--mock-battle','combined','--battle-seconds','30'],
    'low-battle':['--mock-battle','combined','--battle-seconds','30','--graphics','low'],
    'balanced-battle':['--mock-battle','combined','--battle-seconds','30','--graphics','balanced'],
    'dlss-battle':['--mock-battle','combined','--battle-seconds','30','--graphics','balanced','--upscaler','quality'],
    'native-1080-battle':['--mock-battle','combined','--battle-seconds','30','--graphics','balanced'],
    'dlss-1080-battle':['--mock-battle','combined','--battle-seconds','30','--graphics','balanced','--upscaler','quality'],
}
def run(name, flags):
    png=OUT/(name+'.png')
    exe=OUT/'before-rule-of-war.exe' if name=='before-battle' else EXE
    subprocess.run([sys.executable,str(ROOT/'.data/battle-bench/run_benchmark.py'),'--exe',str(exe),
        '--name','photo-'+name,'--timeout','240','--',*flags,'--vfx-quality','balanced',
        '--profile',*([] if '1080' in name else ['--compact']),'--offscreen','--smoke-test',str(png)],cwd=ROOT,check=True)
    stem=ROOT/'.data/battle-bench'/('photo-'+name)
    record=json.loads(Path(str(stem)+'.run.json').read_text())
    assert record['exitCode']==0 and record['status']=='completed',record
    errors=Path(str(stem)+'.stderr').read_text(encoding='utf8')
    assert not any(s in errors for s in ['panicked','ERROR','ShaderError','Validation Error']),errors[-6000:]
    report=json.loads(Path(str(png)+'.json').read_text())
    assert report['terrainReady'] and report['terrainError'] is None
    assert report['cameraPinned']
    result={'run':record,'report':report}
    if name.endswith('-battle'):
        rows=[json.loads(s) for s in Path(str(stem)+'.hardware.jsonl').read_text().splitlines()[1:]]
        active=[r for r in rows if record['wallSeconds']-28 < r['elapsed'] < record['wallSeconds']-2]
        result['hardware']={
            'medianCpuMachinePercent':statistics.median(r['cpuMachinePercent'] for r in active),
            'medianGpu3DPercent':statistics.median(sum(v for k,v in (r['gpuEngines'] or {}).items() if k.endswith('engtype_3D')) for r in active),
            'peakWorkingMiB':max(r['workingMiB'] for r in active),
        }
    return result

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--only',nargs='+');args=parser.parse_args()
    OUT.mkdir(parents=True,exist_ok=True)
    path=OUT/'verification.json'
    results=json.loads(path.read_text()) if path.exists() else {}
    for name,flags in CASES.items():
        if args.only and name not in args.only: continue
        results[name]=run(name,flags)
        path.write_text(json.dumps(results,indent=2))
        print('VERIFIED',name,'DLSS:',results[name]['report'].get('dlssSupported'),flush=True)
