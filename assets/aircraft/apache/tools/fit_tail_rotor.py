"""Keep the scissor tail rotor within the existing airfield footprint, preserving UVs."""
from pathlib import Path
import json,math
OUT=Path(__file__).resolve().parents[1];path=OUT/'apache_geometry.json';data=json.loads(path.read_text())
assert not data.get('tailRotorFitApplied'),'Fit already applied'
pivot=(-.15,1.43,-3.28)
for name in ['ApacheTailRotor','ApacheTailRotorMetal']:
    mesh=data['meshes'][name]
    for p in mesh['vertices']:
        for axis in [1,2]:p[axis]=round(pivot[axis]+(p[axis]-pivot[axis])*.9,7)
    for n in mesh['normals']:
        n[1]/=.9;n[2]/=.9;length=math.sqrt(sum(v*v for v in n));n[:]=[round(v/length,7) for v in n]
data['tailRotorFitApplied']=True;path.write_text(json.dumps(data,separators=(',',':')))
print('TAIL_ROTOR_FIT_READY')
