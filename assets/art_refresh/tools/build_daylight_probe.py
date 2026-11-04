"""Offline linear-HDR sky/ground probe with GGX-prefiltered specular mips.

No sun disk: the game's directional light already supplies direct sunlight.
The diffuse cube contains cosine-weighted mean radiance (irradiance / pi).
"""
from pathlib import Path
import json,math
import numpy as np
ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'materials/lighting';OUT.mkdir(parents=True,exist_ok=True)
def unit(v):return v/np.maximum(np.linalg.norm(v,axis=-1,keepdims=True),1e-12)
def radiance(d):
    y=d[...,1:2]
    horizon=np.array([.58,.57,.51]);zenith=np.array([.24,.34,.49])
    sky=horizon+(zenith-horizon)*np.maximum(y,0)**.45
    ground=np.array([.135,.15,.095])+np.array([.18,.15,.135])*(1-np.maximum(-y,0))**5
    return np.where(y>=0,sky,ground)
def directions(face,size):
    a=(np.arange(size)+.5)/size*2-1;u,v=np.meshgrid(a,a);one=np.ones_like(u)
    faces=[(one,-v,-u),(-one,-v,u),(u,one,v),(u,-one,-v),(u,-v,one),(-u,-v,-one)]
    return unit(np.stack(faces[face],axis=-1))
def basis(n):
    up=np.broadcast_to([0.,1.,0.],n.shape).copy();up[np.abs(n[...,1])>.95]=[1.,0.,0.]
    x=unit(np.cross(up,n));return x,np.cross(n,x)
def filter_map(n,roughness,diffuse=False):
    if roughness==0 and not diffuse:return radiance(n)
    x,y=basis(n);out=np.zeros_like(n);weight=np.zeros_like(n[...,:1])
    samples=128 if diffuse else 64
    for i in range(samples):
        u=(i+.5)/samples;phi=i*math.pi*(3-math.sqrt(5))
        if diffuse:
            r=math.sqrt(u);c=math.sqrt(1-u)
            direction=x*r*math.cos(phi)+y*r*math.sin(phi)+n*c
            w=1.
        else:
            alpha=max(roughness**2,.001)
            c=math.sqrt((1-u)/(1+(alpha*alpha-1)*u));s=math.sqrt(max(0,1-c*c))
            h=x*s*math.cos(phi)+y*s*math.sin(phi)+n*c
            direction=unit(2*np.sum(n*h,axis=-1,keepdims=True)*h-n)
            w=np.maximum(np.sum(n*direction,axis=-1,keepdims=True),0)
        out+=radiance(direction)*w;weight+=w
    return out/np.maximum(weight,1e-8)
def encode(rgb):
    rgba=np.concatenate([rgb,np.ones_like(rgb[...,:1])],axis=-1)
    assert np.isfinite(rgba).all() and rgba.min()>=0
    return rgba.astype('<f2').tobytes()
spec=bytearray();diff=bytearray()
for face in range(6):
    for level in range(7):spec+=encode(filter_map(directions(face,64>>level),level/6))
    diff+=encode(filter_map(directions(face,8),1,True))
(OUT/'afternoon_specular.rgba16f').write_bytes(spec)
(OUT/'afternoon_diffuse.rgba16f').write_bytes(diff)
(OUT/'afternoon.json').write_text(json.dumps({'format':'RGBA16Float little endian, cube faces +X -X +Y -Y +Z -Z, layer-major','specularSize':64,'specularMips':7,'diffuseSize':8,'sunDisk':False,'specularBytes':len(spec),'diffuseBytes':len(diff)},indent=2))
print('Filtered daylight probe:',len(spec)+len(diff),'bytes')
