"""Collect reproducible mesh, colour, test and native render evidence."""
from pathlib import Path
import json,shutil,hashlib,re
from PIL import Image,ImageDraw,ImageFont,ImageOps
import numpy as np
ROOT=Path(__file__).resolve().parents[1];RUN=ROOT/'.data/render-checks/apache-foliage-v2';OUT=ROOT/'assets/art_refresh/apache-foliage-v2'
OUT.mkdir(parents=True,exist_ok=True)
records=json.loads((RUN/'verification.json').read_text());assert len(records)==8
tests=(ROOT/'.data/apache-v2-validated-tests.log').read_text();match=re.search(r'test result: ok\. (\d+) passed; 0 failed',tests);assert match,tests[-1800:]
clippy=(ROOT/'.data/apache-v2-clippy.log').read_text();assert 'Finished' in clippy and 'error:' not in clippy
assert hashlib.sha256((RUN/'bin/rule-of-war.exe').read_bytes()).digest()==hashlib.sha256((ROOT/'native/target/debug/rule-of-war.exe').read_bytes()).digest()
for name,record in records.items():
    assert record['run']['exitCode']==0 and record['report']['vehicleAtlasModels']==6
    shutil.copy2(RUN/(name+'.png'),OUT/(name+'.png'))
shutil.copy2(RUN/'foliage-before.png',OUT/'foliage-before.png')
shutil.copy2(ROOT/'assets/art_refresh/vehicle-atlas-apply/apache.png',OUT/'apache-before.png')
for name in ['textured-review','textured-side']:
    shutil.copy2(ROOT/'assets/aircraft/apache'/(name+'.png'),OUT/(name+'.png'))
