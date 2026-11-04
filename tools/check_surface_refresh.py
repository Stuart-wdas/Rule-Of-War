"""Matched native-render checks for concrete and rough-terrain materials."""
from pathlib import Path
import argparse, json
from PIL import Image, ImageDraw, ImageOps
import check_world_detail as qa

ROOT=Path(__file__).resolve().parents[1]
qa.OUT=ROOT/'.data/render-checks/surface-refresh'
CASES={
    'apron-close':['--aircraft-parking','--apron-close','--graphics','high'],
    'apron':['--aircraft-parking','--warthog-model','--graphics','high'],
    'ground':['--organic-map','--ground-close','--graphics','high'],
    'plains':['--models','--graphics','high'],
    'town':['--environment-town','--graphics','balanced'],
    'low':['--aircraft-parking','--apron-close','--graphics','low'],
    'dlss':['--aircraft-parking','--apron-close','--graphics','high','--upscaler','quality'],
    'settings-cycle':['--aircraft-parking','--apron-close','--graphics','balanced','--graphics-cycle'],
    'training':['--training','--training-tools','--graphics','balanced'],
    'battle':['--mock-battle','combined','--battle-seconds','30','--graphics','balanced'],
}

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--only',nargs='+');args=parser.parse_args()
    qa.OUT.mkdir(parents=True,exist_ok=True)
    path=qa.OUT/'verification.json'
    results=json.loads(path.read_text()) if path.exists() else {}
    before=json.loads((qa.OUT/'before.json').read_text())
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        result=qa.run('surface-after-'+name,qa.CURRENT,flags)
        report=result['report']
        if name not in ['ground','plains']:assert report['concreteGroundBatches']>0,(name,'no ground concrete')
        expected={'low':512,'balanced':1024,'high':2048}[report['graphics']['textures']]
        assert report['concreteTexturePixels']==expected
        if name=='dlss':
            assert report['dlssSupported'] and report['graphics']['upscaler']=='quality'
            assert any('dlss_super_resolution' in k for k in report['gpuPassesMs'])
        if name in before:
            for key in ['cameraCenter','cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:
                assert report[key]==before[name]['report'][key],(name,key)
        results[name]=result;path.write_text(json.dumps(results,indent=2))
        print('VERIFIED surface refresh',name,flush=True)
    for name in ['apron','ground','town']:
        if name not in results:continue
        out=Image.new('RGB',(1920,580),(24,28,27));draw=ImageDraw.Draw(out)
        for i,version in enumerate(['before','after']):
            im=Image.open(qa.OUT/f'surface-{version}-{name}.png').convert('RGB')
            out.paste(im.resize((960,540),Image.Resampling.LANCZOS),(960*i,40))
            draw.text((960*i+16,12),version.upper()+': '+name.upper(),fill='white')
        out.save(ROOT/f'assets/art_refresh/surface-refresh-{name}-comparison.png')
    if 'apron-close' in results:
        im=Image.open(qa.OUT/'surface-after-apron-close.png').convert('RGB')
        im.crop((240,125,1680,880)).save(ROOT/'assets/art_refresh/surface-refresh-apron-close.png')
