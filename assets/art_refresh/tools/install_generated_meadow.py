"""Install the generated albedo without altering the original source textures."""
from pathlib import Path
from PIL import Image
import argparse, hashlib, json, shutil
ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/"materials/pbr"
NAME="field_meadow_v1"
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("--source", type=Path, default=OUT/(NAME+"_albedo.png"))
SOURCE=parser.parse_args().source
if SOURCE.resolve() != (OUT/(NAME+"_albedo.png")).resolve():
    shutil.copy2(SOURCE, OUT/(NAME+"_albedo.png"))
# Neutral technical maps: do not invent geometry from colour or reuse the
# unrelated aerial scan's normals. Existing macro scan retains its own normals.
Image.new("RGB",(4,4),(128,128,255)).save(OUT/(NAME+"_normal.png"))
Image.new("RGB",(4,4),(255,245,0)).save(OUT/(NAME+"_orm.png"))
catalog_path=OUT/"catalog.json"
catalog=json.loads(catalog_path.read_text())
catalog["materials"][NAME]={
    "repeat_m":[22.0,22.0],
    "size":list(Image.open(SOURCE).size),
    "source":"OpenAI built-in image generation",
    "author":"AI-generated for Rule Of War",
    "provenance":"assets/art_refresh/terrain-albedo-v1.json",
    "normal":"Neutral OpenGL +Y; no inferred relief",
    "orm":"Constant R=255 (unoccluded), G=245 (matte), B=0 (nonmetal)",
    "runtime_sha256":{s:hashlib.sha256((OUT/(NAME+"_"+s+".png")).read_bytes()).hexdigest() for s in ["albedo","normal","orm"]}
}
catalog_path.write_text(json.dumps(catalog,indent=2)+"\n")
print(json.dumps(catalog["materials"][NAME],indent=2))
