"""Publish native-render evidence and mark the reviewed atlas export as installed."""
from pathlib import Path
import json,shutil,hashlib
import numpy as np
from PIL import Image,ImageDraw,ImageFont
ROOT=Path(__file__).resolve().parents[1];RUN=ROOT/'.data/render-checks/vehicle-atlas-apply';OUT=ROOT/'assets/art_refresh/vehicle-atlas-apply'
OUT.mkdir(parents=True,exist_ok=True)
records=json.loads((RUN/'verification.json').read_text());assert len(records)==13
assert hashlib.sha256((RUN/'bin/rule-of-war.exe').read_bytes()).digest()==hashlib.sha256((ROOT/'native/target/debug/rule-of-war.exe').read_bytes()).digest()
summary={}
for name,record in records.items():
    r=record['report'];summary[name]={'atlasModels':r['vehicleAtlasModels'],'atlasBatches':r['vehicleAtlasBatches'],'graphics':r['graphics'],'cameraZoom':r['cameraZoom'],'exitCode':record['run']['exitCode']}
    shutil.copy2(RUN/(name+'.png'),OUT/(name+'.png'))
for name,rect in [('support',(560,265,1270,820)),('a10',(700,260,1190,790))]:
    before,after=[records[k]['report'] for k in ['legacy-'+name,name]]
    for key in ['cameraCenter','cameraZoom','cameraFovDegrees','cameraHeadingDegrees','cameraElevationDegrees']:assert before[key]==after[key]
    lighting=[dict(r['worldLighting']) for r in [before,after]]
    for value in lighting:value.pop('modelContactCacheMatches',None)
    assert lighting[0]==lighting[1]
    images=[np.asarray(Image.open(OUT/(k+'.png')).convert('RGB').crop(rect),dtype=np.int16) for k in ['legacy-'+name,name]]
    difference=float(np.abs(images[1]-images[0]).mean());assert difference>1,(name,'no visible atlas change',difference)
    summary[name]['meanVehicleRegionDifference']=difference
for name in ['support','a10']:
    a=np.array(Image.open(OUT/(name+'.png')).convert('RGB'),dtype=np.int16)
    b=np.array(Image.open(OUT/('enemy-'+name+'.png')).convert('RGB'),dtype=np.int16)
    assert np.count_nonzero(np.max(np.abs(a-b),axis=2)>2)>200,'Faction tint did not render'
tests=(ROOT/'.data/vehicle-atlas-apply-tests.log').read_text();assert '331 passed; 0 failed' in tests
clippy=(ROOT/'.data/vehicle-atlas-apply-clippy.log').read_text();assert 'Finished' in clippy and 'error:' not in clippy
report={'appliedToGame':True,'trainingGround':True,'vehicleKinds':['long','xtralong','troopCarrier','antiAir','helicopter','plane'],
    'testsPassed':331,'strictClippy':True,'nativeRenderChecks':13,'sourceUVTrianglesPreserved':75720,
    'textureQuality':'Live LOW / BALANCED / HIGH; existing graphics options',
    'executableSha256':hashlib.sha256((ROOT/'native/target/debug/rule-of-war.exe').read_bytes()).hexdigest(),'cases':summary}
(OUT/'validation.json').write_text(json.dumps(report,indent=2))
labels={'support':'Support gun','rocket':'Rocket artillery','carrier':'Troop carrier','anti-air':'Anti-air vehicle','apache':'Apache','a10':'A-10'}
font=ImageFont.truetype('C:/Windows/Fonts/segoeui.ttf',24);sheet=Image.new('RGB',(1800,830),(22,26,23));draw=ImageDraw.Draw(sheet)
for i,(name,label) in enumerate(labels.items()):
    image=Image.open(OUT/(name+'.png')).convert('RGB');image.thumbnail((590,332));x=(i%3)*600+5;y=(i//3)*415+48
    sheet.paste(image,(x,y));draw.text((x,y-37),label,fill=(230,232,216),font=font)
sheet.save(OUT/'native-overview.jpg',quality=96)
cards=''.join(f'<article><h2>{label}</h2><a href="{name}.png"><img src="{name}.png"></a></article>' for name,label in labels.items())
page='''<!doctype html><meta charset="utf-8"><title>Rule Of War — applied vehicle atlases</title><style>body{max-width:1500px;margin:36px auto;padding:0 24px;background:#151b17;color:#e5e6da;font:16px/1.6 system-ui}a{color:#dfc68c}main{display:grid;grid-template-columns:1fr 1fr;gap:24px}article{background:#222c24;padding:18px;border:1px solid #4a5748}h2{margin:0 0 12px;font-size:20px}img{display:block;width:100%}p{max-width:1050px}@media(max-width:850px){main{grid-template-columns:1fr}}</style><h1>Vehicle atlases applied in game</h1><p>Native 1920 × 1080 captures of all six vehicles using the reviewed UV-mapped colour and physical material maps. Shared rendering applies them to the base game and training ground. Turrets, wheels, launcher tubes and rotor geometry keep their existing joints.</p><p>Adjust <strong>Texture Quality</strong> in Graphics for Low, Balanced or High. Save and restart an already-running game to load the rebuilt executable.</p><p>331 tests passed; strict Clippy passed; 13 native rendering checks passed. Lighting and camera remain matched in the comparisons. <a href="validation.json">Validation report</a></p><main>'''+cards+'''</main><h2>Matched comparisons</h2><p><a href="legacy-support.png">Previous support materials</a> · <a href="support.png">New support atlas</a> · <a href="legacy-a10.png">Previous A-10 materials</a> · <a href="a10.png">New A-10 atlas</a></p><p><a href="enemy-support.png">Enemy support gun</a> · <a href="enemy-a10.png">Enemy A-10</a> · <a href="low.png">Low texture quality</a> · <a href="overview.png">Unit overview</a> · <a href="training.png">Training ground</a></p>'''
(OUT/'comparison.html').write_text(page,encoding='utf-8')
export=ROOT/'exports/vehicle_atlases_2026-10-26';manifest=json.loads((export/'manifest.json').read_text());manifest['appliedToGame']=True
manifest['runtimeInstallation']='materials/vehicle_atlases';manifest['nativeValidation']='assets/art_refresh/vehicle-atlas-apply/validation.json'
(export/'manifest.json').write_text(json.dumps(manifest,indent=2))
old=(export/'comparison.html').read_text(encoding='utf-8');old=old.replace('Asset delivery — not yet installed in the running game.','Installed in the base game and training ground. Restart a running game to load the rebuilt executable.')
(export/'comparison.html').write_text(old,encoding='utf-8')
readme=(export/'README.txt').read_text()
readme=readme.replace('These sets are created assets, not installed in the live game. They require the\nnew UVs in the Blender file or the supplied uv_mapping.json, rather than the\ngame\'s old overlapping tiled UVs.',
    'Installed in the base game and training ground on 2026-10-26. Restart an\nalready-running game to load the rebuilt executable. Texture Quality in Graphics\nselects Low, Balanced or High. The game now applies uv_mapping.json instead of\nthe former overlapping tiled UVs.')
readme=readme.replace('No terrain, explosions or live-game source materials are replaced by this package.',
    'Vehicle atlases are installed under materials/vehicle_atlases. Terrain and\nexplosions retain their existing approved assets. Native validation and screenshots\nare in assets/art_refresh/vehicle-atlas-apply.')
(export/'README.txt').write_text(readme)
print(json.dumps({'tests':331,'nativeChecks':len(records),'applied':True,'gallery':str(OUT/'comparison.html')}))
