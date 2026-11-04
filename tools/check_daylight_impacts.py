"""Native GPU review of the published four-family Cycles impact pack."""
import argparse,json
from pathlib import Path
from PIL import Image, ImageDraw
import check_world_detail as qa

ROOT=Path(__file__).resolve().parents[1]
qa.OUT=ROOT/'.data/daylight-review'
CASES={
    'shell': (0,.35,'shell'),
    'artillery': (1,.55,'debris'),
    'rocket': (3,.25,'rocket'),
    'vehicle': (2,.75,'mushroom'),
    'rocket-smoke': (3,1.1,'rocket'),
}
if __name__=='__main__':
    p=argparse.ArgumentParser(); p.add_argument('--only',nargs='+'); args=p.parse_args()
    qa.OUT.mkdir(parents=True,exist_ok=True)
    record=qa.OUT/'verification.json'
    results=json.loads(record.read_text()) if record.exists() else {}
    for name,(effect,age,family) in CASES.items():
        if args.only and name not in args.only: continue
        flags=['--visual-demo','--demo-effect',str(effect),'--demo-scale','1',
               '--demo-age',str(age),'--demo-close','--graphics','balanced']
        result=qa.run('after-'+name,qa.CURRENT,flags)
        # A cold GPU pipeline can produce a capture before the world shaders
        # finish compiling. Never accept metadata alone as a visible render.
        def world_visible():
            im=Image.open(qa.OUT/f'after-{name}.png').convert('L').crop((0,160,1920,950))
            histogram=im.histogram()
            return sum(histogram[40:])/sum(histogram)>.55
        if not world_visible():
            result=qa.run('after-'+name,qa.CURRENT,flags)
        assert world_visible(),'World shaders were not visible in the GPU capture'
        report=result['report']
        assert report['blenderDaylightVfx'] and report['blenderSimulatedHeat']
        assert report['blenderFramePixels']==512
        assert [family,0] in report['observedBlenderStyles'],report['observedBlenderStyles']
        assert 1000<report['blenderTextureMiB']<1030
        results[name]=result
        (qa.OUT/'verification.json').write_text(json.dumps(results,indent=2))
        print('VERIFIED DAYLIGHT',name,flush=True)
    if not args.only or 'low' in args.only:
        result=qa.run('after-low',qa.CURRENT,['--visual-demo','--demo-effect','1',
            '--demo-scale','1','--demo-age','.55','--graphics','low','--vfx-quality','low'])
        assert result['report']['blenderFramePixels']==256
        assert result['report']['blenderDaylightVfx']
        results['low']=result
    (qa.OUT/'verification.json').write_text(json.dumps(results,indent=2))
    board=Image.new('RGB',(1600,1080),(26,31,28))
    draw=ImageDraw.Draw(board)
    for i,name in enumerate(['shell','artillery','rocket','vehicle']):
        x,y=i%2*800,i//2*540
        with Image.open(qa.OUT/f'after-{name}.png') as im:
            im=im.convert('RGB').resize((800,450),Image.Resampling.LANCZOS)
            board.paste(im,(x,y+40))
        draw.text((x+16,y+12),name.upper()+' / ACTUAL GAME CAPTURE',fill='white')
    board.save(ROOT/'assets/Realistic_Explosion_VFX/daylight_impacts/in-game-review.jpg',quality=94)
    print('DAYLIGHT NATIVE REVIEW COMPLETE',flush=True)
