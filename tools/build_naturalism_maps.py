"""Compile registered generated height to shallow PBR data; preserve albedo bytes."""
from pathlib import Path
import hashlib, json
import numpy as np
from PIL import Image
ROOT=Path(__file__).resolve().parents[1]
SOURCE=ROOT/'materials/pbr/naturalism_v3/source'
OUT=ROOT/'assets/art_refresh/naturalism-v3'

def blur(data,sigma):
    offsets=np.arange(-max(1,int(sigma*3)),max(1,int(sigma*3))+1)
    weights=np.exp(-offsets**2/(2*sigma**2));weights/=weights.sum()
    for axis in [0,1]:data=sum(np.roll(data,int(i),axis)*w for i,w in zip(offsets,weights))
    return data

def main():
    path=ROOT/'materials/pbr/catalog.json';catalog=json.loads(path.read_text());records={}
    for name,label,repeat,depth in [('olive','natural_meadow_v3',16.,.055),('stony','natural_stony_v3',12.,.075),('shoulder','natural_shoulder_v3',8.,.04)]:
        albedo=SOURCE/f'{name}.png';height=Image.open(SOURCE/f'{name}_height.png').convert('L')
        assert height.size==Image.open(albedo).size
        h=blur(np.asarray(height,dtype=np.float32)/255.,.65)
        dx=(np.roll(h,-1,1)-np.roll(h,1,1))*depth*h.shape[1]/(2*repeat)
        dz=(np.roll(h,-1,0)-np.roll(h,1,0))*depth*h.shape[0]/(2*repeat)
        normal=np.stack((-dx,-dz,np.ones_like(h)),axis=2);normal/=np.linalg.norm(normal,axis=2,keepdims=True)
        rgba=np.full((*h.shape,4),255,dtype=np.uint8);rgba[:,:,:3]=np.round((normal*.5+.5)*255).astype(np.uint8)
        Image.fromarray(rgba).save(ROOT/f'materials/pbr/{label}_normal.png')
        cavity=np.clip(blur(h,6.)-h,0,.4);orm=np.full_like(rgba,255)
        orm[:,:,0]=np.round(255*np.clip(1-cavity*.8,.76,1)).astype(np.uint8)
        orm[:,:,1]=np.round(255*np.clip(.96-h*.15,.81,.96)).astype(np.uint8);orm[:,:,2]=0
        Image.fromarray(orm).save(ROOT/f'materials/pbr/{label}_orm.png')
        (ROOT/f'materials/pbr/{label}_albedo.png').write_bytes(albedo.read_bytes())
        rgb=np.asarray(Image.open(albedo).convert('RGB'),dtype=np.float64)/255.
        linear=np.where(rgb<=.04045,rgb/12.92,((rgb+.055)/1.055)**2.4)
        record={'repeat_m':[repeat,repeat],'size':list(height.size),'source':'OpenAI built-in image generation',
            'provenance':'assets/art_refresh/naturalism-v3/generation.json',
            'normal':'OpenGL +Y, registered approximate companion height with periodic filtered physical gradients',
            'normal_height_range_m':depth,'albedo_linear_mean_rgb':linear.mean(axis=(0,1)).tolist(),
            'linear_reflectance_gain':{'olive':.30,'stony':.34,'shoulder':.90}[name],
            'orm':'R=restrained cavity occlusion G=varied matte roughness B=nonmetal',
            'usage':'Ordinary matches, training and terrain demos; concrete/asphalt retain their own materials.',
            'runtime_sha256':{s:hashlib.sha256((ROOT/f'materials/pbr/{label}_{s}.png').read_bytes()).hexdigest() for s in ['albedo','normal','orm']}}
        catalog['materials'][label]=record;records[label]=record
    path.write_text(json.dumps(catalog,indent=2)+'\n');(OUT/'materials.json').write_text(json.dumps(records,indent=2)+'\n')
    print(json.dumps(records,indent=2))
if __name__=='__main__':main()
