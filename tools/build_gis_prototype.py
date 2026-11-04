"""Build the isolated Hottot GIS map. Python 3 + Pillow + numpy; no API keys.

Run --fetch once to cache source data; subsequent builds are entirely offline.
Source geometries and tags are retained in sources/osm.json for audit/rebuild.
"""
from pathlib import Path
import argparse
import hashlib
import json
import math
import struct
import urllib.parse
import urllib.request

import numpy as np
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "prototypes/real-world-hottot"
SOURCE = OUT / "sources"
LAT, LON = 49.1515, -0.6495
COLS, ROWS, MARGIN = 2048, 1024, 2048
BOUNDS = [-MARGIN, COLS - 1 + MARGIN, -MARGIN, ROWS - 1 + MARGIN]
DEG_LAT = 111132.92 - 559.82 * math.cos(2 * math.radians(LAT)) + 1.175 * math.cos(4 * math.radians(LAT))
DEG_LON = 111412.84 * math.cos(math.radians(LAT)) - 93.5 * math.cos(3 * math.radians(LAT))
ZOOM = 12


def local(lat, lon):
    return [(lon - LON) * DEG_LON + COLS / 2, (LAT - lat) * DEG_LAT + ROWS / 2]


def geographic(x, z):
    return LAT - (z - ROWS / 2) / DEG_LAT, LON + (x - COLS / 2) / DEG_LON


def pixel(lat, lon):
    n = 256 * 2 ** ZOOM
    return (lon + 180) / 360 * n, (1 - math.asinh(math.tan(math.radians(lat))) / math.pi) / 2 * n


def request(url, data=None):
    req = urllib.request.Request(url, data=data, headers={"User-Agent": "RuleOfWar-GIS-prototype/1.0 (offline map authoring)"})
    with urllib.request.urlopen(req, timeout=55) as response:
        return response.read()


