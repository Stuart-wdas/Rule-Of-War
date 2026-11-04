"""Isolate the colour, coverage and lighting contributions to terrain swirls."""
from pathlib import Path
import argparse,json,shutil
import check_world_detail as qa

ROOT=Path(__file__).resolve().parents[1]
qa.OUT=ROOT/'.data/render-checks/terrain-swirl'
qa.OUT.mkdir(parents=True,exist_ok=True)
CASES={
    'lit':['--organic-map','--graphics','high'],
    'colour':['--organic-map','--graphics','high','--terrain-debug','1'],
    'coverage':['--organic-map','--graphics','high','--terrain-debug','2'],
    'grass-colour':['--organic-map','--graphics','high','--terrain-debug','3'],
}
def main():
    p=argparse.ArgumentParser();p.add_argument('--only',nargs='+');args=p.parse_args()
    path=qa.OUT/'verification.json'
    records=json.loads(path.read_text()) if path.exists() else {}
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        records[name]=qa.run('swirl-'+name,qa.CURRENT,flags)
        path.write_text(json.dumps(records,indent=2))
        print('VERIFIED',name,flush=True)
if __name__=='__main__':main()
