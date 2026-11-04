"""Compile technical PBR maps and an inspectable atlas delivery, without changing AI albedos."""
from pathlib import Path
import json,hashlib,html,struct
import numpy as np
from PIL import Image,ImageFilter,ImageDraw
ROOT=Path(__file__).resolve().parents[1];OUT=ROOT/'exports/vehicle_atlases_2026-10-26'
KINDS={'long':'Support gun','xtralong':'Rocket artillery','troopCarrier':'Troop carrier','antiAir':'Anti-air vehicle','helicopter':'Apache','plane':'A-10'}
mapping=json.loads((OUT/'uv_mapping.json').read_text());source=json.loads((OUT/'source/runtime_kit.json').read_text())
records=[]
def signature(batch):
    value=14695981039346656037
    for field in ['vertices','normals']:
        for v in batch[field]:
            for number in v:
                for byte in struct.pack('<f',number):value=((value^byte)*1099511628211)&0xffffffffffffffff
    return f'{value:016x}'
for kind,label in KINDS.items():
    folder=OUT/kind;albedo=Image.open(folder/'albedo.png').convert('RGB');size=albedo.size
    colour=np.array(albedo,dtype=np.float32)/255
    base=np.array(Image.open(folder/'base_orm.png').convert('RGB').resize(size,Image.Resampling.BILINEAR),dtype=np.float32)/255
    identity=np.array(Image.open(folder/'material_data.png').convert('RGB').resize(size,Image.Resampling.NEAREST),dtype=np.float32)/255
    paint=identity[:,:,2]>.8
    maximum=colour.max(2);minimum=colour.min(2);saturation=(maximum-minimum)/np.maximum(maximum,.001)
    # Conservative colour-based chip estimate, restricted to known painted parts.
    # The colour atlas remains the exact original generated PNG. No colour-to-height conversion.
    exposed=paint & (saturation<.16) & (maximum>.14) & (maximum<.58)
    metallic=base[:,:,2].copy();metallic[exposed]=np.maximum(metallic[exposed],.72)
    rough=base[:,:,1].copy();rough[exposed]=np.minimum(rough[exposed],.43)
    rough=np.clip(rough,.26,.98);ao=np.clip(base[:,:,0],.66,1)
    paintmask=paint.astype(np.float32);paintmask[exposed]=.1
    def save(name,data):Image.fromarray(np.uint8(np.clip(data,0,1)*255+.5)).save(folder/(name+'.png'))
    save('orm',np.dstack([ao,rough,metallic]));save('roughness',rough);save('metallic',metallic);save('occlusion',ao);save('paint_mask',paintmask)
    batches=mapping['models'][kind]['batches'];asset=next(a for a in source['assets'] if a['name']==kind)
    actual=[b for b in asset['batches'] if b['finish'] not in {'Glass','AircraftGlass','Light'}]
    assert len(batches)==len(actual)
    paint_density=[]
    for uv,batch in zip(batches,actual):
        assert uv['finish']==batch['finish'] and uv['joint']==batch['joint']
        assert uv['geometryHash']==signature(batch)
        assert len(uv['cornerUv'])==len(batch['indices'])
        assert np.isfinite(uv['cornerUv']).all()
        assert np.min(uv['cornerUv'])>=-.0001 and np.max(uv['cornerUv'])<=1.0001
        if batch['finish']=='Team':
            p=np.array(batch['vertices'])[np.array(batch['indices']).reshape(-1,3)]
            world=np.linalg.norm(np.cross(p[:,1]-p[:,0],p[:,2]-p[:,0]),axis=1).sum()/2
            t=np.array(uv['cornerUv']).reshape(-1,3,2);u=t[:,1]-t[:,0];v=t[:,2]-t[:,0]
            uv_area=np.abs(u[:,0]*v[:,1]-u[:,1]*v[:,0]).sum()/2
            if world>.5:paint_density.append(size[0]*np.sqrt(uv_area/world))
    if kind in {'long','xtralong','antiAir'}:
        assert max(paint_density)/min(paint_density)<1.1,(kind,'unequal hull / turret detail density',paint_density)
    normal=np.array(Image.open(folder/'normal.png').convert('RGB'),dtype=np.float32)/127.5-1
    coverage=np.array(Image.open(folder/'material_data.png').getchannel('A'))>254
    # Bake/interpolation rounding permits a small unit-vector tolerance.
    lengths=np.linalg.norm(normal[coverage],axis=1)
    assert np.mean((lengths>.9)&(lengths<1.1))>.98,(kind,'invalid normal vectors')
    names=['albedo','normal','roughness','metallic','occlusion','orm','paint_mask']
    maps={name:{'path':f'{kind}/{name}.png','size':list(Image.open(folder/(name+'.png')).size),'sha256':hashlib.sha256((folder/(name+'.png')).read_bytes()).hexdigest()} for name in names}
    records.append({'kind':kind,'label':label,'triangles':mapping['models'][kind]['triangles'],'batches':len(batches),
        'paintTexelsPerMetre':[round(float(v),2) for v in paint_density],'maps':maps})
    # Overview thumbnail only. Full-resolution albedo is copied byte-for-byte.
    thumb=albedo.copy();thumb.thumbnail((480,480));thumb.save(folder/'thumbnail.jpg',quality=94)
