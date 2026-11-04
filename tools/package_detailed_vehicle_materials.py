"""Package imagegen outputs without resampling or modifying their pixels."""
from pathlib import Path
from PIL import Image
import argparse, hashlib, html, json, shutil, zipfile

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'exports/vehicle_material_maps_2026-10-26'
DEST = ROOT / 'exports/vehicle_material_maps_detailed_2026-10-26'
CHANNELS = ['normal.png', 'roughness.png', 'metalness.png', 'ao.png', 'orm.png']

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--applied', action='store_true')
    applied = parser.parse_args().applied
    original = json.loads((SOURCE / 'manifest.json').read_text())
    prompts = json.loads((DEST / 'generation/final-prompts.json').read_text())
    provenance = {entry['name']: entry for entry in json.loads((DEST / 'generation/outputs.json').read_text())}
    assert len(prompts['prompts']) == len(original['sets']) == 16
    (DEST / 'originals').mkdir(exist_ok=True)
    entries = []; cards = []
    for source in original['sets']:
        name = source['name']; folder = DEST / name
        albedo = folder / 'albedo.png'
        with Image.open(albedo) as im:
            assert im.format == 'PNG'; im.load(); size = im.size
            assert min(size) >= 1024
            if 'A' in im.getbands():
                assert im.getchannel('A').getextrema() == (255, 255)
        with Image.open(SOURCE / name / 'albedo.png') as im:
            aspect = im.width / im.height
        assert abs(size[0] / size[1] - aspect) < .001, name
        shutil.copy2(SOURCE / name / 'albedo.png', DEST / 'originals' / f'{name}.png')
        refs = folder / 'reference_pbr'; refs.mkdir(exist_ok=True)
        for channel in CHANNELS:
            before = SOURCE / name / channel
            after = refs / channel
            shutil.copy2(before, after)
            assert before.read_bytes() == after.read_bytes()
        shutil.copy2(SOURCE / name / 'material.json', refs / 'original_material.json')
        sha = hashlib.sha256(albedo.read_bytes()).hexdigest()
        assert sha != source['files']['albedo.png']['sha256']
        entry = {'name': name, 'label': source['label'], 'albedo': f'{name}/albedo.png',
                 'size': list(size), 'sha256': sha, 'generation': provenance[name],
                 'mode': 'built-in image_gen edit', 'colourSpace': 'sRGB',
                 'baseColourMultiplier': [1, 1, 1, 1], 'appliedToGame': applied,
                 'companions': 'Unchanged original PBR maps in reference_pbr; new wear is not encoded in them.',
                 'notes': 'Generative edit; original palette/layout guided the edit. Seamlessness and exact atlas texel alignment are not certified.'}
        (folder / 'material.json').write_text(json.dumps(entry, indent=2))
        entries.append(entry)
        label = html.escape(source['label'])
        cards.append(f'''<article><h2>{label}</h2>
<div class="compare" style="aspect-ratio:{size[0]}/{size[1]}">
<img src="{name}/albedo.png" alt="Detailed {label}" loading="lazy">
<img class="original" src="originals/{name}.png" alt="Original {label}" loading="lazy">
<span class="before">ORIGINAL</span><span class="after">AI DETAIL</span><span class="line"></span></div>
<label class="slider">Compare<input type="range" min="0" max="100" value="50" aria-label="Compare {label}" oninput="compare(this)"></label>
<p><a href="{name}/albedo.png" target="_blank">Open detailed PNG</a><span>{size[0]} × {size[1]}</span></p></article>''')
    gallery = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Vehicle materials — surface detail</title><style>
