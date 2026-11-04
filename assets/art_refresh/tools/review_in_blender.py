"""Create editable Blender review libraries from the exact native game meshes.

First run the game with --export-art assets/art_refresh. Then run this script
with Blender --background --factory-startup --python review_in_blender.py.
Runtime geometry remains parametric in native/src; this is an exact review export.
"""
from pathlib import Path
import json
import math
import sys
import bpy
from mathutils import Vector

ROOT=Path(__file__).resolve().parents[1]
REPO=ROOT.parents[1]
DATA=json.loads((ROOT/"runtime_kit.json").read_text())
COLORS={
    "Team":(.26,.31,.20),"Dark":(.035,.045,.043),"Steel":(.20,.23,.23),
    "Cloth":(.23,.28,.16),"Roof":(.27,.23,.18),"Concrete":(.48,.46,.41),
    "Glass":(.035,.075,.09),"Wood":(.25,.18,.10),"Light":(.80,.76,.57),
    "Brass":(.48,.38,.16),"Red":(.53,.16,.11),"White":(.73,.73,.65),
    "Earth":(.27,.20,.12),"Brick":(.43,.24,.17),"Plaster":(.71,.68,.57),
    "Charred":(.045,.039,.03),"Scorched":(.11,.12,.09),"Rust":(.26,.13,.06),
    "Rubber":(.025,.027,.024),"Asphalt":(.13,.14,.13),"Gravel":(.31,.30,.26),
    "AircraftPaint":(.41,.45,.42),"Scorch":(.035,.03,.025),"Ochre":(.64,.43,.19),
    "Sage":(.38,.49,.34),"BluePlaster":(.29,.43,.53),"Rose":(.57,.33,.27),
    "Ivory":(.71,.66,.51),"Terracotta":(.45,.23,.13),"SlateRoof":(.17,.23,.28),
    "GreenRoof":(.19,.30,.22),"Skin":(.53,.35,.23),"AircraftGlass":(.027,.045,.053),
    "A10Livery":(1.,1.,1.),
}
TEXTURES={"Steel":"steel","Concrete":"concrete","Wood":"wood","Brick":"brick",
    "Plaster":"plaster","Cloth":"cloth","Roof":"roof","AircraftPaint":"aircraft_paint",
    "Earth":"soil","Rubber":"rubber","Asphalt":"asphalt","Gravel":"gravel",
    "Ochre":"plaster","Sage":"plaster","BluePlaster":"plaster","Rose":"plaster",
    "Ivory":"plaster","Terracotta":"roof","SlateRoof":"roof","GreenRoof":"roof",
    "Charred":"wood","Rust":"steel"}
TEXTURES['A10Livery']='a10_livery'


def convert(p):return (p[0],-p[2],p[1])


def aim(obj,point):
    obj.rotation_euler=(Vector(point)-obj.location).to_track_quat("-Z","Y").to_euler()


