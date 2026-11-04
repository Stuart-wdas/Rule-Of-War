"""Cover the previously untreated airbase foundation, not just slab paving."""
from pathlib import Path
import argparse,json
from PIL import Image,ImageDraw
import check_world_detail as qa

ROOT=Path(__file__).resolve().parents[1]
qa.OUT=ROOT/'.data/render-checks/concrete-weathering'
CASES={
    'hardstanding':['--aircraft-parking','--hardstanding-close','--graphics','high'],
    'apron':['--aircraft-parking','--apron-close','--graphics','high'],
    'low':['--aircraft-parking','--hardstanding-close','--graphics','low'],
    'dlss':['--aircraft-parking','--hardstanding-close','--graphics','high','--upscaler','quality'],
    'settings-cycle':['--aircraft-parking','--hardstanding-close','--graphics','balanced','--graphics-cycle'],
    'town':['--environment-town','--graphics','balanced'],
    'wide':['--aircraft-parking','--graphics','balanced'],
}
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--only',nargs='+');args=parser.parse_args()
    qa.OUT.mkdir(parents=True,exist_ok=True)
    path=qa.OUT/'verification.json'
    results=json.loads(path.read_text()) if path.exists() else {}
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        result=qa.run('weathered-'+name,qa.CURRENT,flags);report=result['report']
        assert report['concreteGroundBatches']>0
        if name!='town':
            assert report['foundationGroundBatches']>0,'The plain airbase foundation was not routed to its weathered material'
        assert report['concreteTexturePixels']=={'low':512,'balanced':1024,'high':2048}[report['graphics']['textures']]
        if name=='dlss':
            assert report['dlssSupported'] and report['graphics']['upscaler']=='quality'
            assert any('dlss_super_resolution' in key for key in report['gpuPassesMs'])
        before=qa.OUT/f'before-{name}.png.json'
        if before.exists():
            old=json.loads(before.read_text())
            for key in ['cameraCenter','cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:
                assert old[key]==report[key],(name,key)
        results[name]=result;path.write_text(json.dumps(results,indent=2))
        print('VERIFIED concrete weathering',name,flush=True)
    for name in ['apron','town']:
        if name not in results:continue
        out=Image.new('RGB',(1920,580),(24,28,27));draw=ImageDraw.Draw(out)
        for i,p in enumerate([qa.OUT/f'before-{name}.png',qa.OUT/f'weathered-{name}.png']):
            im=Image.open(p).convert('RGB').resize((960,540),Image.Resampling.LANCZOS)
            out.paste(im,(i*960,40));draw.text((i*960+16,12),['BEFORE','WEATHERED'][i],fill='white')
        out.save(ROOT/f'assets/art_refresh/concrete-weathering-{name}-comparison.png')
    if 'hardstanding' in results:
        im=Image.open(qa.OUT/'weathered-hardstanding.png').convert('RGB').crop((200,145,1720,810))
        im.save(ROOT/'assets/art_refresh/concrete-weathering-hardstanding.png')