*{box-sizing:border-box}body{margin:0;background:#141b19;color:#eee9dd;font:16px/1.5 system-ui,sans-serif}header{padding:36px clamp(20px,4vw,64px);border-bottom:1px solid #3e4a43;max-width:1800px;margin:auto}h1{font-size:clamp(28px,4vw,44px);line-height:1.1;font-weight:600;margin:0 0 14px}header p{max-width:950px;color:#b9c4bc}small{display:block;color:#a3b29f;letter-spacing:.12em;margin-bottom:14px}button{cursor:pointer;background:#cad8ab;color:#152016;border:0;border-radius:4px;padding:9px 16px;font:inherit;margin:8px 6px 0 0}main{max-width:1800px;margin:0 auto;padding:28px clamp(20px,4vw,64px);display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,340px),1fr));gap:24px;align-items:start}article{background:#202a25;border:1px solid #3b483f;border-radius:6px;overflow:hidden}h2{font-size:18px;font-weight:550;padding:14px 16px;margin:0}.compare{position:relative;background:#101512;overflow:hidden}.compare img{position:absolute;width:100%;height:100%;object-fit:fill}.original{clip-path:inset(0 50% 0 0)}.line{position:absolute;left:50%;height:100%;border-left:1px solid #f4edc3;pointer-events:none}.before,.after{position:absolute;bottom:10px;background:#101a16bd;color:#eee;padding:4px 7px;font-size:11px;letter-spacing:.1em}.before{left:10px}.after{right:10px}.slider{display:flex;gap:14px;align-items:center;padding:12px 16px;color:#c0cbbb;font-size:13px}input{flex:1;accent-color:#dce6b5}article p{display:flex;justify-content:space-between;padding:0 16px 14px;margin:0;font-size:13px;color:#aebbaa}a{color:#dce6b5}footer{padding:12px 4vw 40px;color:#9eafa2;max-width:1800px;margin:auto}</style>
<header><small>RULE OF WAR · MATERIAL REVIEW</small><h1>More surface character.</h1><p>Sixteen colour maps edited with built-in image generation. Drag each slider to compare the original with the detailed version. Open a PNG to inspect its full resolution.</p><p>This is an albedo detail pass. Original normal, roughness, metalness and AO maps are included as references; they have not been rebaked for the new wear. The game materials have not been replaced.</p><button onclick="all(100)">Originals</button><button onclick="all(50)">Compare</button><button onclick="all(0)">Detailed</button></header>
<main>''' + '\n'.join(cards) + '''</main><footer>Original maps remain intact. Prompts and output provenance are in generation/. Refer to README.txt before importing.</footer>
<script>function compare(input){let card=input.closest('article');let n=Number(input.value);card.querySelector('.original').style.clipPath=`inset(0 ${100-n}% 0 0)`;card.querySelector('.line').style.left=n+'%'}function all(value){document.querySelectorAll('input').forEach(input=>{input.value=value;compare(input)})}</script></html>'''
    if applied:
        gallery = gallery.replace('The game materials have not been replaced.',
            'These albedos are active in the game and training ground, with the existing runtime PBR response.')
    (DEST / 'comparison.html').write_text(gallery, encoding='utf-8')
    readme = '''VEHICLE MATERIALS — AI SURFACE DETAIL PASS — 26 October 2026

Sixteen albedo edits made using the built-in image_gen tool and the original
exported colour maps as edit targets. These PNG files are direct copies of the
generated outputs: no pixel processing, resizing, recolouring or retouching.

Open comparison.html in a browser for the before/after sliders. Each material
folder contains the new albedo.png and metadata. originals/ contains the source
albedos. generation/final-prompts.json contains every exact generation prompt;
generation/outputs.json records the native generation output paths.

The colour-map pass adds material-appropriate scratches, paint wear, dusty
handling, fine corrosion, rubber abrasion and fabric fibre variation. Glass
and lights use much lighter wear; the A-10 atlas preserves the visible original
panel/marking layout. All outputs were visually reviewed. Generative edits are
not pixel-identical UV remaps, and seamless tiling is not certified.

ALBEDO ONLY: the maps in each reference_pbr/ directory are unchanged copies of
the original normal, roughness, metalness, AO and packed ORM maps, with their
original material settings. They DO NOT encode the newly generated scratches,
chips and corrosion. They are supplied as references, not newly matched PBR
bakes. Albedo is sRGB; the reference data maps are Non-Color/linear.

The existing game's runtime dust, contact shading and geometric edge wear are
still separate. No game material files or shaders have been replaced in this
pass. A material/model preview in the game remains the next integration check.
'''
    if applied:
        readme = readme.replace('No game material files or shaders have been replaced in this\npass. A material/model preview in the game remains the next integration check.',
            'These albedos are active in the game and training ground through\nmaterials/vehicle_detail/. The runtime retains its existing normal/ORM maps,\nmetal response, contact shading and dynamic dirt. Original building materials\nare separate and unchanged. See integration-validation.json for render checks.')
    (DEST / 'README.txt').write_text(readme, encoding='utf-8')
    (DEST / 'manifest.json').write_text(json.dumps({'date': '2026-10-26', 'tool': 'built-in image_gen',
        'editedAlbedos': len(entries), 'unchangedPbrReferenceMaps': len(entries) * 5,
        'gameIntegration': applied, 'sets': entries}, indent=2))
    for path in DEST.rglob('*.png'):
        with Image.open(path) as im:
            im.load()
    archive = DEST.with_suffix('.zip')
    with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for path in sorted(DEST.rglob('*')):
            if path.is_file(): z.write(path, path.relative_to(DEST.parent))
    with zipfile.ZipFile(archive) as z:
        assert z.testzip() is None
    print(json.dumps({'albedos': len(entries), 'referenceMaps': len(entries) * 5,
        'gallery': str(DEST / 'comparison.html'), 'archive': str(archive),
        'zipMiB': round(archive.stat().st_size / 1048576, 2)}, indent=2))
