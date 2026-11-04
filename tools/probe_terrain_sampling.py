"""Read-only material isolation: separate runtime folders and catalog copies.

PNG files are linked but NEVER modified. Only each private catalog's repeat
scales change, forcing selected material patterns down to their filtered mean.
The user's running game and the project's material catalog stay untouched.
"""
from pathlib import Path
import argparse,json,os,subprocess,shutil
from PIL import Image

ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'.data/render-checks/terrain-swirl'
PROBES={
    'flat-soil':['photo_shoulder_v1'],
    'flat-all':['earth_grass_olive_v2','earth_grass_stony_v2','photo_shoulder_v1','meadow_relief_v1','asphalt'],
}
def link_once(src,dst):
    if not Path(dst).exists():os.link(src,dst)
    return dst
def main():
    p=argparse.ArgumentParser();p.add_argument('--only',nargs='+');p.add_argument('--legacy',action='store_true');p.add_argument('--exe',type=Path,default=ROOT/'native/target/debug/rule-of-war.exe');args=p.parse_args()
    original=(ROOT/'materials/pbr/catalog.json').read_bytes()
    for name,materials in PROBES.items():
        if args.only and name not in args.only:continue
        runtime=OUT/'runtimes'/name
        runtime.mkdir(parents=True,exist_ok=True)
        assert runtime.resolve().is_relative_to(OUT.resolve())
        shutil.copytree(ROOT/'materials',runtime/'materials',copy_function=link_once,
            ignore=shutil.ignore_patterns('catalog.json'),dirs_exist_ok=True)
        assets=runtime/'assets'
        if not assets.exists():
            command=f"New-Item -ItemType Junction -Path '{assets}' -Target '{ROOT/'assets'}' | Out-Null"
            subprocess.run(['powershell','-NoProfile','-Command',command],check=True)
        catalog=json.loads(original)
        for material in materials:catalog['materials'][material]['repeat_m']=[.02,.02]
        (runtime/'materials/pbr/catalog.json').write_text(json.dumps(catalog))
        label=name+('-legacy' if args.legacy else '')
        png=OUT/f'{label}.png'
        with (OUT/f'{label}.stdout').open('w') as stdout,(OUT/f'{label}.stderr').open('w') as stderr:
            r=subprocess.run([str(args.exe),'--organic-map',
                '--graphics','high','--terrain-debug','1','--hide-ui','--offscreen',
                *(['--legacy-terrain-noise'] if args.legacy else []),'--smoke-test',str(png)],
                cwd=runtime,stdout=stdout,stderr=stderr,timeout=240,creationflags=subprocess.CREATE_NO_WINDOW)
        assert r.returncode==0
        errors=(OUT/f'{label}.stderr').read_text()
        assert not any(x in errors for x in ['panicked','ERROR','ShaderError','Validation Error']),errors[-5000:]
        assert Image.open(png).size==(1920,1080)
        report=json.loads(Path(str(png)+'.json').read_text())
        assert report['terrainReady'] and report['cameraPinned']
        assert (ROOT/'materials/pbr/catalog.json').read_bytes()==original
        assert report.get('legacyTerrainBlendNoise')==args.legacy
        print('VERIFIED',label,flush=True)
if __name__=='__main__':main()
