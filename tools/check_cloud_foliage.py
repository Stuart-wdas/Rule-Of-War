"""Native checks for projected weather shadows and the revised plant meshes."""
import argparse,json,shutil
from pathlib import Path
import numpy as np
from PIL import Image,ImageDraw,ImageFont
import check_world_detail as qa

ROOT=Path(__file__).resolve().parents[1]
qa.OUT=ROOT/'.data/cloud-foliage'
OUT=ROOT/'assets/art_refresh/cloud-foliage'
CASES={
    'clear':['--environment-town','--no-cloud-shadows','--graphics','high'],
    'cloud-0':['--environment-town','--cloud-time','0','--graphics','high'],
    'cloud-120':['--environment-town','--cloud-time','120','--graphics','high'],
    'foliage-close':['--environment-town','--foliage-close','--cloud-time','0','--graphics','high'],
    'low':['--environment-town','--cloud-time','0','--graphics','low'],
    'training':['--training','--training-tools','--cloud-time','0','--graphics','balanced'],
}

def world_visible(path):
    rgb=np.asarray(Image.open(path).convert('RGB'))[160:950].astype('int16')
    # A depth prepass may finish before an opaque colour pipeline. Those
    # missing surfaces expose the dark-blue clear colour, not natural shade.
    holes=(rgb[:,:,0]<20)&(rgb[:,:,1]<26)&(rgb[:,:,2]<36)&(rgb[:,:,2]>rgb[:,:,0]+2)
    if holes.mean()>.01:return False
    im=Image.open(path).convert('L').crop((0,160,1920,790))
    h=im.histogram()
    return sum(h[40:])/sum(h)>.45

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--only',nargs='+');args=p.parse_args()
    qa.OUT.mkdir(parents=True,exist_ok=True);OUT.mkdir(parents=True,exist_ok=True)
    # Keep one executable snapshot throughout a comparison, and allow another
    # build to replace native/target/debug/rule-of-war.exe during the captures.
    executable=qa.OUT/'review.exe'
    shutil.copy2(qa.CURRENT,executable)
    record=qa.OUT/'verification.json'
    results=json.loads(record.read_text()) if record.exists() else {}
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        flags=[*flags,'--hide-ui','--vfx-quality','low']
        result=qa.run(name,executable,flags)
        path=qa.OUT/(name+'.png')
        if not world_visible(path):result=qa.run(name,executable,flags)
        assert world_visible(path),'World shaders were not visible in capture'
        report=result['report'];cloud=report['cloudShadows']
        assert cloud['enabled']==(name!='clear')
        if name in ['cloud-0','cloud-120']:
            assert cloud['seconds']==(120 if name=='cloud-120' else 0)
        results[name]=result;record.write_text(json.dumps(results,indent=2))
        Image.open(path).save(OUT/(name+'.jpg'),quality=95)
        print('VERIFIED WEATHER / FOLIAGE',name,flush=True)
    if all(k in results for k in ['clear','cloud-0','cloud-120']):
        for key in ['cameraCenter','cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:
            assert results['clear']['report'][key]==results['cloud-0']['report'][key]==results['cloud-120']['report'][key]
        images=[np.asarray(Image.open(qa.OUT/(n+'.png')).convert('RGB')).astype('float32') for n in ['clear','cloud-0','cloud-120']]
        a,b,c=images
        field=(a[:,:,1]>a[:,:,0]*1.01)&(a[:,:,1]>a[:,:,2]*1.10)&(a.mean(2)>40)
        field[:150]=False;field[850:]=False
        shadow=b if ((abs(a-b).max(2)>3)&field).sum()>((abs(a-c).max(2)>3)&field).sum() else c
        changed=(abs(a-shadow).max(2)>3)&field
        moved=(abs(b-c).max(2)>3)&field
        assert changed.sum()>2000,'Sun mask did not shade the ground on the GPU'
        assert moved.sum()>2000,'Cloud shadows did not travel across the ground'
        assert np.median(shadow[changed].mean(1)/a[changed].mean(1))<.99
        summary=dict(shadedGroundPixels=int(changed.sum()),movingShadowPixels=int(moved.sum()),
                     shadeBrightnessRatio=float(np.median(shadow[changed].mean(1)/a[changed].mean(1))),
                     cloudTimeSeconds=[0,120])
        (OUT/'validation.json').write_text(json.dumps(summary,indent=2))
        font=ImageFont.truetype('C:/Windows/Fonts/arial.ttf',24)
        board=Image.new('RGB',(1920,1160),(24,30,27));draw=ImageDraw.Draw(board)
        for i,(name,label) in enumerate([('clear','CLEAR SUN / REVISED FOLIAGE'),('cloud-0','SOFT CLOUD SHADOWS'),('cloud-120','CLOUDS AFTER 120 SECONDS'),('foliage-close','CLOSER LOOK AT VEGETATION')]):
            path=qa.OUT/(name+'.png')
            if not path.exists():continue
            x,y=i%2*960,i//2*580
            im=Image.open(path).convert('RGB').resize((960,540),Image.Resampling.LANCZOS)
            board.paste(im,(x,y+40));draw.text((x+16,y+8),label,font=font,fill='white')
        board.save(OUT/'review.jpg',quality=94)
        print(json.dumps(summary),flush=True)
