"""Package larger-wear albedos and matching native screenshots without editing pixels."""
from pathlib import Path
from PIL import Image
import hashlib, html, json, shutil, zipfile

ROOT = Path(__file__).resolve().parents[1]
PACK = ROOT / 'exports/vehicle_material_maps_large_wear_2026-10-26'
CHECKS = ROOT / '.data/render-checks/larger-wear'

if __name__ == '__main__':
    generation = json.loads((PACK / 'generation/prompts.json').read_text())
    revised = [o['name'] for o in generation['outputs']]
    assert len(revised) == 9
    cards = []; entries = []
    def card(label, before, after, aspect):
        return f'''<article><h2>{html.escape(label)}</h2><div class="compare" style="aspect-ratio:{aspect}"><img src="{after}" alt="Larger wear"><img class="old" src="{before}" alt="Previous wear"><span class="left">PREVIOUS</span><span class="right">LARGER WEAR</span><i></i></div><input aria-label="Compare {html.escape(label)}" type="range" min="0" max="100" value="50" oninput="update(this)"><a href="{after}" target="_blank">Open full-resolution image</a></article>'''
    (PACK / 'in_game').mkdir(exist_ok=True)
    for name, label in [('tank', 'Tank close view'), ('normal-view', 'Gameplay-distance view')]:
        for version in ['before', 'after']:
            shutil.copy2(CHECKS / f'{version}-{name}.png', PACK / 'in_game' / f'{version}-{name}.png')
        cards.append(card(label, f'in_game/before-{name}.png', f'in_game/after-{name}.png', '16/9'))
    for name in revised:
        image = PACK / name / 'albedo.png'; live = ROOT / 'materials/vehicle_detail' / f'{name}_albedo.png'
        assert live.read_bytes() == image.read_bytes()
        with Image.open(image) as im:
            im.load(); size = list(im.size)
        entries.append({'name': name, 'size': size, 'sha256': hashlib.sha256(image.read_bytes()).hexdigest(), 'appliedToGame': True})
        cards.append(card(name.replace('_', ' ').title(), f'before/{name}_albedo.png', f'{name}/albedo.png', f'{size[0]}/{size[1]}'))
    (PACK / 'comparison.html').write_text('''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Larger vehicle wear</title><style>*{box-sizing:border-box}body{margin:0;background:#161c18;color:#eae6d9;font:16px/1.5 system-ui,sans-serif}header{padding:30px 4vw;border-bottom:1px solid #465143}h1{font-size:34px;margin:0 0 12px}header p{max-width:950px;color:#b4c1b1}main{padding:25px 4vw;display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,500px),1fr));gap:24px;align-items:start}article{background:#252f27;border:1px solid #465143;border-radius:6px;overflow:hidden}h2{font-size:18px;font-weight:500;padding:12px;margin:0}.compare{position:relative;overflow:hidden;background:#0b100c}.compare img{position:absolute;width:100%;height:100%;object-fit:fill}.old{clip-path:inset(0 50% 0 0)}i{position:absolute;left:50%;height:100%;border-left:1px solid #f0e8b7}.left,.right{position:absolute;bottom:10px;font-size:11px;background:#162318bd;padding:4px 8px;letter-spacing:.1em}.left{left:10px}.right{right:10px}input{width:calc(100% - 30px);margin:15px;accent-color:#b9c99a}a{display:block;color:#d0e0ae;margin:0 15px 16px}button{padding:8px 14px;margin:0 8px 0 0;background:#c5d5ab;border:0;border-radius:4px;font:inherit;cursor:pointer}</style><header><h1>Wear that reads from the game camera.</h1><p>Larger chipped paint islands, broader rubbed areas and wider scrape groups on armour, fittings and aircraft. Camouflage and UV scale remain the same. The first two comparisons are native game captures; the remaining comparisons show the revised colour maps.</p><p>Active in the battlefield and training ground. Existing normal/ORM maps, metal sheen and runtime dust remain in use. Fine fabric, rubber and glazing materials retain their previous treatment.</p><button onclick="all(100)">Previous</button><button onclick="all(50)">Compare</button><button onclick="all(0)">Larger wear</button></header><main>''' + '\n'.join(cards) + '''</main><script>function update(input){let p=input.closest('article'),n=Number(input.value);p.querySelector('.old').style.clipPath=`inset(0 ${100-n}% 0 0)`;p.querySelector('i').style.left=n+'%'}function all(v){document.querySelectorAll('input').forEach(i=>{i.value=v;update(i)})}</script></html>''', encoding='utf-8')
    (PACK / 'manifest.json').write_text(json.dumps({'date': '2026-10-26', 'tool': 'built-in image_gen', 'revisedMaps': entries, 'runtimeRevision': 2, 'otherMaterialMaps': 'Retained; these edits are albedo only.'}, indent=2))
    (PACK / 'README.txt').write_text('''LARGER VEHICLE WEAR — ACTIVE RUNTIME REVISION 2

Nine albedos revised with built-in image generation: green/sand armour, steel,
dark fittings, four aircraft paint colours and the A-10 livery atlas. Main paint
chips and scraped areas were redesigned several times larger than the prior
micro-wear. Camouflage/UV scale and the aircraft atlas layout were preserved.

The images are direct copies of generated PNGs, with no resizing or retouching.
The previous active maps are saved under before/. Other materials retain their
previous finish. Current normals, ORM, metal sheen and runtime dust remain in
use; the large paint chips are colour detail, not a new physical height bake.

Open comparison.html for in-game and flat-map before/after sliders. The wider
view uses the native model showcase at zoom 48; the tank close view uses zoom
18. They use the same fixed camera and lighting in each comparison pair.
generation/prompts.json contains all exact prompts and output provenance.
validation.json records the native render checks and tests.
''', encoding='utf-8')
    for name in ['tank', 'normal-view']:
        shutil.copy2(CHECKS / f'after-{name}.png', ROOT / 'assets/art_refresh' / f'large-wear-{name}.png')
    archive = PACK.with_suffix('.zip')
    with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for path in sorted(PACK.rglob('*')):
            if path.is_file(): z.write(path, path.relative_to(PACK.parent))
    with zipfile.ZipFile(archive) as z: assert z.testzip() is None
    print(json.dumps({'revisedAlbedos': len(entries), 'gallery': str(PACK / 'comparison.html'), 'archive': str(archive), 'zipMiB': round(archive.stat().st_size / 1048576, 2)}, indent=2))