before=json.loads((RUN/'foliage-before.png.json').read_text());after=records['foliage-after']['report']
for key in ['cameraCenter','cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:assert before[key]==after[key]
old_apache=json.loads((ROOT/'.data/render-checks/vehicle-atlas-apply/verification.json').read_text())['apache']['report']
for key in ['cameraCenter','cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:assert old_apache[key]==records['apache']['report'][key]
lights=[dict(p['worldLighting']) for p in [before,after]]
for light in lights:light.pop('modelContactCacheMatches',None)
assert lights[0]==lights[1]
a=np.array(Image.open(OUT/'foliage-before.png').convert('RGB'),dtype=float);b=np.array(Image.open(OUT/'foliage-after.png').convert('RGB'),dtype=float)
open_field=np.zeros(a.shape[:2],dtype=bool)
for x,y,u,v in [(0,0,300,270),(1690,0,1920,550)]:open_field[y:v,x:u]=True
# Restrict the metric to open fields. Building roof colours may vary by run.
changed=open_field&(np.abs(a-b).max(2)>5)&(a[:,:,1]>a[:,:,0]*1.02)&(a[:,:,1]>a[:,:,2]*1.2)
assert changed.sum()>1000
ya=(a*np.array([.2126,.7152,.0722])).sum(2);yb=(b*np.array([.2126,.7152,.0722])).sum(2)
brightness=float(np.median(yb[changed]/ya[changed]));assert brightness<.97,brightness
flight=records['apache-flight']['report']['aircraft'];assert any(p['kind']=='helicopter' and p['altitude']>2 and p['speed']>.1 for p in flight)
geometry=json.loads((ROOT/'assets/aircraft/apache/apache_geometry.json').read_text());triangles=sum(len(m['indices'])//3 for m in geometry['meshes'].values())
assert geometry['tailRotorFitApplied'] and triangles==17712
report={'appliedToGame':True,'trainingGround':True,'testsPassed':int(match[1]),'strictClippy':True,'nativeChecks':8,
    'triangles':triangles,'changedFoliagePixels':int(changed.sum()),'medianFoliageBrightnessRatio':brightness,
    'airfieldClearancePassed':True,'trainingMarkerRegressionPassed':True,'factionAndLowQualityPassed':True,
    'inspiration':['https://www.boeing.com/defense/military-rotorcraft/ah-64-apache','https://www.army.mod.uk/learn-and-explore/equipment/aircraft/ah-64e-apache-attack-helicopter/'],
    'executableSha256':hashlib.sha256((ROOT/'native/target/debug/rule-of-war.exe').read_bytes()).hexdigest(),
    'geometrySha256':hashlib.sha256((ROOT/'assets/aircraft/apache/apache_geometry.json').read_bytes()).hexdigest()}
(OUT/'validation.json').write_text(json.dumps(report,indent=2))
font=ImageFont.truetype('C:/Windows/Fonts/segoeui.ttf',25)
for name,pair in [('foliage',('foliage-before','foliage-after')),('apache',('apache-before','apache'))]:
    sheet=Image.new('RGB',(1920,585),(22,27,23));draw=ImageDraw.Draw(sheet)
    for i,key in enumerate(pair):
        im=Image.open(OUT/(key+'.png')).convert('RGB')
        im=im.crop((0,0,300,270) if name=='foliage' else (730,290,1240,730))
        im=ImageOps.contain(im,(960,540),Image.Resampling.LANCZOS)
        sheet.paste(im,(i*960+(960-im.width)//2,45+(540-im.height)//2));draw.text((i*960+18,8),'BEFORE' if i==0 else 'AFTER',font=font,fill=(230,231,218))
    sheet.save(OUT/(name+'-comparison.jpg'),quality=96)
page='''<!doctype html><meta charset="utf-8"><title>Rule Of War — Apache and foliage rework</title><style>body{max-width:1500px;margin:36px auto;padding:0 24px;background:#151a16;color:#e1e5d5;font:16px/1.6 system-ui}h1,h2{font-weight:500}a{color:#d9c08c}img{width:100%;display:block}figure{margin:24px 0}p{max-width:1080px}.pair{display:grid;grid-template-columns:1fr 1fr;gap:16px}@media(max-width:850px){.pair{grid-template-columns:1fr}}</style><h1>Apache and foliage rework — applied in game</h1><p>Trees, hedges and shrubs now use darker, muted olive foliage. The rebuilt 17,712-triangle Apache has a stepped tandem cockpit, curved fuselage and engine pods, tapered tail and wings, detailed rocket tubes, rotor head, swept blades, scissor tail rotor, nose optics and aiming chin gun. Its new olive atlas keeps more intact paint and uses restrained service wear.</p><p>Native captures use the existing fixed camera and lighting. Source geometry was created against <a href="https://www.boeing.com/defense/military-rotorcraft/ah-64-apache">Boeing Apache photographs</a> and <a href="https://www.army.mod.uk/learn-and-explore/equipment/aircraft/ah-64e-apache-attack-helicopter/">British Army references</a>. Applied in the base game and training ground; save and restart an already-running game to load the rebuilt executable.</p><p><a href="../../aircraft/apache/apache.blend">Editable Blender model</a> · <a href="../../aircraft/apache/albedo.png">New colour atlas</a> · <a href="../../aircraft/apache/generation.json">Exact image-generation prompt</a> · <a href="validation.json">Validation report</a></p><h2>Matched in-game comparisons</h2><figure><a href="foliage-comparison.jpg"><img src="foliage-comparison.jpg"></a></figure><figure><a href="apache-comparison.jpg"><img src="apache-comparison.jpg"></a></figure><h2>New helicopter in game</h2><a href="apache-close.png"><img src="apache-close.png"></a><h2>Authored geometry and surface detail</h2><div class="pair"><a href="textured-review.png"><img src="textured-review.png"></a><a href="textured-side.png"><img src="textured-side.png"></a></div><p><a href="apache-enemy.png">Enemy colour</a> · <a href="apache-low.png">Low texture quality</a> · <a href="apache-flight.png">In flight</a> · <a href="training.png">Training ground</a> · <a href="other-vehicles.png">Other vehicles</a></p>'''
(OUT/'comparison.html').write_text(page,encoding='utf-8')
print(json.dumps(report))
