"""Native GPU checks for the denser mushroom base, quality and ground contact."""
from pathlib import Path
import argparse,json
from PIL import Image,ImageDraw
import check_world_detail as qa
ROOT=Path(__file__).resolve().parents[1]
qa.OUT=ROOT/'.data/render-checks/mushroom-base'
CASES={
    'early':['--visual-demo','--demo-effect','2','--demo-age','.45','--graphics','balanced'],
    'tail':['--visual-demo','--demo-effect','2','--demo-age','1.15','--graphics','balanced'],
    'low':['--visual-demo','--demo-effect','2','--demo-age','.45','--graphics','low','--vfx-quality','low'],
    'high':['--visual-demo','--demo-effect','2','--demo-age','.45','--graphics','high','--vfx-quality','high'],
    'scale-large':['--visual-demo','--demo-effect','2','--demo-scale','3','--demo-age','.85','--graphics','balanced'],
    'vehicle':['--explosions','--tank-impact','--tank-kill','--graphics','balanced'],
}
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--only',nargs='+');args=p.parse_args()
    path=qa.OUT/'verification.json';results=json.loads(path.read_text()) if path.exists() else {}
    before=json.loads((qa.OUT/'before.json').read_text())
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        result=qa.run('after-'+name,qa.CURRENT,flags);r=result['report']
        assert r['blenderHdrVfx'] and ['mushroom',0] in r['observedBlenderStyles']
        if name!='vehicle':
            expected={'LOW':28,'BALANCED':48,'HIGH':64}[r['vfxQuality']]
            assert r['peakEffects']>=expected+3,(name,r['peakEffects'])
        if name in before:
            for key in ['cameraCenter','cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:
                assert before[name]['report'][key]==r[key],(name,key)
            assert r['peakEffects']>before[name]['report']['peakEffects']
        results[name]=result;path.write_text(json.dumps(results,indent=2))
        print('VERIFIED mushroom base',name,flush=True)
    for name in ['early','tail']:
        if name not in results:continue
        out=Image.new('RGB',(1600,680),(24,28,27));draw=ImageDraw.Draw(out)
        for i,version in enumerate(['before','after']):
            im=Image.open(qa.OUT/f'{version}-{name}.png').convert('RGB').crop((560,180,1360,820))
            out.paste(im,(i*800,40));draw.text((i*800+15,12),['BEFORE','DENSER BASE'][i],fill='white')
        out.save(ROOT/f'assets/art_refresh/mushroom-base-{name}-comparison.png')