manifest={'format':1,'appliedToGame':False,'normalConvention':'Blender/OpenGL tangent-space, UV Atlas; runtime cornerUv uses V-down so invert normal green when importing those UVs.',
    'colourSpace':{'albedo':'sRGB','normal':'linear','roughness':'linear','metallic':'linear','occlusion':'linear','orm':'linear'},
    'ormChannels':{'R':'occlusion','G':'roughness','B':'metallic'},
    'provenance':'AI-generated colour edits, exact geometry UVs from Blender; existing physical normal/ORM rebaked, conservative paint-chip metallic/roughness estimate. These are authored approximations, not measured scan maps.',
    'vehicles':records}
(OUT/'manifest.json').write_text(json.dumps(manifest,indent=2))
generation=json.loads((OUT/'generation.json').read_text())
for record in generation['records']:
    delivered=OUT/record['kind']/'albedo.png'
    assert hashlib.sha256(delivered.read_bytes()).digest()==hashlib.sha256(Path(record['path']).read_bytes()).digest()
rows=[]
for rec in records:
    k=rec['kind'];label=html.escape(rec['label'])
    rows.append(f'''<article><h2>{label}</h2><a href="{k}/render.png"><img class="beauty" src="{k}/render.png" alt="{label} 3D preview"></a>
    <div class="compare"><figure><a href="{k}/source_albedo.png"><img src="{k}/source_albedo.png"></a><figcaption>Existing surfaces baked into new UVs</figcaption></figure><figure><a href="{k}/albedo.png"><img src="{k}/albedo.png"></a><figcaption>New realistic colour atlas</figcaption></figure></div>
    <nav>{' '.join(f'<a href="{k}/{n}.png">{n.replace("_"," ")}</a>' for n in ['albedo','normal','roughness','metallic','occlusion','orm','paint_mask'])} <a href="{k}/uv_layout.svg">UV layout</a></nav></article>''')
page='''<!doctype html><meta charset="utf-8"><title>Rule Of War — vehicle texture atlases</title><style>
body{background:#141815;color:#e6e4d9;font:16px/1.55 system-ui;margin:36px auto;max-width:1400px;padding:0 24px}h1{font-weight:500}p{max-width:1000px;color:#bfc5b8}main{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:24px}article{background:#202621;padding:20px;border:1px solid #3a4439;border-radius:8px}h2{font-size:22px;margin:0 0 12px}a{color:#d7c68d}img{width:100%;display:block}img.beauty{aspect-ratio:4/3;object-fit:contain;background:#151916}.compare{display:grid;grid-template-columns:1fr 1fr;gap:12px;margin:16px 0}figure{margin:0}figcaption{font-size:13px;color:#bbc1b5;margin-top:5px}nav a{display:inline-block;padding:5px 8px;border:1px solid #4e5948;margin:4px 4px 0 0;text-decoration:none;font-size:13px}@media(max-width:800px){main{grid-template-columns:1fr}}</style>
<h1>Rule Of War · vehicle texture atlases</h1><p>Six new vehicle-specific sets in the battle tank reference's realistic weathered style. Click a map to inspect the full PNG. Colour atlases are 1254²; rebaked physical normal maps are 2048². Turret, launcher and rotor parts retain separate geometry.</p>
<p><strong>Asset delivery — not yet installed in the running game.</strong> New UVs are supplied in the Blender file and per-triangle runtime mapping. Glass and lights retain their existing separate materials. Normal/roughness/metallic maps are authored approximations; colour-only rivet detail is not newly modelled geometry.</p>
<p><a href="vehicles_textured.blend">Open Blender scene</a> · <a href="manifest.json">Map specification</a> · <a href="generation.json">Exact prompts and generation provenance</a> · <a href="uv_mapping.json">Runtime UV mapping</a></p><main>'''+''.join(rows)+'</main>'
(OUT/'comparison.html').write_text(page,encoding='utf-8')
print(json.dumps({'vehicles':len(records),'maps':sum(len(r['maps']) for r in records),'uvTriangles':sum(r['triangles'] for r in records),'albedosUnmodified':True}))
