"""Initial Blender authoring setup, using the user's three-view drawing.

Initial creation only: overwrites a10.blend. Further edits should be made in that
file and exported with export_a10.py, without rerunning this bootstrap script.
--sync-details updates only the native mechanical detail collection and previews.
"""
from pathlib import Path
import json
import math
import sys
import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[1]
REPO = ROOT.parents[2]
sys.path.insert(0, str(ROOT / "tools"))
from export_a10 import export


def convert(p):
    return (p[0], -p[2], p[1])


def aim(obj, point):
    obj.rotation_euler = (Vector(point) - obj.location).to_track_quat("-Z", "Y").to_euler()


def collection(name):
    c = bpy.data.collections.new(name)
    bpy.context.scene.collection.children.link(c)
    return c


def material(name, color, roughness=.82):
    m = bpy.data.materials.get(name) or bpy.data.materials.new(name)
    m.diffuse_color = (*color, 1)
    m.use_nodes = True
    bsdf = m.node_tree.nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (*color, 1)
    bsdf.inputs["Roughness"].default_value = roughness
    bsdf.inputs["Specular IOR Level"].default_value = .18
    if name=="Steel":
        bsdf.inputs["Metallic"].default_value=.55
        bsdf.inputs["Roughness"].default_value=.55
    if name=="AircraftPaint" and not m.node_tree.nodes.get("Aircraft wear"):
        path=REPO/"assets/art_refresh/aircraft_paint.png"
        if path.exists():
            tex=m.node_tree.nodes.new("ShaderNodeTexImage");tex.name="Aircraft wear"
            tex.image=bpy.data.images.load(str(path),check_existing=True);tex.image.pack()
            tex.projection="BOX";tex.projection_blend=.2
            coords=m.node_tree.nodes.new("ShaderNodeTexCoord")
            scale=m.node_tree.nodes.new("ShaderNodeVectorMath");scale.operation="MULTIPLY"
            scale.inputs[1].default_value=(4,4,4)
            m.node_tree.links.new(coords.outputs["Generated"],scale.inputs[0])
            m.node_tree.links.new(scale.outputs[0],tex.inputs["Vector"])
            mix=m.node_tree.nodes.new("ShaderNodeMixRGB");mix.blend_type="MULTIPLY"
            mix.inputs[0].default_value=1;mix.inputs[2].default_value=(*color,1)
            m.node_tree.links.new(tex.outputs["Color"],mix.inputs[1])
            m.node_tree.links.new(mix.outputs[0],bsdf.inputs["Base Color"])
    return m


def object_mesh(name, verts, faces, smooth=False):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata([convert(p) for p in verts], [], faces)
    mesh.update()
    for p in mesh.polygons:
        p.use_smooth = smooth
    obj = bpy.data.objects.new(name, mesh)
    surfaces.objects.link(obj)
    return obj


def loft(name, stations, sides=24, canopy=False):
    verts, faces = [], []
    count = sides + 1 if canopy else sides
    for z, w, b, t in stations:
        for n in range(count):
            a = n / sides * (math.pi if canopy else math.tau)
            y = b + math.sin(a) * (t-b) if canopy else (b+t)/2 + math.sin(a)*(t-b)/2
            verts.append((math.cos(a)*w, y, z))
    for i in range(len(stations)-1):
        for n in range(count-1 if canopy else count):
            a, b = i*count+n, i*count+(n+1)%count
            faces.append((a, b, b+count, a+count))
    faces.extend([tuple(reversed(range(count))), tuple(range(len(verts)-count, len(verts)))])
    obj = object_mesh(name, verts, faces, True)
    # Flat closing faces; curved skins retain shared-vertex normals.
    for p in obj.data.polygons[-2:]:
        p.use_smooth = False
    return obj


def wing(name, stations):
    verts, faces = [], []
    for x, y, lead, trail, t in stations:
        c = lead - trail
        verts.extend([(x,y,lead),(x,y+t*.55,lead-c*.28),(x,y+t*.16,trail),
                      (x,y-t*.16,trail),(x,y-t*.45,lead-c*.28)])
    for i in range(len(stations)-1):
        for n in range(5):
            a, b = i*5+n, i*5+(n+1)%5
            faces.append((a,a+5,b+5,b))
    faces.extend([tuple(range(5)), tuple(reversed(range(len(verts)-5,len(verts))))])
    return object_mesh(name, verts, faces)