def fetch():
    SOURCE.mkdir(parents=True, exist_ok=True)
    # The source extract includes a 300 m collar for continuous map-edge features.
    south, west = geographic(-300, ROWS + 300)
    north, east = geographic(COLS + 300, -300)
    bbox = f"{south},{west},{north},{east}"
    query = f'[out:json][timeout:40];(way["building"]({bbox});way["highway"]({bbox});way["landuse"]({bbox});way["natural"]({bbox});way["waterway"]({bbox});way["barrier"="hedge"]({bbox});node["place"]({bbox}););out tags geom;'
    if not (SOURCE / "osm.json").exists():
        errors = []
        for endpoint in ["https://overpass-api.de/api/interpreter", "https://overpass.kumi.systems/api/interpreter"]:
            try:
                payload = request(endpoint, urllib.parse.urlencode({"data": query}).encode())
                parsed = json.loads(payload)
                if not parsed.get("elements") or parsed.get("remark"):
                    raise ValueError(f"Incomplete OSM response: {parsed.get('remark')}")
                (SOURCE / "osm.json").write_bytes(payload)
                (SOURCE / "osm-query.txt").write_text(query, encoding="utf-8")
                print(f"OSM: {len(parsed['elements'])} features", flush=True)
                break
            except Exception as error:
                errors.append(str(error))
                print(f"OSM endpoint unavailable: {error}", flush=True)
        else:
            raise RuntimeError("OSM download failed: " + "; ".join(errors))
    x0, y0 = pixel(*geographic(BOUNDS[0], BOUNDS[2]))
    x1, y1 = pixel(*geographic(BOUNDS[1], BOUNDS[3]))
    for y in range(int(y0 // 256), int(y1 // 256) + 1):
        for x in range(int(x0 // 256), int(x1 // 256) + 1):
            path = SOURCE / f"terrarium-{ZOOM}-{x}-{y}.png"
            if not path.exists():
                path.write_bytes(request(f"https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{ZOOM}/{x}/{y}.png"))
                print(f"Elevation tile {x}/{y}", flush=True)


def geometry(feature):
    return [local(p["lat"], p["lon"]) for p in feature.get("geometry", []) if "lat" in p]


def building(feature):
    points = np.asarray(geometry(feature), dtype=float)
    if len(points) < 4:
        return None
    points = points[:-1] if np.linalg.norm(points[0] - points[-1]) < .01 else points
    center = points.mean(axis=0)
    # Principal rectangle approximates orientation; all playable houses retain
    # RoW's 10 m plots, single storey, windows and destruction rules.
    edges = np.roll(points, -1, axis=0) - points
    edge = edges[np.argmax(np.sum(edges * edges, axis=1))]
    yaw = -math.atan2(edge[1], edge[0])
    if not (12 < center[0] < COLS - 12 and 12 < center[1] < ROWS - 12):
        return None
    return {"osm_id": feature["id"], "at": center.round(3).tolist(), "yaw": round(yaw, 6), "name": feature.get("tags", {}).get("name", ""), "footprint": np.round(points, 3).tolist()}


def build():
    data = json.loads((SOURCE / "osm.json").read_text(encoding="utf-8"))
    features = data["elements"]
    roads, buildings, areas, hedges = [], [], [], []
    widths = {"primary": 7., "secondary": 6., "tertiary": 5.5, "residential": 4.5, "unclassified": 4., "service": 3., "track": 2.8, "path": 1.5, "footway": 1.5, "living_street": 4.}
    for f in features:
        tags, points = f.get("tags", {}), geometry(f)
        if "building" in tags and tags["building"] != "no":
            b = building(f)
            if b:
                buildings.append(b)
        if tags.get("highway") in widths and len(points) >= 2:
            roads.append({"osm_id": f["id"], "name": tags.get("name", ""), "kind": tags["highway"], "width": widths[tags["highway"]], "paved": tags["highway"] not in ["track", "path", "footway"] and tags.get("surface") not in ["ground", "dirt", "gravel", "grass"], "points": points})
        kind = {"farmland": 1, "farmyard": 2, "meadow": 3, "grass": 3, "orchard": 3, "forest": 4, "residential": 5}.get(tags.get("landuse"), 0)
        kind = {"wood": 4, "water": 6, "scrub": 4, "grassland": 3}.get(tags.get("natural"), kind)
        if kind and len(points) >= 4 and np.linalg.norm(np.asarray(points[0]) - points[-1]) < .1:
            areas.append({"osm_id": f["id"], "kind": kind, "points": points})
        if tags.get("barrier") == "hedge" and len(points) >= 2:
            hedges.append({"osm_id": f["id"], "points": points})
    # One current house per real building, with close source footprints merged
    # only when native 10 m plots would physically overlap.
    accepted, omitted = [], []
    for b in sorted(buildings, key=lambda b: b["osm_id"]):
        if any(np.linalg.norm(np.asarray(b["at"]) - a["at"]) < 13.5 for a in accepted):
            omitted.append(b["osm_id"])
        else:
            accepted.append(b)
    # Spatial mask at 2 m. R=land cover; G=road exclusion; B=building exclusion.
    mask_step = 2
    mw, mh = math.ceil((BOUNDS[1] - BOUNDS[0]) / mask_step) + 1, math.ceil((BOUNDS[3] - BOUNDS[2]) / mask_step) + 1
    layers = [Image.new("L", (mw, mh)) for _ in range(3)]
    draw = [ImageDraw.Draw(layer) for layer in layers]
    def mp(p):
        return ((p[0] - BOUNDS[0]) / mask_step, (p[1] - BOUNDS[2]) / mask_step)
    for area in sorted(areas, key=lambda a: a["kind"]):
        draw[0].polygon([mp(p) for p in area["points"]], fill=area["kind"])
    for road in roads:
        draw[1].line([mp(p) for p in road["points"]], fill=255, width=math.ceil((road["width"] + 6) / mask_step), joint="curve")
    for b in accepted:
        x, y = mp(b["at"])
        draw[2].ellipse((x - 6, y - 6, x + 6, y + 6), fill=255)
    cover = np.asarray(layers[0])
    # Byte mask is deliberately header-free; dimensions/spacing live in map.json.
    np.stack([np.asarray(layer) for layer in layers], axis=-1).tofile(OUT / "cover.rgb")
    # Sample the actual DEM onto an 8 m runtime grid, keeping all geographic relief.
    step = 8
    w, h = math.ceil((BOUNDS[1] - BOUNDS[0]) / step) + 1, math.ceil((BOUNDS[3] - BOUNDS[2]) / step) + 1
    xs, zs = np.linspace(BOUNDS[0], BOUNDS[1], w), np.linspace(BOUNDS[2], BOUNDS[3], h)
    xx, zz = np.meshgrid(xs, zs)
    lat = LAT - (zz - ROWS / 2) / DEG_LAT
    lon = LON + (xx - COLS / 2) / DEG_LON
    n = 256 * 2 ** ZOOM
    px, py = (lon + 180) / 360 * n - .5, (1 - np.arcsinh(np.tan(np.radians(lat))) / math.pi) / 2 * n - .5
    # Pixel-centre convention plus a single mosaic makes tile seams continuous.
    tiles = {}
    for path in SOURCE.glob("terrarium-*.png"):
        _, _, tx, ty = path.stem.split("-")
        tiles[int(tx), int(ty)] = np.asarray(Image.open(path).convert("RGB"), dtype=float)
    minx, miny = min(k[0] for k in tiles), min(k[1] for k in tiles)
    maxx, maxy = max(k[0] for k in tiles), max(k[1] for k in tiles)
    mosaic = np.zeros(((maxy - miny + 1) * 256, (maxx - minx + 1) * 256), dtype=float)
    for (tx, ty), rgb in tiles.items():
        mosaic[(ty-miny)*256:(ty-miny+1)*256, (tx-minx)*256:(tx-minx+1)*256] = rgb[:,:,0]*256 + rgb[:,:,1] + rgb[:,:,2]/256 - 32768
    px, py = px - minx*256, py - miny*256
    ix, iy = np.floor(px).astype(int), np.floor(py).astype(int)
    if ix.min() < 0 or iy.min() < 0 or ix.max()+1 >= mosaic.shape[1] or iy.max()+1 >= mosaic.shape[0]:
        raise ValueError("Elevation tile collar is incomplete")
    u, v = px-ix, py-iy
    heights = (mosaic[iy, ix]*(1-u) + mosaic[iy, ix+1]*u)*(1-v) + (mosaic[iy+1, ix]*(1-u) + mosaic[iy+1, ix+1]*u)*v
    offset = math.floor(float(heights.min())) - 10
    scale = max(100, math.ceil(float(heights.max()) - offset) + 10)
    elev = ((heights-offset)/scale).astype("<f4")
    mx = np.clip(np.round((xx-BOUNDS[0])/mask_step).astype(int), 0, mw-1)
    mz = np.clip(np.round((zz-BOUNDS[2])/mask_step).astype(int), 0, mh-1)
    covers = cover[mz, mx]
    moisture = np.where(covers == 4, .72, np.where(covers == 3, .52, .35)).astype("<f4")
    kinds = np.where(covers == 6, 1, 4).astype("u1")
    count = w*h
    header = json.dumps({"version": 1, "seed": "hottot-real-world-v1", "width": w, "height": h, "bounds": dict(zip(["minX", "maxX", "minY", "maxY"], BOUNDS)), "config": {"maxElevation": scale}, "sections": [{"name": key, "byteLength": count*size} for key,size in [("elevation",4),("surfaceElevation",4),("moisture",4),("terrainTypeCodes",1)]]}, separators=(",", ":")).encode()
    (OUT / "hottot.terrain").write_bytes(b"ROWTERR1" + struct.pack("<I",len(header)) + header + elev.tobytes()*2 + moisture.tobytes() + kinds.tobytes())
    info = {"id": "gis-hottot", "title": "Hottot-les-Bagues / Normandy", "center_wgs84": [LAT,LON], "cols": COLS, "rows": ROWS, "scenery_margin": MARGIN, "bounds": BOUNDS, "projection": "Local WGS84 tangent approximation; x east, z south, metres", "vertical_offset_m": offset, "dem_grid": [w,h], "dem_source_zoom": ZOOM, "dem_nominal_tile_spacing_m": round(40075016.686*math.cos(math.radians(LAT))/(256*2**ZOOM),2), "mask": {"width":mw,"height":mh,"step":mask_step}, "roads":roads,"buildings":accepted,"omitted_close_buildings":omitted,"areas":areas,"hedges":hedges,"sources": {"osm": "https://www.openstreetmap.org/copyright", "elevation":"https://registry.opendata.aws/terrain-tiles/", "elevation_attribution":"https://github.com/tilezen/joerd/blob/master/docs/attribution.md"}, "source_hashes": {p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(SOURCE.iterdir()) if p.is_file()}}
    (OUT / "map.json").write_text(json.dumps(info,ensure_ascii=False,indent=2),encoding="utf-8")
    preview(info, heights, xs, zs)
    print(json.dumps({"buildings":len(accepted),"omitted_close_buildings":len(omitted),"roads":len(roads),"areas":len(areas),"hedges":len(hedges),"elevation_m":[round(float(heights.min()),2),round(float(heights.max()),2)],"output":str(OUT)},indent=2))


def preview(info, heights, xs, zs):
    # A factual map drawing, not satellite imagery or generated scene art.
    image = Image.new("RGB", (1500,960), "#101b21")
    draw = ImageDraw.Draw(image)
    font_path = "C:/Windows/Fonts/segoeui.ttf"
    font = ImageFont.truetype(font_path,20)
    title = ImageFont.truetype(font_path,32)
    small = ImageFont.truetype(font_path,16)
    draw.text((50,25),"HOTTOT-LES-BAGUES  /  REAL-WORLD MAP PROTOTYPE",font=title,fill="#e9ebda")
    draw.text((50,78),"Normandy, France     |     2,048 x 1,024 metres     |     North up     |     1:1 geographic distances",font=font,fill="#b6c6c2")
    def p(at): return (50+at[0]*1400/COLS,160+at[1]*700/ROWS)
    draw.rectangle((50,160,1450,860),fill="#76846a")
    colors={1:"#92916b",2:"#a69c7c",3:"#859579",4:"#506f55",5:"#899485",6:"#507e87"}
    # Clip all features to the playable rectangle using a separate canvas.
    canvas=Image.new("RGB",(1400,700),"#76846a")
    d=ImageDraw.Draw(canvas)
    def q(at): return (at[0]*1400/COLS,at[1]*700/ROWS)
    for a in info["areas"]: d.polygon([q(at) for at in a["points"]],fill=colors[a["kind"]])
    for r in info["roads"]: d.line([q(at) for at in r["points"]],fill="#d7cba8" if r["paved"] else "#b1a27f",width=max(2,round(r["width"]*1400/COLS)),joint="curve")
    for b in info["buildings"]:
        pts=np.asarray([[-5,-5],[5,-5],[5,5],[-5,5]])
        c,s=math.cos(b["yaw"]),math.sin(b["yaw"])
        pts=pts@np.array([[c,-s],[s,c]])+np.asarray(b["at"])
        d.polygon([q(at) for at in pts],fill="#f4e4c6",outline="#4e5047")
    image.paste(canvas,(50,160))
    draw.rectangle((50,160,1450,860),outline="#d4ddcf",width=2)
    draw.text((60,125),f"{len(info['buildings'])} playable house sites    /    {len(info['roads'])} source road ways    /    native RoW houses approximate footprints",font=font,fill="#c5d1c7")
    draw.text((55,882),"Map data © OpenStreetMap contributors · ODbL  |  Elevation: Mapzen / AWS Terrain Tiles (Copernicus EU-DEM, USGS)",font=small,fill="#b6c6c2")
    draw.line((1176,902,1449,902),fill="#e9ebda",width=3)
    draw.text((1176,918),"400 m",font=small,fill="#e9ebda")
    image.save(OUT / "map-preview.png")


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch",action="store_true",help="Download missing source files; preserves cached inputs")
    args=parser.parse_args()
    OUT.mkdir(parents=True,exist_ok=True)
    if args.fetch: fetch()
    build()
