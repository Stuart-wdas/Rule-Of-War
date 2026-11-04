"""Native validation of the rebuilt Apache, environment palette and other atlas regressions."""
from pathlib import Path
import json,argparse
import check_world_detail as qa
ROOT=Path(__file__).resolve().parents[1];qa.OUT=ROOT/'.data/render-checks/apache-foliage-v2';EXE=qa.OUT/'bin/rule-of-war.exe'
CASES={
    'apache-close':['--aircraft-parking','--apache-model','--apache-close','--graphics','high','--hide-ui'],
    'apache':['--aircraft-parking','--apache-model','--graphics','high','--hide-ui'],
    'apache-enemy':['--aircraft-parking','--apache-model','--apache-close','--model-enemy','--graphics','high','--hide-ui'],
    'apache-low':['--aircraft-parking','--apache-model','--apache-close','--graphics','low','--hide-ui'],
    'apache-flight':['--heli-combat','--graphics','high','--hide-ui'],
    'foliage-after':['--environment-town','--graphics','high','--hide-ui'],
    'training':['--training','--training-tools','--graphics','balanced'],
    'other-vehicles':['--models','--graphics','balanced','--hide-ui'],
}
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--only',nargs='+');args=parser.parse_args()
    path=qa.OUT/'verification.json';records=json.loads(path.read_text()) if path.exists() else {}
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        result=qa.run(name,EXE,flags);r=result['report']
        assert r['vehicleAtlasModels']==6
        if name!='foliage-after':assert r['vehicleAtlasBatches']>0
        assert r['worldLighting']['modelContactCacheMatches']>0
        records[name]=result;path.write_text(json.dumps(records,indent=2));print('VERIFIED APACHE / FOLIAGE',name,flush=True)
