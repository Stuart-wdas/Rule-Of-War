"""Real renderer checks for the shared military kits and baked infantry export."""
from pathlib import Path
import argparse,json
from PIL import Image,ImageDraw,ImageFont,ImageOps
import check_world_detail as qa

ROOT=Path(__file__).resolve().parents[1]
qa.OUT=ROOT/'.data/render-checks/model-overhaul'
CASES={
    'infantry':['--infantry-models'],
    'tank':['--tank-model'],
    'support':['--vehicle-model','long'],
    'carrier':['--vehicle-model','troopCarrier'],
    'rocket':['--vehicle-model','xtralong'],
    'anti-air':['--vehicle-model','antiAir'],
    'apache':['--aircraft-parking','--apache-model'],
    'a10':['--aircraft-parking','--warthog-model'],
    'town':['--environment-town'],
    'facilities':['--training','--training-tools','--training-buildings'],
    'low':['--models','--graphics','low'],
    'dlss':['--infantry-models','--graphics','high','--upscaler','quality'],
    'battle':['--mock-battle','combined','--battle-seconds','30'],
}
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--only',nargs='+');args=p.parse_args()
    path=qa.OUT/'verification.json'
    results=json.loads(path.read_text()) if path.exists() else {}
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        flags=flags if '--graphics' in flags else [*flags,'--graphics','balanced']
        result=qa.run('models-after-'+name,qa.CURRENT,flags)
        if name=='dlss':
            assert result['report']['dlssSupported']
        if name in ['infantry','dlss']:
            assert result['report']['skinnedInfantry']==6
        if name in ['apache','a10']:
            assert len(result['report']['aircraft'])==4
            assert result['report']['cameraZoom']==(36 if name=='apache' else 48)
        results[name]=result
        path.write_text(json.dumps(results,indent=2))
        print('VERIFIED model overhaul',name,flush=True)
    before_path=qa.OUT/'before.json'
    if before_path.exists():
        before=json.loads(before_path.read_text())
        for name in ['infantry','tank','apache','a10','town']:
            if name not in results or name not in before:continue
            for key in ['cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:
                assert abs(before[name]['report'][key]-results[name]['report'][key])<.001,(name,key)
            comparison=Image.new('RGB',(1920,740),(27,30,27))
            for i,version in enumerate(['before','after']):
                im=Image.open(qa.OUT/f'models-{version}-{name}.png').convert('RGB')
                im=im.crop((480,145,1440,845))
                comparison.paste(im,(i*960,40))
            draw=ImageDraw.Draw(comparison)
            draw.text((20,14),'BEFORE: '+name.upper(),fill='white')
            draw.text((980,14),'AFTER: '+name.upper(),fill='white')
            comparison.save(ROOT/f'assets/art_refresh/model-overhaul-{name}.png')
    items=[('tank','BATTLE TANK',(540,185,1370,800)),
        ('carrier','TROOP CARRIER',(540,185,1370,800)),
        ('infantry','INFANTRY',(650,145,1400,780)),
        ('apache','APACHE',(400,110,1520,800)),
        ('a10','A-10',(300,110,1620,800)),
        ('town','ARCHITECTURE',(350,145,1570,800))]
    if all(name in results for name,_,_ in items):
        gallery=Image.new('RGB',(1800,916),(25,29,28))
        draw=ImageDraw.Draw(gallery)
        font=ImageFont.truetype('C:/Windows/Fonts/segoeui.ttf',22)
        for i,(name,label,crop) in enumerate(items):
            im=Image.open(qa.OUT/f'models-after-{name}.png').convert('RGB').crop(crop)
            im=ImageOps.contain(im,(592,412),Image.Resampling.LANCZOS)
            x=(i%3)*600;y=(i//3)*458
            draw.text((x+14,y+9),label,font=font,fill=(231,222,199))
            gallery.paste(im,(x+(600-im.width)//2,y+42+(412-im.height)//2))
        gallery.save(ROOT/'assets/art_refresh/model-overhaul-gallery.png')
