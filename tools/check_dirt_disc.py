"""Check the filled ground-level dirt burst independently of upright puffs."""
from pathlib import Path
import argparse,json
from PIL import Image,ImageDraw
import check_world_detail as qa
ROOT=Path(__file__).resolve().parents[1]
qa.OUT=ROOT/'.data/render-checks/dirt-disc'
CASES={
    'ignition':['--visual-demo','--demo-effect','2','--demo-age','.12','--graphics','balanced'],
    'spread':['--visual-demo','--demo-effect','2','--demo-age','.45','--graphics','balanced'],
    'faded':['--visual-demo','--demo-effect','2','--demo-age','1.2','--graphics','balanced'],
    'low':['--visual-demo','--demo-effect','2','--demo-age','.45','--graphics','low','--vfx-quality','low'],
    'high':['--visual-demo','--demo-effect','2','--demo-age','.45','--graphics','high','--vfx-quality','high'],
    'dlss':['--visual-demo','--demo-effect','2','--demo-age','.45','--graphics','high','--upscaler','quality'],
    'large':['--visual-demo','--demo-effect','2','--demo-scale','3','--demo-age','.45','--graphics','balanced'],
    'vehicle':['--explosions','--tank-impact','--tank-kill','--graphics','balanced'],
}
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--only',nargs='+');args=p.parse_args()
    path=qa.OUT/'verification.json';results=json.loads(path.read_text()) if path.exists() else {}
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        result=qa.run('disc-'+name,qa.CURRENT,flags);r=result['report']
        assert ['mushroom',0] in r['observedBlenderStyles'] and r['blenderHdrVfx']
        assert r['dirtDiscsSpawned']>=1 and r['dirtDiscsPeak']>=1
        assert r['dirtDiscMaximumRadius']>5
        if name=='dlss':
            assert r['dlssSupported'] and r['graphics']['upscaler']=='quality'
            assert any('dlss_super_resolution' in key for key in r['gpuPassesMs'])
        results[name]=result;path.write_text(json.dumps(results,indent=2))
        print('VERIFIED dirt disc',name,flush=True)
    if 'spread' in results:
        before=json.loads((qa.OUT/'before.png.json').read_text());after=results['spread']['report']
        for key in ['cameraCenter','cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:
            assert before[key]==after[key],key
        out=Image.new('RGB',(1600,700),(24,28,27));draw=ImageDraw.Draw(out)
        for i,path in enumerate([qa.OUT/'before.png',qa.OUT/'disc-spread.png']):
            im=Image.open(path).convert('RGB').crop((980,470,1380,800)).resize((800,660),Image.Resampling.LANCZOS)
            out.paste(im,(i*800,40));draw.text((i*800+16,12),['BEFORE','GROUND-LEVEL DIRT BURST'][i],fill='white')
        out.save(ROOT/'assets/art_refresh/dirt-disc-comparison.png')
