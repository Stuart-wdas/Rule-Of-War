"""Package actual native material pixels into ready-to-use flat PBR PNG maps."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
import hashlib, json, zipfile
import numpy as np

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / '.data/material-export/raw'
DEST = ROOT / 'exports/vehicle_material_maps_2026-10-26'
LABELS = {
    'armour_green': 'Green armour', 'armour_sand': 'Sand armour',
    'steel': 'Exposed steel', 'dark_fittings': 'Dark fittings',
    'rubber': 'Rubber / track pads', 'canvas': 'Canvas / stowage',
    'brass': 'Brass', 'rust': 'Rust', 'aircraft_grey': 'Grey aircraft paint',
    'aircraft_sand': 'Sand aircraft paint', 'helicopter_green': 'Green helicopter',
    'helicopter_sand': 'Sand helicopter', 'a10_livery': 'A-10 livery atlas',
    'vehicle_glass': 'Vehicle glass', 'cockpit_glass': 'Cockpit glass', 'lights': 'Lights',
}

def decode_srgb(a):
    return np.where(a <= .04045, a / 12.92, ((a + .055) / 1.055) ** 2.4)

def encode_srgb(a):
    return np.where(a <= .0031308, a * 12.92, 1.055 * np.maximum(a, 0) ** (1 / 2.4) - .055)

def byte(a):
    return np.rint(np.clip(a, 0, 1) * 255).astype(np.uint8)

def save_channel(a, path):
    Image.fromarray(byte(a)).save(path)

if __name__ == '__main__':
    manifest = json.loads((SOURCE / 'runtime_materials.json').read_text())
    DEST.mkdir(parents=True, exist_ok=True)
    exported = []
    for material in manifest['sets']:
        name = material['name']; maps = material['maps']; source = SOURCE / name
        folder = DEST / name; folder.mkdir(exist_ok=True)
        def source_image(map_name):
            return Image.open(source / maps[map_name]['file']).convert('RGB')
        albedo = source_image('albedo') if 'albedo' in maps else Image.new('RGB', (64, 64), 'white')
        tint = np.asarray(material['baseColorLinear'][:3], dtype=np.float32)
        coloured = encode_srgb(decode_srgb(np.asarray(albedo, dtype=np.float32) / 255) * tint)
        Image.fromarray(byte(coloured)).save(folder / 'albedo.png')
        normal = source_image('normal') if 'normal' in maps else Image.new('RGB', (64, 64), (128, 128, 255))
        normal.save(folder / 'normal.png')
        if 'orm' in maps:
            raw = np.asarray(source_image('orm'), dtype=np.float32) / 255
            roughness = raw[:, :, 1] * material['roughnessFactor']
            metalness = raw[:, :, 2] * material['metallicFactor']
        else:
            dims = source_image('occlusion').size if 'occlusion' in maps else (64, 64)
            roughness = np.full((dims[1], dims[0]), material['roughnessFactor'])
            metalness = np.full_like(roughness, material['metallicFactor'])
        dims = (roughness.shape[1], roughness.shape[0])
        ao_image = source_image('occlusion').getchannel('R') if 'occlusion' in maps else Image.new('L', dims, 255)
        ao = np.asarray(ao_image.resize(dims, Image.Resampling.BILINEAR), dtype=np.float32) / 255
        save_channel(roughness, folder / 'roughness.png')
        save_channel(metalness, folder / 'metalness.png')
        save_channel(ao, folder / 'ao.png')
        Image.fromarray(byte(np.stack([ao, roughness, metalness], axis=2))).save(folder / 'orm.png')
        entry = {**material, 'label': LABELS[name], 'factorsAlreadyBaked': True,
                 'importBaseColor': [1, 1, 1, 1], 'importRoughnessFactor': 1,
                 'importMetallicFactor': 1, 'normalConvention': 'OpenGL +Y',
                 'modelWeatheringBaked': False, 'files': {}}
        entry['sourceMaps'] = entry.pop('maps')
        for path in sorted(folder.glob('*.png')):
            with Image.open(path) as image:
                assert image.format == 'PNG'
                image.load()
                entry['files'][path.name] = {'size': list(image.size), 'mode': image.mode,
                    'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
        orm = np.asarray(Image.open(folder / 'orm.png'))
        for i, filename in enumerate(['ao.png', 'roughness.png', 'metalness.png']):
            assert np.array_equal(orm[:, :, i], np.asarray(Image.open(folder / filename)))
        assert np.array_equal(np.asarray(normal), np.asarray(Image.open(folder / 'normal.png')))
        (folder / 'material.json').write_text(json.dumps(entry, indent=2))
        exported.append(entry)
    font_path = Path('C:/Windows/Fonts/segoeui.ttf')
    font = ImageFont.truetype(str(font_path), 18) if font_path.exists() else ImageFont.load_default()
    preview = Image.new('RGB', (1120, 1040), (24, 28, 27)); draw = ImageDraw.Draw(preview)
    for i, entry in enumerate(exported):
        x = (i % 4) * 280; y = (i // 4) * 260
        image = Image.open(DEST / entry['name'] / 'albedo.png').convert('RGB')
        image.thumbnail((260, 220), Image.Resampling.LANCZOS)
        # Preview crops retain each source's proportions and show constant swatches clearly.
        if image.width < 220 and image.height < 220:
            image = image.resize((220, 220), Image.Resampling.NEAREST)
        preview.paste(image, (x + (280 - image.width) // 2, y + 32))
        draw.text((x + 10, y + 6), entry['label'], font=font, fill=(235, 232, 217))
    preview.save(DEST / 'overview.png')
    channels = Image.new('RGB', (1320, 540), (24, 28, 27)); draw = ImageDraw.Draw(channels)
    files = ['albedo.png', 'normal.png', 'roughness.png', 'metalness.png', 'ao.png', 'orm.png']
    for row, name in enumerate(['armour_green', 'steel']):
        for col, filename in enumerate(files):
            x = col * 220; y = row * 270
            image = Image.open(DEST / name / filename).convert('RGB').resize((208, 208), Image.Resampling.LANCZOS)
            channels.paste(image, (x + 6, y + 52))
            draw.text((x + 6, y + 4), LABELS[name], font=font, fill='white')
            draw.text((x + 6, y + 27), filename, font=font, fill=(185, 190, 180))
    channels.save(DEST / 'map_channels.png')
    readme = '''VEHICLE MATERIAL MAPS — 26 October 2026

These are the current game's flat PBR material maps, exported from the native
material generators and supplied texture files. No new AI interpretation or
upscaling was used. Individual maps retain their native resolutions. Materials
which are constant colours in the game use 64 x 64 swatches.

Each material folder contains:
  albedo.png     Colour in sRGB, with the game's material tint already included.
  normal.png     Tangent-space normal, OpenGL +Y. Load as Non-Color/linear data.
  roughness.png  Linear greyscale: white = rough; black = smooth.
  metalness.png  Linear greyscale: white = metal; black = dielectric.
  ao.png         Linear ambient occlusion: white = unoccluded.
  orm.png        Packed RGB: R = AO, G = roughness, B = metalness.
  material.json  Source values, UV transform, reflectance and image checksums.

Material colour, roughness and metallic factors have already been applied to
the PNGs. Use white base colour and roughness/metallic multipliers of 1 when
importing these processed maps. Normal/roughness/metalness/AO must not be read
as sRGB. Reflectance remains a material setting, recorded in material.json.
Use the recorded UV transform for the same tiling scale as the current game.

IMPORTANT: These are reusable material tiles, not new per-vehicle UV unwraps.
The A-10 livery is the existing aircraft atlas. The recent lower-hull dust,
mud, vertical grime streaks, geometric edge chips and per-mesh contact shadows
are applied using model position, vertex colours and the runtime shader. They
are NOT baked into these base tiles. Full weathered UV atlases would require
a separate bake for each model. The 'rust' map is the game's existing rust
finish, not a newly authored rust photograph. Glass/lights are opaque material
swatches as used by the game; these files do not add physical transparency or
emission that the runtime material does not have.

Source images are in materials/; generated camouflage and fallback relief come
from native/src/surfaces.rs and native/src/surface_wear.rs. Native extraction:
  native/target/debug/rule-of-war.exe --export-vehicle-materials <directory>
Packaging tool: tools/package_vehicle_materials.py.

overview.png shows every albedo. map_channels.png explains the six map files.
'''
    (DEST / 'README.txt').write_text(readme, encoding='utf-8')
    (DEST / 'manifest.json').write_text(json.dumps({'date': '2026-10-26', 'sets': exported,
        'verification': 'All PNGs decoded; packed ORM channels checked pixel-for-pixel; normal pixels preserved.'}, indent=2))
    archive = DEST.with_suffix('.zip')
    with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as z:
        for path in sorted(DEST.rglob('*')):
            if path.is_file():
                z.write(path, path.relative_to(DEST.parent))
    with zipfile.ZipFile(archive) as z:
        assert z.testzip() is None
    print(json.dumps({'directory': str(DEST), 'archive': str(archive), 'materials': len(exported),
                      'maps': len(exported) * 6, 'zipMiB': round(archive.stat().st_size / 1048576, 2)}, indent=2))