def fin():
    outline = [(-.23,.30),(.53,.26),(.61,-.09),(.59,-.30),(-.12,-.40),(-.23,-.10)]
    verts = [(x,y,z) for x in [-.018,.018] for y,z in outline]
    count=len(outline)
    faces = [tuple(range(count)), tuple(reversed(range(count,count*2)))]
    faces += [(n,n+count,(n+1)%count+count,(n+1)%count) for n in range(count)]
    return object_mesh("Fin.R",verts,faces)


def engine():
    profile = [(-.46,.175),(-.40,.20),(-.30,.24),(.20,.25),(.47,.245),
               (.56,.235),(.59,.22),(.56,.205),(.49,.202),(.32,.195),(.20,.19)]
    verts, faces = [], []
    for z,r in profile:
        for n in range(32):
            a = n/32*math.tau
            verts.append((math.cos(a)*r, math.sin(a)*r, z))
    for i in range(len(profile)-1):
        for n in range(32):
            a,b=i*32+n,i*32+(n+1)%32
            faces.append((a,b,b+32,a+32))
    return object_mesh("Engine.R",verts,faces,True)


def camera(name, location, center, scale):
    data = bpy.data.cameras.new(name)
    obj = bpy.data.objects.new(name, data)
    studio.objects.link(obj)
    obj.location = location
    aim(obj, center)
    data.type = "ORTHO"
    data.ortho_scale = scale
    return obj


def reference(name, scale, offset, rotation):
    # Whole original image, including its attribution strip, stays packed intact.
    # Offsets align the appropriate drawing to the model origin in each view.
    obj = bpy.data.objects.new(name, None)
    references.objects.link(obj)
    obj.empty_display_type = "IMAGE"
    obj.data = image
    obj.empty_display_size = scale * 474
    obj.empty_image_offset = (0, 0)
    obj.location = offset
    obj.rotation_euler = rotation
    obj.color[3] = .60
    obj.empty_image_depth = "BACK"
    obj.empty_image_side = "DOUBLE_SIDED"
    obj.show_empty_image_orthographic = True
    obj.show_empty_image_perspective = False
    obj.show_empty_image_only_axis_aligned = True
    obj.hide_render = True
    obj["source"] = "User supplied Alamy three-view; original watermark retained; reference only"
    return obj


def sync_details():
    # Import only mechanical parts, preserving authored Blender surfaces.
    old = bpy.data.collections.get("MECHANICAL | native pivots and sockets")
    if old:
        for obj in list(old.objects):
            bpy.data.objects.remove(obj, do_unlink=True)
        bpy.data.collections.remove(old)
    col = collection("MECHANICAL | native pivots and sockets")
    data = json.loads((REPO / "assets/art_refresh/runtime_kit.json").read_text())
    plane = next(a for a in data["assets"] if a["name"] == "plane")
    controls = {}
    colors = {"AircraftPaint":(.406,.448,.462), "AircraftGlass":(.027,.045,.053),
              "Dark":(.026,.032,.032),"Steel":(.20,.23,.23),"Rubber":(.025,.027,.024),
              "Red":(.53,.16,.11),"Light":(.8,.76,.57)}
    for i, component in enumerate(plane["components"]):
        mesh = bpy.data.meshes.new(f'detail_{i}')
        v = [convert(p) for p in component["vertices"]]
        indices = component["indices"]
        mesh.from_pydata(v, [], [indices[n:n+3] for n in range(0,len(indices),3)])
        mesh.update()
        for p in mesh.polygons:
            p.use_smooth = True
        mesh.normals_split_custom_set_from_vertices([convert(n) for n in component["normals"]])
        obj = bpy.data.objects.new(f'{component["finish"]}_{i:03}',mesh)
        col.objects.link(obj)
        mesh.materials.append(material(component["finish"], colors[component["finish"]]))
        obj["runtime_detail"] = True
        role, pivot = component["joint"], component["pivot"]
        if role:
            key = role,tuple(pivot)
            if key not in controls:
                ctrl = bpy.data.objects.new(f'joint_{role}_{len(controls)}',None)
                ctrl.location = convert(pivot)
                ctrl.empty_display_type = "ARROWS"
                ctrl.empty_display_size = .12
                ctrl["joint_role"] = role
                col.objects.link(ctrl)
                controls[key] = ctrl
            obj.parent = controls[key]
            obj.location = -controls[key].location
    for i,socket in enumerate(plane["sockets"]):
        obj = bpy.data.objects.new(f'muzzle_{i}',None)
        col.objects.link(obj)
        obj.location = convert(socket["at"])
        obj.rotation_euler = Vector(convert(socket["forward"])).to_track_quat("Z","Y").to_euler()
        obj.empty_display_type = "CONE"
        obj.empty_display_size = .10


