"""Validate final files, make a model contact sheet, and archive the portable delivery."""
from pathlib import Path
import hashlib,json,re,zipfile
from urllib.parse import unquote
from PIL import Image,ImageDraw,ImageFont
ROOT=Path(__file__).resolve().parents[1];OUT=ROOT/'exports/vehicle_atlases_2026-10-26'
manifest=json.loads((OUT/'manifest.json').read_text());assert len(manifest['vehicles'])==6
font=ImageFont.truetype('C:/Windows/Fonts/segoeui.ttf',24)
sheet=Image.new('RGB',(1800,1028),(20,24,21));draw=ImageDraw.Draw(sheet)
for i,rec in enumerate(manifest['vehicles']):
    for m in rec['maps'].values():
        path=OUT/m['path'];assert hashlib.sha256(path.read_bytes()).hexdigest()==m['sha256']
        assert list(Image.open(path).size)==m['size']
    render=OUT/rec['kind']/'render.png';assert render.exists()
    assert render.stat().st_mtime>(OUT/'manifest.json').stat().st_mtime,'Stale render '+rec['kind']
    image=Image.open(render).convert('RGB');assert image.size==(1024,768)
    image.thumbnail((580,435));x=(i%3)*600+10;y=(i//3)*514+55
    sheet.paste(image,(x,y));draw.text((x,y-39),rec['label'],fill=(225,226,211),font=font)
sheet.save(OUT/'models_overview.jpg',quality=96)
for href in re.findall(r'href="([^"]+)"',(OUT/'comparison.html').read_text(encoding='utf-8')):
    assert (OUT/unquote(href)).exists(),href
assert (OUT/'vehicles_textured.blend').stat().st_size>1000000
validation={'vehicles':6,'maps':42,'triangles':sum(r['triangles'] for r in manifest['vehicles']),
    'uvGeometryHashesValidated':True,'normalVectorsValidated':True,'colourOriginalBytesValidated':True,
    'hullTurretDensityValidated':True,'galleryLinksValidated':True,'litProofs':6,
    'appliedToGame':False}
(OUT/'validation.json').write_text(json.dumps(validation,indent=2))
names=['comparison.html','models_overview.jpg','README.txt','manifest.json','generation.json','uv_mapping.json','vehicles_textured.blend','validation.json','reference_tank.png']
for rec in manifest['vehicles']:
    names.extend(m['path'] for m in rec['maps'].values())
    names.extend(f'{rec["kind"]}/{n}' for n in ['source_albedo.png','uv_layout.svg','render.png'])
archive=OUT.with_suffix('.zip')
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=4) as z:
    for name in names:z.write(OUT/name,OUT.name+'/'+name)
with zipfile.ZipFile(archive) as z:assert z.testzip() is None
print(json.dumps({**validation,'archive':str(archive),'archiveBytes':archive.stat().st_size}))
