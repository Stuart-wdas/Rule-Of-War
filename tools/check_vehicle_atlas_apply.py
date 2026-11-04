"""Matched native GPU proofs for the six atlases, factions and quality presets."""
from pathlib import Path
import argparse,json,shutil
import check_world_detail as qa
ROOT=Path(__file__).resolve().parents[1];qa.OUT=ROOT/'.data/render-checks/vehicle-atlas-apply'
BIN=qa.OUT/'bin';EXE=BIN/'rule-of-war.exe'
CASES={
    'legacy-support':['--vehicle-model','long','--legacy-vehicle-atlases','--graphics','high','--hide-ui'],
    'support':['--vehicle-model','long','--graphics','high','--hide-ui'],
    'rocket':['--vehicle-model','xtralong','--graphics','high','--hide-ui'],
    'carrier':['--vehicle-model','troopCarrier','--graphics','high','--hide-ui'],
    'anti-air':['--vehicle-model','antiAir','--graphics','high','--hide-ui'],
    'apache':['--aircraft-parking','--apache-model','--graphics','high','--hide-ui'],
    'legacy-a10':['--aircraft-parking','--warthog-model','--legacy-vehicle-atlases','--graphics','high','--hide-ui'],
    'a10':['--aircraft-parking','--warthog-model','--graphics','high','--hide-ui'],
    'enemy-support':['--vehicle-model','long','--model-enemy','--graphics','high','--hide-ui'],
    'enemy-a10':['--aircraft-parking','--warthog-model','--model-enemy','--graphics','high','--hide-ui'],
    'low':['--vehicle-model','long','--graphics','low','--hide-ui'],
    'overview':['--models','--graphics','balanced','--hide-ui'],
    'training':['--training','--training-tools','--graphics','balanced'],
}
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--only',nargs='+');parser.add_argument('--snapshot-only',action='store_true');args=parser.parse_args()
    BIN.mkdir(parents=True,exist_ok=True)
    if not EXE.exists() or args.snapshot_only:
        shutil.copy2(ROOT/'native/target/debug/rule-of-war.exe',EXE)
        for dll in (ROOT/'native/target/debug').glob('*.dll'):shutil.copy2(dll,BIN/dll.name)
    if args.snapshot_only:print('Snapshot ready',EXE);raise SystemExit
    path=qa.OUT/'verification.json';records=json.loads(path.read_text()) if path.exists() else {}
    for name,flags in CASES.items():
        if args.only and name not in args.only:continue
        record=qa.run(name,EXE,flags);r=record['report']
        if name.startswith('legacy-'):assert r['vehicleAtlasModels']==0 and r['vehicleAtlasBatches']==0
        else:
            assert r['vehicleAtlasModels']==6
            if name!='training':assert r['vehicleAtlasBatches']>0
        if name in ['support','rocket','carrier','anti-air','enemy-support','low']:
            assert r['worldLighting']['modelContactCacheMatches']>0
        if name in ['apache','a10','enemy-a10']:assert len(r['aircraft'])==4
        records[name]=record;path.write_text(json.dumps(records,indent=2));print('VERIFIED ATLAS',name,flush=True)
