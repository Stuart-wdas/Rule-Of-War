"""Verify the actual GPU pressure-ring location at native and DLSS resolutions."""
from pathlib import Path
import argparse, json, shutil
import numpy as np
from PIL import Image
import check_world_detail as qa

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.data/render-checks/pressure-alignment'

def check_mask(path, report):
    pixels = np.asarray(Image.open(path).convert('RGB')).astype(float)
    # AgX and DLSS soften magenta; retain its chromatic signature.
    mask = (pixels[:,:,0] > pixels[:,:,1] * 1.35 + 20) & (pixels[:,:,2] > pixels[:,:,1] * 1.35 + 20)
    mask[:140] = False
    mask[970:] = False
    ys, xs = np.nonzero(mask)
    assert len(xs) > 150, 'Pressure ring was not visible on the GPU'
    left, right = np.percentile(xs, [0.5, 99.5])
    cx, cy = report['pressureCentersScreen'][0]
    # The preview's impact is centred horizontally. Terrain can clip the
    # bottom of a sphere, but both side arcs must remain centred on impact.
    midpoint = (left + right) / 2
    assert abs(midpoint - cx) < 12, (midpoint, cx, left, right)
    assert left < cx - 25 and right > cx + 25, (left, cx, right)
    assert ys.min() < cy - 25, (ys.min(), cy)
    return dict(maskPixels=len(xs), horizontalCenter=float(midpoint), impactCenter=[cx,cy],
                centerError=float(abs(midpoint-cx)), bounds=[float(left),int(ys.min()),float(right),int(ys.max())])

if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--only', nargs='+', choices=['native','quality','performance','appearance'])
    args = p.parse_args()
    qa.OUT = OUT
    assert qa.CURRENT.stat().st_mtime >= max(p.stat().st_mtime for p in (ROOT/'native/src').glob('shockwave.*')), 'Build the current renderer before running GPU checks'
    (OUT/'bin').mkdir(parents=True, exist_ok=True)
    exe = OUT/'bin/rule-of-war.exe'
    shutil.copy2(qa.CURRENT, exe)
    dll = qa.CURRENT.parent/'nvngx_dlss.dll'
    if dll.exists(): shutil.copy2(dll, exe.parent/dll.name)
    record = OUT/'verification.json'
    results = json.loads(record.read_text()) if record.exists() else {}
    for mode in ['native','quality','performance','appearance']:
        if args.only and mode not in args.only: continue
        flags = ['--visual-demo','--demo-effect','1','--demo-scale','1',
                 '--demo-close','--demo-age','.22','--graphics','balanced',
                 '--upscaler','quality' if mode=='appearance' else mode]
        if mode != 'appearance': flags += ['--shockwave-mask']
        name = 'aligned-'+mode
        result = qa.run(name, exe, flags)
        report = result['report']
        assert report['pressureShells']==1, report['pressureShells']
        assert report['pressureWavesSpawned']==1
        assert len(report['pressureCentersScreen'])==1
        if mode != 'native': assert report['dlssSupported'], 'GPU does not support DLSS'
        if mode != 'appearance': result['alignment'] = check_mask(OUT/(name+'.png'), report)
        results[mode]=result
        record.write_text(json.dumps(results, indent=2))
        print('VERIFIED PRESSURE ALIGNMENT',mode,result.get('alignment',{}),flush=True)