if "--sync-details" in sys.argv:
    bpy.ops.wm.open_mainfile(filepath=str(ROOT/"a10.blend"))
    scene = bpy.context.scene
    studio = bpy.data.collections["STUDIO | reference cameras"]
    sync_details()
else:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.context.preferences.filepaths.save_version = 0
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene["reference_note"] = "Three-view aligned independently: source views have slightly different scales. Original image retained intact."
    surfaces = collection("SURFACES | editable game source")
    references = collection("REFERENCES | user three-view, not exported")
    studio = collection("STUDIO | reference cameras")
    paint = material("AircraftPaint",(.406,.448,.462))
    glass = material("AircraftGlass",(.027,.045,.053),.38)
    body = loft("Fuselage",[
        (-2.34,.035,.68,.77),(-2.08,.075,.64,.79),(-1.50,.14,.47,.85),
        (-.70,.215,.43,.89),(.10,.23,.42,.90),(.85,.23,.42,.93),
        (1.45,.21,.40,.87),(1.94,.17,.44,.77),(2.23,.115,.50,.66),(2.35,.045,.54,.61)])
    # Compact canopy over the forward fuselage; the nose remains exposed ahead.
    stations = [(.90,.055,.92,.935),(1.04,.115,.92,1.105),(1.25,.13,.91,1.16),
                (1.44,.12,.87,1.14),(1.58,.09,.835,1.055),(1.77,.04,.795,.81)]
    canopy = loft("Canopy",stations,12,True)
    scene["canopy_stations"] = json.dumps(stations)
    main_wing = wing("Main wing",[
        (-2.55,.73,.10,-.54,.025),(-1.70,.695,.215,-.63,.038),
        (-.56,.65,.33,-.67,.06),(0,.65,.33,-.67,.07),(.56,.65,.33,-.67,.06),
        (1.70,.695,.215,-.63,.038),(2.55,.73,.10,-.54,.025)])
    tail = wing("Horizontal tail",[
        (-.86,.72,-1.89,-2.34,.023),(-.23,.72,-1.80,-2.34,.04),
        (.23,.72,-1.80,-2.34,.04),(.86,.72,-1.89,-2.34,.023)])
    right_fin = fin()
    right_fin.location = convert((.825,.72,-1.98))
    right_engine = engine()
    right_engine.location = convert((.44,.94,-1.10))
    for obj,name in zip([body,canopy,main_wing,tail,right_fin,right_engine],
                        ["A10Body","A10Canopy","A10Wing","A10Tail","A10Fin","JetNacelle"]):
        obj["runtime_shape"] = name
        obj.data.materials.append(glass if obj==canopy else paint)
    for obj,name in [(right_fin,"Fin.L"),(right_engine,"Engine.L")]:
        left = bpy.data.objects.new(name,obj.data)
        surfaces.objects.link(left)
        left.location = obj.location
        left.location.x *= -1
        # Shared mesh: edit either counterpart; export uses the right instance.
    image = bpy.data.images.load(str(ROOT/"references/user_three_view.png"))
    image.pack()
    # Pixel-to-metre mapping. The whole image remains visible outside the model.
    s=5.1/183
    reference("TOP | nose forward", s, (355*s, (386-91)*s-2.35, -.025), (0,0,math.pi))
    s=4.69/180
    reference("SIDE | nose left", s, (-3, -2.35-29*s, -(386-290)*s), (math.pi/2,0,math.pi/2))
    s=5.1/181
    reference("FRONT | intakes", s, (-118.5*s, 2.65, -(386-111)*s), (math.pi/2,0,0))
    # Orthographic cameras make direct inspection straightforward.
    top_camera=camera("REFERENCE_TOP",(0,0,12),(0,0,0),6.2)
    top_camera.rotation_euler.z += math.pi
    camera("REFERENCE_SIDE",(12,0,.7),(0,0,.7),5.8)
    camera("REFERENCE_FRONT",(0,-12,.7),(0,0,.7),6.2)
    camera("BEAUTY",(8,-11,8),(0,0,.65),7.3)
    for name, loc, power, size in [("Key",(5,-7,10),1400,7),("Fill",(-7,-4,7),1000,7),("Rim",(0,8,7),1500,6)]:
        d=bpy.data.lights.new(name,"AREA");d.energy=power;d.size=size
        o=bpy.data.objects.new(name,d);studio.objects.link(o);o.location=loc;aim(o,(0,0,.7))
    world=bpy.data.worlds.new("Neutral studio");world.use_nodes=True
    world.node_tree.nodes.get("Background").inputs["Color"].default_value=(.11,.13,.16,1)
    scene.world=world
    text=bpy.data.texts.new("EXPORT_A10.py")
    text.write((ROOT/"tools/export_a10.py").read_text())
    text=bpy.data.texts.new("READ ME | model and reference workflow")
    text.write("Edit meshes in SURFACES. Fin and engine counterparts share mesh data.\n"
               "Use Top / Right / Front orthographic views for packed reference alignment.\n"
               "References retain the original watermark and are never exported into the game.\n"
               "Save a10.blend, then run EXPORT_A10.py to update a10_geometry.json; rebuild the game.\n"
               "MECHANICAL is a review of native rigged detail; edit those parts in aircraft_models.rs.\n")
    for screen in bpy.data.screens:
        for area in screen.areas:
            if area.type=="VIEW_3D":
                space=area.spaces.active
                space.region_3d.view_rotation=(0,0,0,1)
                space.region_3d.view_perspective="ORTHO"
                space.region_3d.view_location=(0,0,0)
                space.region_3d.view_distance=6.5
                space.shading.type="SOLID"
                space.shading.color_type="MATERIAL"
    scene.camera=bpy.data.objects["BEAUTY"]
    bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/"a10.blend"))
    export()

