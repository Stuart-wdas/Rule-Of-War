"""Bake bind-pose contact visibility and bevel wear from exact exported kits.

Run with Blender Python, passing the runtime_kit.json path after --.
This runs offline; gameplay performs no raycasts or texture baking.
"""
from pathlib import Path
import sys,json,struct,math
from mathutils import Vector
from mathutils.bvhtree import BVHTree
ROOT=Path(__file__).resolve().parents[3]
source=Path(sys.argv[sys.argv.index('--')+1])
document=json.loads(source.read_text());output={};report={}
kinds=['medium','long','troopCarrier','xtralong','antiAir']
if '--only' in sys.argv:
    kinds=sys.argv[sys.argv.index('--only')+1:]
    target=ROOT/'assets/art_refresh/model_contacts.json'
    if target.exists():output=json.loads(target.read_text())
    target=ROOT/'assets/art_refresh/model-contact-validation.json'
    if target.exists():report=json.loads(target.read_text())
def signature(batch):
    h=14695981039346656037
    for field,fmt in [('vertices','<3f'),('normals','<3f')]:
        for value in batch[field]:
            for b in struct.pack(fmt,*value):h=((h^b)*1099511628211)&0xffffffffffffffff
    return f'{h:016x}'
for asset in document['assets']:
    if asset['name'] not in kinds:continue
    batches=asset['batches'];vertices=[];faces=[]
    for batch in batches:
        offset=len(vertices);vertices.extend(batch['vertices']);ids=batch['indices']
        faces.extend(tuple(offset+ids[i+j] for j in range(3)) for i in range(0,len(ids),3))
    bvh=BVHTree.FromPolygons(vertices,faces,all_triangles=True)
    memo={};groups=[];ao_values=[]
    for batch in batches:
        positions=batch['vertices'];normals=batch['normals'];ids=batch['indices'];edges=[0.]*len(positions)
        for i in range(0,len(ids),3):
            tri=ids[i:i+3];a,b,c=[Vector(positions[j]) for j in tri]
            cross=(b-a).cross(c-a);area=cross.length*.5
            if area<.004 and cross.length>1e-10:
                fn=cross.normalized();tilt=1-max(abs(v) for v in fn)
                wear=min(1,max(0,(.004-area)/.004))*max(0,tilt)**.45
                for j in tri:edges[j]=max(edges[j],wear)
        channels=[]
        for index,(p,n) in enumerate(zip(positions,normals)):
            key=tuple(round(v,5) for v in [*p,*n])
            if key not in memo:
                pos=Vector(p);normal=Vector(n).normalized()
                axis=Vector((1,0,0)) if abs(normal.y)>.95 else Vector((0,1,0))
                tangent=axis.cross(normal).normalized();bitangent=normal.cross(tangent)
                blocked=0.;samples=16;reach=.14
                for ray in range(samples):
                    u=(ray+.5)/samples;phi=ray*math.pi*(3-math.sqrt(5));r=math.sqrt(u)
                    direction=tangent*(r*math.cos(phi))+bitangent*(r*math.sin(phi))+normal*math.sqrt(1-u)
                    location,_,_,distance=bvh.ray_cast(pos+normal*.003,direction,reach)
                    if location is not None:blocked+=max(0,1-distance/reach)
                memo[key]=max(.38,1-blocked/samples)
            ao=memo[key];ao_values.append(ao)
            channels.append([round(ao,4),round(edges[index],4)])
        groups.append({'geometryHash':signature(batch),'channels':channels})
    output[asset['name']]=groups
    report[asset['name']]={'groups':len(groups),'vertices':len(ao_values),'minimumVisibility':min(ao_values),'meanVisibility':sum(ao_values)/len(ao_values)}
    print('BAKED_CONTACTS',asset['name'],report[asset['name']],flush=True)
out=ROOT/'assets/art_refresh/model_contacts.json'
out.write_text(json.dumps(output,separators=(',',':')))
(out.with_name('model-contact-validation.json')).write_text(json.dumps(report,indent=2))
print('MODEL_CONTACTS_SAVED',out,flush=True)
