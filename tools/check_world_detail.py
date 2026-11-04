"""Matched 1080p native captures for terrain, tracks, smoke and vehicle review."""
from pathlib import Path
import argparse,json,subprocess,sys
from PIL import Image,ImageDraw

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'.data/render-checks/world-detail'
CURRENT=ROOT/'native/target/debug/rule-of-war.exe'
BEFORE=OUT/'before-rule-of-war.exe'
LIGHTING_BEFORE=OUT/'lighting-before-rule-of-war.exe'
CASES={
    'sun-before':(LIGHTING_BEFORE,['--training','--training-tools','--graphics','balanced']),
    'sun-after':(CURRENT,['--training','--training-tools','--graphics','balanced']),
    'ground-before':(BEFORE,['--organic-map','--ground-close','--graphics','high']),
    'ground-after':(CURRENT,['--organic-map','--ground-close','--graphics','high']),
    'training-after':(CURRENT,['--training','--training-tools','--graphics','balanced']),
    'tracks-after':(CURRENT,['--training','--training-tools','--track-study','--graphics','balanced']),
    'tracks-mask':(CURRENT,['--training','--training-tools','--track-study','--graphics','balanced','--terrain-debug','5']),
    'smoke-before':(BEFORE,['--rocket-trails','--graphics','balanced']),
    'smoke-after':(CURRENT,['--rocket-trails','--graphics','balanced']),
    'vehicle-review':(CURRENT,['--tank-model','--graphics','high']),
    'town-after':(CURRENT,['--environment-town','--graphics','balanced']),
    'low-after':(CURRENT,['--rocket-trails','--graphics','low']),
    'dlss-after':(CURRENT,['--training','--training-tools','--track-study','--graphics','high','--upscaler','quality']),
    'battle-after':(CURRENT,['--mock-battle','combined','--battle-seconds','30','--graphics','balanced']),
}
def run(name,exe,flags):
    png=OUT/f'{name}.png'
    # Never let the native capture's completion check mistake a previous PNG
    # for the current asynchronous GPU readback.
    for old in [png,Path(str(png)+'.json')]:
        if old.exists():old.unlink()
    subprocess.run([sys.executable,str(ROOT/'.data/battle-bench/run_benchmark.py'),'--exe',str(exe),'--name','detail-'+name,'--timeout','240','--',*flags,'--vfx-quality','balanced','--profile','--offscreen','--smoke-test',str(png)],cwd=ROOT,check=True)
    log=ROOT/'.data/battle-bench'/('detail-'+name)
    record=json.loads(Path(str(log)+'.run.json').read_text())
    assert record['exitCode']==0 and record['status']=='completed',record
    errors=Path(str(log)+'.stderr').read_text(encoding='utf8')
    assert not any(t in errors for t in ['panicked','ERROR','ShaderError','Validation Error']),errors[-5000:]
    report=json.loads(Path(str(png)+'.json').read_text())
    assert report['terrainReady'] and report['terrainError'] is None
    assert report['cameraPinned']
    assert Image.open(png).size==(1920,1080)
    if name in ['tracks-after','tracks-mask','dlss-after']:
        assert report['groundTrackStamps']>12 and report['trackMaskPixels']>50,report
    if name in ['smoke-after','low-after']:
        assert report['rocketTrails']>20,report
    if name=='tracks-mask':
        import numpy as np
        pixels=np.asarray(Image.open(png).convert('RGB').crop((0,145,1920,790))).astype('int16')
        white=(pixels.min(axis=2)>160)&((pixels.max(axis=2)-pixels.min(axis=2))<12)
        remaining=set(zip(*np.nonzero(white)))
        largest=0
        while remaining:
            stack=[remaining.pop()];count=0
            while stack:
                y,x=stack.pop();count+=1
                for q in [(y-1,x),(y+1,x),(y,x-1),(y,x+1)]:
                    if q in remaining:remaining.remove(q);stack.append(q)
            largest=max(largest,count)
        assert largest>120 and white.mean()<0.1, 'Track data was recorded but no bounded connected tread mark was drawn by the GPU'
        report['visibleTrackMaskPixels']=int(white.sum())
        report['largestTrackMaskComponent']=largest
    return {'run':record,'report':report}
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--only',nargs='+');args=p.parse_args()
    OUT.mkdir(parents=True,exist_ok=True)
    path=OUT/'verification.json'
    results=json.loads(path.read_text()) if path.exists() else {}
    for name,(exe,flags) in CASES.items():
        if args.only and name not in args.only:continue
        results[name]=run(name,exe,flags);path.write_text(json.dumps(results,indent=2))
        print('VERIFIED',name,flush=True)
    for kind in ['ground','smoke','sun']:
        if not all(kind+'-'+s in results for s in ['before','after']):continue
        a,b=[results[kind+'-'+s]['report'] for s in ['before','after']]
        for key in ['cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:
            assert abs(a[key]-b[key])<0.001,(kind,key,a[key],b[key])
        images=[Image.open(OUT/f'{kind}-{s}.png').convert('RGB') for s in ['before','after']]
        comparison=Image.new('RGB',(1920,580),(27,30,27))
        for i,im in enumerate(images):comparison.paste(im.resize((960,540),Image.Resampling.LANCZOS),(i*960,40))
        draw=ImageDraw.Draw(comparison);draw.text((16,12),'BEFORE',fill='white');draw.text((976,12),'AFTER',fill='white')
        comparison.save(ROOT/f'assets/art_refresh/world-detail-{kind}-comparison.png')
