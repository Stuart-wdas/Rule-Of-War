"""Review metal/paint highlights with the same camera and daylight probe."""
from pathlib import Path
import argparse,json
from PIL import Image,ImageDraw,ImageOps,ImageFont
import check_world_detail as qa
ROOT=Path(__file__).resolve().parents[1]
qa.OUT=ROOT/'.data/render-checks/metal-sheen'
CASES={
    'tank':['--tank-model','--graphics','balanced'],
    'carrier':['--vehicle-model','troopCarrier','--graphics','balanced'],
    'anti-air':['--vehicle-model','antiAir','--graphics','balanced'],
    'apache':['--aircraft-parking','--apache-model','--graphics','balanced'],
    'a10':['--aircraft-parking','--warthog-model','--graphics','balanced'],
    'low':['--tank-model','--graphics','low'],
    'dlss':['--tank-model','--graphics','high','--upscaler','quality'],
}
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--only',nargs='+');args=p.parse_args()
    path=qa.OUT/'verification.json';results=json.loads(path.read_text()) if path.exists() else {}
    before=json.loads((qa.OUT/'before.json').read_text())
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        result=qa.run('after-'+name,qa.CURRENT,flags);r=result['report']
        assert r['worldLighting']['environmentProbeIntensity']==280
        if name in ['tank','carrier','anti-air','low','dlss']:assert r['worldLighting']['modelContactCacheMatches']>0
        if name in ['apache','a10']:assert len(r['aircraft'])==4
        if name=='dlss':
            assert r['graphics']['upscaler']=='quality' and r['dlssSupported']
            assert any('dlss_super_resolution' in key for key in r['gpuPassesMs'])
        if name in before:
            for key in ['cameraCenter','cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:
                assert before[name]['report'][key]==r[key],(name,key)
        results[name]=result;path.write_text(json.dumps(results,indent=2));print('VERIFIED metal sheen',name,flush=True)
    for name in before:
        if name not in results:continue
        out=Image.new('RGB',(1600,680),(24,28,27));draw=ImageDraw.Draw(out)
        for i,version in enumerate(['before','after']):
            im=Image.open(qa.OUT/f'{version}-{name}.png').convert('RGB').crop((560,180,1360,820))
            out.paste(im,(i*800,40));draw.text((i*800+16,12),['BEFORE','METAL / SATIN PAINT'][i],fill='white')
        out.save(ROOT/f'assets/art_refresh/metal-sheen-{name}-comparison.png')
