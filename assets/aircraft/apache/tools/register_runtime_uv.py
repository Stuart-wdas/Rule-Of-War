"""Register the exact new runtime geometry with the unchanged authored atlas UVs."""
from pathlib import Path
import json,struct,hashlib
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'assets/aircraft/apache'
source=json.loads((OUT/'runtime/runtime_kit.json').read_text());asset=next(a for a in source['assets'] if a['name']=='helicopter')
def signature(batch):
    value=14695981039346656037
    for field in ['vertices','normals']:
        for vertex in batch[field]:
            for n in vertex:
                for byte in struct.pack('<f',n):value=((value^byte)*1099511628211)&0xffffffffffffffff
    return f'{value:016x}'
batches=[]
for batch in asset['batches']:
    if batch['finish'] in {'AircraftGlass','Light','Red'}:continue
    uv=[batch['uv'][i] for i in batch['indices']]
    assert len(uv)==len(batch['indices']) and all(0<=v<=1 for p in uv for v in p)
    batches.append({'finish':batch['finish'],'joint':batch['joint'],'geometryHash':signature(batch),
        'sourceVertexCount':len(batch['vertices']),'cornerUv':uv})
path=ROOT/'materials/vehicle_atlases/uv_mapping.json';data=json.loads(path.read_text())
data['models']['helicopter']={'label':'Apache — rebuilt AH-64-inspired airframe','batches':batches,
    'triangles':sum(len(b['cornerUv'])//3 for b in batches),'revision':2,'source':'assets/aircraft/apache/apache.blend'}
path.write_text(json.dumps(data,separators=(',',':')))
print('APACHE_RUNTIME_UVS_REGISTERED',len(batches),data['models']['helicopter']['triangles'],flush=True)
