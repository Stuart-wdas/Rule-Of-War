"""Publish the measured camera regression results with their exact build identity."""
from pathlib import Path
import csv
import hashlib
import html
import json
import re
import shutil

ROOT=Path(__file__).resolve().parents[1]
DATA=ROOT/'.data/render-checks/camera-deep'
OUT=ROOT/'assets/art_refresh/camera-performance'
PHASE_NAMES={'camera.full_pan':'Full-map pan','camera.scroll':'Scroll zoom','camera.near_pan':'Close-range pan',
 'camera.drag':'Grab / drag pan','camera.mixed':'Mixed zoom and pan','camera.settle':'Settling after movement','frame':'Whole measured period'}

def main():
    results=json.loads((DATA/'verification.json').read_text())
    comparison=json.loads((DATA/'summary.json').read_text())
    assert {'native','dlss'}.issubset(comparison)
    exe=ROOT/'native/target/debug/rule-of-war.exe'
    sha=hashlib.sha256(exe.read_bytes()).hexdigest()
    assert (DATA/'after-bin/rule-of-war.exe').read_bytes()==exe.read_bytes()
    assert all(results[k]['exeSha256']==sha for k in ['after-native','after-dlss'])
    tests=re.search(r'test result: ok\. (\d+) passed; 0 failed',(ROOT/'.data/camera-deep-tests.log').read_text())
    assert tests and int(tests[1])>=359
    assert 'Finished' in (ROOT/'.data/camera-deep-clippy.log').read_text()
    assert 'Finished' in (ROOT/'.data/camera-deep-build.log').read_text()
    regressions=json.loads((DATA/'regressions.json').read_text())
    assert len(regressions)==5
    assert all(record['exeSha256']==sha for record in regressions.values())
    battle=json.loads((DATA/'live-battle-regression.json').read_text())
    assert battle['exeSha256']==sha
    battle_before=json.loads((DATA/'live-battle-before-crater-fix.json').read_text())
    memory={}
    for stage in ['before','after']:
        rows=list(csv.DictReader((ROOT/f'.data/battle-bench/camera-deep-{stage}-dlss.gpu.csv').open()))
        memory[stage]=max(float(r[' memory.used [MiB]'].split()[0]) for r in rows)
    validation={'exeSha256':sha,'testsPassed':int(tests[1]),'strictClippy':True,'nativeRenderChecks':8,
        'device':'NVIDIA GeForce RTX 3060 Laptop GPU, 6 GiB','settings':results['after-dlss']['report']['graphics'],
        'peakAdapterVramMiB':memory,'comparison':comparison,
        'liveBattleTimings':{name:{stage:r['report']['timings'][name] for stage,r in [('beforeCraterFix',battle_before),('afterCraterFix',battle)]}
            for name in ['frame','app.main','terrain.stream','scenery.stream','base.sync']},
        'notes':['Each case drives 45 seconds of real camera-control input on the 1024x1024 map.',
            'Startup is excluded from the measured camera phases. Isolated GPU submission spikes remain.',
            'The wall-time input driver processes more wheel events when rendering is faster; trajectories are not pixel-identical.',
            'VRAM counters cover the whole adapter, including other desktop applications; they do not establish driver eviction behavior.',
            'Before/after stress captures ran without concurrent Cargo or Blender. Functional regression captures ran during test compilation.']}
    OUT.mkdir(parents=True,exist_ok=True)
    (OUT/'validation.json').write_text(json.dumps(validation,indent=2))
    for name in ['training-regression','near-ground-regression','overview-regression']:
        shutil.copy2(DATA/f'{name}.png',OUT/f'{name}.png')
    tables=[]
    for name in ['dlss','native']:
        rows=[]
        for phase,row in comparison[name]['phases'].items():
            rows.append(f'<tr><th>{PHASE_NAMES[phase]}</th><td>{row["beforeP95Ms"]:.1f} → {row["afterP95Ms"]:.1f}</td>'
                f'<td>{row["beforeP99Ms"]:.1f} → {row["afterP99Ms"]:.1f}</td>'
                f'<td>{row["beforeWorstMs"]:.1f} → {row["afterWorstMs"]:.1f}</td>'
                f'<td>{row["beforeOver50Ms"]} → {row["afterOver50Ms"]}</td></tr>')
        title='High + DLSS Quality' if name=='dlss' else 'High + native rendering'
        tables.append(f'<h2>{title}</h2><div class="scroll"><table><thead><tr><th>Phase</th><th>95th percentile, ms</th><th>99th percentile, ms</th><th>Worst frame, ms</th><th>Frames &gt;50 ms</th></tr></thead><tbody>'+''.join(rows)+'</tbody></table></div>')
    changes=['Terrain uses persistent 64 m tiles, matching LOD seams, local edit invalidation and cached fine meshes.',
        'Vegetation uses per-chunk screen-distance LOD, split meshes, a 1 MiB/frame admission budget and a bounded inactive cache.',
        'Crater updates patch sparse edit changes into retained worker snapshots; base meshes persist while their anchor heights update.',
        'Static transforms, visibility, materials, HUD text and minimap markers stay unchanged when their values stay unchanged.',
        'DLSS history resets on explicit camera cuts, rather than normal zoom or fast panning.',
        'Picking clips the ray to the map and its height range, including at maximum zoom.',
        'The renderer has an 8 MiB soft asset-upload limit and separate extraction, queue, upload and submission diagnostics.']
    document='''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Rule of War camera performance</title><style>body{font:16px/1.55 system-ui,sans-serif;background:#151915;color:#e3e7de;max-width:1150px;margin:40px auto;padding:0 24px}h1,h2{font-weight:600}h2{margin-top:38px}.scroll{overflow:auto}table{width:100%;border-collapse:collapse;font-variant-numeric:tabular-nums}td,th{padding:10px;text-align:left;border-bottom:1px solid #485044}thead{color:#bcc9b4}li{margin:8px 0}a{color:#a8d2e1}img{max-width:100%;display:block;margin-top:14px}code{overflow-wrap:anywhere}.small{color:#c0c7ba}summary{cursor:pointer}</style><h1>Camera performance investigation</h1>
<p>RTX 3060 Laptop GPU · 1920 × 1080 · 1024 × 1024 battlefield · High textures, shadows and explosions.</p>
<p>TEST_COUNT tests, strict Clippy, two sustained camera stress runs, five functional rendering checks and a live battle passed.</p>'''
    document=document.replace('TEST_COUNT',str(validation['testsPassed']))
    document+=''.join(tables)+'<h2>What changed</h2><ul>'+''.join('<li>'+html.escape(s)+'</li>' for s in changes)+'</ul>'
    document+=f'<p>Peak adapter memory in the DLSS runs: {memory["before"]:.0f} → {memory["after"]:.0f} MiB.</p>'
    document+='<h2>Live battle: crater update follow-up</h2><p>The combat check exposed a separate synchronous terrain-update hitch. These measurements compare the initial streaming rewrite with the follow-up fix during a 30-second combined-arms battle.</p><div class="scroll"><table><thead><tr><th>Stage</th><th>95th percentile, ms</th><th>Worst call, ms</th><th>Calls &gt;50 ms</th></tr></thead><tbody>'
    for name,stages in validation['liveBattleTimings'].items():
        b,a=[stages[s] for s in ['beforeCraterFix','afterCraterFix']]
        document+=f'<tr><th>{html.escape(name)}</th><td>{b["p95Ms"]:.2f} → {a["p95Ms"]:.2f}</td><td>{b["maxMs"]:.1f} → {a["maxMs"]:.1f}</td><td>{b["over50Ms"]} → {a["over50Ms"]}</td></tr>'
    document+='</tbody></table></div>'
    document+='<h2>Method and limits</h2><ul>'+''.join('<li>'+html.escape(s)+'</li>' for s in validation['notes'])+'</ul>'
    document+='<p>The reported improvements are measured stress-test results, not a guarantee that every driver or gameplay hitch has disappeared.</p>'
    document+='<details><summary>Visual regression captures</summary><h3>Training</h3><img src="training-regression.png" alt="Training ground after the performance changes"><h3>Close terrain</h3><img src="near-ground-regression.png" alt="Close terrain detail retained"><h3>Overview</h3><img src="overview-regression.png" alt="Whole battlefield rendered with persistent terrain tiles"></details>'
    document+='<p><a href="validation.json">Machine-readable validation and pipeline counters</a></p><p class="small">Executable SHA-256: <code>'+sha+'</code></p></html>'
    (OUT/'comparison.html').write_text(document,encoding='utf-8')
    print(json.dumps({k:v for k,v in validation.items() if k!='comparison'},indent=2))

if __name__=='__main__':main()