aircraft_review="--aircraft-review" in sys.argv
for category in (["vehicles"] if aircraft_review else ["buildings","vehicles"]):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.context.preferences.filepaths.save_version=0
    scene=bpy.context.scene
    scene.unit_settings.system="METRIC"
    mats={}
    assets=[a for a in DATA["assets"] if a["category"]==category]

    def material(name):
        key=name+"_"+asset["name"] if name=="AircraftPaint" else name
        if key in mats:return mats[key]
        mat=bpy.data.materials.new(key)
        mat.use_nodes=True
        color=COLORS[name]
        if name=="AircraftPaint":
            color=(.406,.448,.462) if asset["name"]=="plane" else (.068,.095,.053)
        bsdf=mat.node_tree.nodes.get("Principled BSDF")
        bsdf.inputs["Base Color"].default_value=(*color,1)
        bsdf.inputs["Roughness"].default_value=.85
        if name in ["Steel","Brass"]:
            bsdf.inputs["Metallic"].default_value=.55
            bsdf.inputs["Roughness"].default_value=.55
        if name=="Glass":bsdf.inputs["Roughness"].default_value=.20
        if name=="AircraftGlass":
            bsdf.inputs["Roughness"].default_value=.38
            bsdf.inputs["Specular IOR Level"].default_value=.16
        if name=="AircraftPaint":
            bsdf.inputs["Roughness"].default_value=.82
            bsdf.inputs["Metallic"].default_value=.05
            bsdf.inputs["Specular IOR Level"].default_value=.18
        path=(ROOT/"armour_0.png") if name=="Team" and category=="vehicles" else REPO/"materials"/(TEXTURES.get(name,"no_texture")+".png")
        if name=="AircraftPaint":path=ROOT/"aircraft_paint.png"
        if path.exists():
            tex=mat.node_tree.nodes.new("ShaderNodeTexImage")
            tex.image=bpy.data.images.load(str(path),check_existing=True)
            if name=="Team":
                mat.node_tree.links.new(tex.outputs["Color"],bsdf.inputs["Base Color"])
            else:
                mix=mat.node_tree.nodes.new("ShaderNodeMixRGB")
                mix.blend_type="MULTIPLY"
                mix.inputs[0].default_value=1
                mix.inputs[2].default_value=(*color,1)
                mat.node_tree.links.new(tex.outputs["Color"],mix.inputs[1])
                mat.node_tree.links.new(mix.outputs[0],bsdf.inputs["Base Color"])
        mat.diffuse_color=(*color,1)
        mats[key]=mat
        return mat

    roots=[]
    for asset in assets:
        col=bpy.data.collections.new(asset["name"])
        scene.collection.children.link(col)
        root=bpy.data.objects.new(asset["name"],None)
        col.objects.link(root)
        root["source"]="Exact runtime mesh export; native/src/models.rs"
        root["category"]=category
        controls={}
        for i,batch in enumerate(asset["batches"]):
            mesh=bpy.data.meshes.new(f'{asset["name"]}_{i}')
            verts=[convert(p) for p in batch["vertices"]]
            faces=[batch["indices"][j:j+3] for j in range(0,len(batch["indices"]),3)]
            mesh.from_pydata(verts,[],faces)
            mesh.update()
            for p in mesh.polygons:p.use_smooth=True
            mesh.normals_split_custom_set_from_vertices([convert(n) for n in batch["normals"]])
            uv=mesh.uv_layers.new(name="UVMap")
            for loop in mesh.loops:uv.data[loop.index].uv=batch["uv"][loop.vertex_index]
            label=("wall" if batch["wallNormal"] else "roof") if batch["roof"] is not None else "body"
            obj=bpy.data.objects.new(f'{label}_{batch["finish"]}_{i:03d}',mesh)
            col.objects.link(obj)
            mesh.materials.append(material(batch["finish"]))
            obj.parent=root
            obj["roof_lot"]=batch["roof"] if batch["roof"] is not None else -2
            if batch["wallNormal"]:obj["wall_normal"]=batch["wallNormal"]
            obj["joint_role"]=batch["joint"]
            if batch["joint"]:
                key=(batch["joint"],tuple(batch["pivot"]))
                if key not in controls:
                    ctrl=bpy.data.objects.new(f'joint_{batch["joint"]}_{len(controls)}',None)
                    col.objects.link(ctrl)
                    ctrl.parent=root
                    ctrl.location=convert(batch["pivot"])
                    ctrl.empty_display_type="ARROWS"
                    ctrl.empty_display_size=.25
                    controls[key]=ctrl
                obj.parent=controls[key]
                obj.location=-Vector(convert(batch["pivot"]))
        for i,socket in enumerate(asset["sockets"]):
            tip=bpy.data.objects.new(f'muzzle_{i}',None)
            col.objects.link(tip)
            tip.parent=root
            tip.location=convert(socket["at"])
            tip.empty_display_type="CONE"
            tip.empty_display_size=.10
            tip.rotation_euler=Vector(convert(socket["forward"])).to_track_quat("Z","Y").to_euler()
        roots.append((root,col))
        col.hide_render=True
        col.hide_viewport=True

    # One selected asset opens visible; every other collection remains selectable.
    preferred="plane" if aircraft_review else ("house_0_intact" if category=="buildings" else "medium")
    root,col=next(p for p in roots if p[0].name==preferred)
    col.hide_render=False
    col.hide_viewport=False
    studio=bpy.data.collections.new("STUDIO | review only")
    scene.collection.children.link(studio)
    for name,loc,power,size in [("Key",(8,-10,15),1600,8),("Fill",(-8,-4,10),1100,7),("Rim",(1,9,11),1700,6)]:
        data=bpy.data.lights.new(name,"AREA");data.energy=power;data.size=size
        light=bpy.data.objects.new(name,data);studio.objects.link(light);light.location=loc;aim(light,(0,0,1))
    world=bpy.data.worlds.new("Review world");world.use_nodes=True
    world.node_tree.nodes.get("Background").inputs["Color"].default_value=(.10,.12,.14,1)
    scene.world=world
    camera_data=bpy.data.cameras.new("Review camera")
    camera=bpy.data.objects.new("Review camera",camera_data);studio.objects.link(camera)
    camera_data.type="ORTHO"
    scene.camera=camera
    scene.render.engine="CYCLES";scene.cycles.samples=24;scene.cycles.use_denoising=True
    scene.render.resolution_x=1200;scene.render.resolution_y=900;scene.render.resolution_percentage=100
    scene.render.image_settings.file_format="PNG";scene.view_settings.view_transform="AgX"
    for im in bpy.data.images:
        if im.source=="FILE":im.pack()
    points=[Vector(convert(v)) for a in assets if a["name"]==preferred for b in a["batches"] for v in b["vertices"]]
    low=Vector([min(p[i] for p in points) for i in range(3)])
    high=Vector([max(p[i] for p in points) for i in range(3)])
    center=(low+high)*.5
    span=max(high-low)*1.60
    camera_data.ortho_scale=span
    camera.location=center+Vector((1.0,-1.5,1.15))*span
    aim(camera,center)
    bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/f"{category}.blend"))
    (ROOT/"previews").mkdir(exist_ok=True)
    names=["house_0_intact","house_1_intact","house_2_intact","house_0_damaged","house_0_ruin","vehicleDepot","fieldHospital"] if category=="buildings" else ["medium","troopCarrier","xtralong","long","antiAir","plane","helicopter"]
    if aircraft_review:names=["plane","helicopter"]
    for name in names:
        for _,c in roots:c.hide_render=True;c.hide_viewport=True
        obj,col=next(p for p in roots if p[0].name==name)
        col.hide_render=False;col.hide_viewport=False
        points=[Vector(convert(v)) for a in assets if a["name"]==name for b in a["batches"] for v in b["vertices"]]
        low=Vector([min(p[i] for p in points) for i in range(3)])
        high=Vector([max(p[i] for p in points) for i in range(3)])
        center=(low+high)*.5
        span=max((high-low).x,(high-low).y,(high-low).z)*1.60
        camera_data.ortho_scale=span
        camera.location=center+Vector((1.0,-1.5,1.15))*span
        aim(camera,center)
        scene.render.filepath=str(ROOT/"previews"/(name+".png"))
        bpy.ops.render.render(write_still=True)
        print("REVIEW_RENDERED",name,flush=True)
        if name in ("plane","helicopter"):
            camera.location=center+Vector((1.8,-.35,.30))*span
            aim(camera,center)
            scene.render.filepath=str(ROOT/"previews"/(name+"_side.png"))
            bpy.ops.render.render(write_still=True)
            print("REVIEW_RENDERED",name+"_side",flush=True)
print("REVIEW_COMPLETE",flush=True)