scene.camera=bpy.data.objects["BEAUTY"]
bpy.context.preferences.filepaths.save_version=0
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type=="VIEW_3D":
            space=area.spaces.active
            space.shading.show_xray=True
            space.shading.xray_alpha=.35
            space.overlay.show_floor=False
            space.overlay.show_axis_x=False
            space.overlay.show_axis_y=False
for obj in bpy.data.objects:
    if obj.type in {"CAMERA","LIGHT"} or (obj.type=="EMPTY" and obj.empty_display_type!="IMAGE"):
        obj.hide_set(True)
scene.render.engine="CYCLES";scene.cycles.samples=24;scene.cycles.use_denoising=True
scene.render.resolution_x=1400;scene.render.resolution_y=1050;scene.render.resolution_percentage=100
scene.render.image_settings.file_format="PNG";scene.view_settings.view_transform="AgX"
bpy.ops.wm.save_as_mainfile(filepath=str(ROOT/"a10.blend"))
for name,cam in [("a10_blender","BEAUTY"),("a10_top","REFERENCE_TOP"),("a10_side","REFERENCE_SIDE")]:
    scene.camera=bpy.data.objects[cam]
    scene.render.filepath=str(ROOT/"previews"/(name+".png"))
    bpy.ops.render.render(write_still=True)
    print("A10_RENDERED",name,flush=True)
print("A10_BLENDER_COMPLETE",flush=True)
