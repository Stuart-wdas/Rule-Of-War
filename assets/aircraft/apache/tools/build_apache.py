"""AH-64-inspired authored mesh, articulated batches and coherent UV atlas.
Blender 5.2 --background --python this-file. Canonical coordinates: +Y up, +Z nose.
"""
from pathlib import Path
import bpy,bmesh,json,math,runpy
from mathutils import Vector
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'assets/aircraft/apache';OUT.mkdir(parents=True,exist_ok=True)
PIVOTS={0:(0,0,0),20:(0,.43,1.02),30:(0,1.89,-.46),33:(-.15,1.43,-3.28)}
GROUPS=[('ApacheSkin','AircraftPaint',0),('ApacheMetal','Steel',0),('ApacheDark','Dark',0),('ApacheTyres','Rubber',0),('ApacheGlass','AircraftGlass',0),('ApacheRotor','Dark',30),('ApacheRotorMetal','Steel',30),('ApacheTailRotor','Dark',33),('ApacheTailRotorMetal','Steel',33),('ApacheGun','Dark',20),('ApacheGunMetal','Steel',20),('ApacheRed','Red',0),('ApacheLight','Light',0)]
COLOURS={'AircraftPaint':(.135,.155,.095,1),'Steel':(.10,.115,.11,1),'Dark':(.024,.029,.028,1),'Rubber':(.018,.021,.019,1),'AircraftGlass':(.025,.075,.09,1),'Red':(.5,.02,.015,1),'Light':(.85,.75,.5,1)}
def xyz(p):return (p[0],-p[2],p[1])
def native(p):return [round(p[0],7),round(p[2],7),round(-p[1],7)]
bpy.ops.object.select_all(action='SELECT');bpy.ops.object.delete(use_global=False)
MATS={}
for finish,colour in COLOURS.items():
    mat=bpy.data.materials.new(finish);mat.use_nodes=True;mat.diffuse_color=colour
    bsdf=mat.node_tree.nodes.get('Principled BSDF');bsdf.inputs['Base Color'].default_value=colour
    bsdf.inputs['Roughness'].default_value=.63 if finish=='AircraftPaint' else .35
    bsdf.inputs['Metallic'].default_value=.82 if finish=='Steel' else .02
    if finish=='AircraftGlass':bsdf.inputs['Roughness'].default_value=.16;bsdf.inputs['Coat Weight'].default_value=.65
    if finish=='Rubber':bsdf.inputs['Roughness'].default_value=.92
    MATS[finish]=mat
objects=[]
def mesh(name,verts,faces,finish='AircraftPaint',joint=0,bevel=0,smooth=False):
    data=bpy.data.meshes.new(name);data.from_pydata([xyz(p) for p in verts],[],faces);data.update()
    bm=bmesh.new();bm.from_mesh(data);bmesh.ops.recalc_face_normals(bm,faces=list(bm.faces));bm.to_mesh(data);bm.free()
    obj=bpy.data.objects.new(name,data);bpy.context.collection.objects.link(obj);obj.data.materials.append(MATS[finish]);obj['finish']=finish;obj['joint']=joint
    bpy.context.view_layer.objects.active=obj;obj.select_set(True)
    if bevel:
        m=obj.modifiers.new('Small manufactured edge radii','BEVEL');m.width=bevel;m.segments=2;m.limit_method='ANGLE';m.angle_limit=.28
        bpy.ops.object.modifier_apply(modifier=m.name)
    for poly in obj.data.polygons:poly.use_smooth=True
    if not smooth:
        m=obj.modifiers.new('Area weighted panel shading','WEIGHTED_NORMAL');m.keep_sharp=True;m.weight=50
        bpy.ops.object.modifier_apply(modifier=m.name)
    attr=obj.data.attributes.new('runtime_joint','INT','FACE')
    for v in attr.data:v.value=joint
    obj.select_set(False);objects.append(obj);return obj
def box(name,at,size,finish='AircraftPaint',joint=0,bevel=.012):
    x,y,z=at;a,b,c=[v/2 for v in size]
    verts=[(x+dx*a,y+dy*b,z+dz*c) for dx,dy,dz in [(-1,-1,-1),(1,-1,-1),(1,1,-1),(-1,1,-1),(-1,-1,1),(1,-1,1),(1,1,1),(-1,1,1)]]
    return mesh(name,verts,[(0,3,2,1),(4,5,6,7),(0,1,5,4),(3,7,6,2),(1,2,6,5),(0,4,7,3)],finish,joint,bevel)
def rod(name,a,b,r,finish='Steel',joint=0,sides=12):
    a=Vector(a);b=Vector(b);axis=(b-a).normalized();u=axis.cross(Vector((1,0,0)))
    if u.length<.1:u=axis.cross(Vector((0,1,0)))
    u.normalize();v=axis.cross(u);verts=[p+r*(math.cos(i*math.tau/sides)*u+math.sin(i*math.tau/sides)*v) for p in [a,b] for i in range(sides)]
    faces=[tuple(reversed(range(sides))),tuple(range(sides,2*sides))]+[(i,(i+1)%sides,(i+1)%sides+sides,i+sides) for i in range(sides)]
    return mesh(name,verts,faces,finish,joint,0,True)
def sphere(name,at,scale,finish='AircraftPaint',joint=0,segments=20,rings=12):
    bpy.ops.mesh.primitive_uv_sphere_add(segments=segments,ring_count=rings,location=xyz(at));obj=bpy.context.object;obj.name=name
    obj.scale=(scale[0]/2,scale[2]/2,scale[1]/2);bpy.ops.object.transform_apply(location=True,rotation=True,scale=True)
    data=obj.data;verts=[native(v.co) for v in data.vertices];faces=[tuple(p.vertices) for p in data.polygons];bpy.data.objects.remove(obj,do_unlink=True)
    return mesh(name,verts,faces,finish,joint,0,True)
def loft(name,stations,finish='AircraftPaint',sides=24,subdiv=3,exponent=1):
    # Smooth Catmull-Rom interpolation along the real airframe, not scaled boxes.
    st=[]
    for i in range(len(stations)-1):
        p0,p1,p2,p3=[Vector(stations[j]) for j in [max(i-1,0),i,i+1,min(i+2,len(stations)-1)]]
        for j in range(subdiv):
            t=j/subdiv;st.append(.5*((2*p1)+(-p0+p2)*t+(2*p0-5*p1+4*p2-p3)*t*t+(-p0+3*p1-3*p2+p3)*t*t*t))
    st.append(Vector(stations[-1]));verts=[]
    for z,w,b,t in st:
        for i in range(sides):
            a=i*math.tau/sides;c=math.cos(a);s=math.sin(a)
            verts.append((math.copysign(abs(c)**exponent,c)*w,(b+t)/2+math.copysign(abs(s)**exponent,s)*(t-b)/2,z))
    faces=[tuple(reversed(range(sides))),tuple(range((len(st)-1)*sides,len(st)*sides))]
    for row in range(len(st)-1):
        for i in range(sides):faces.append((row*sides+i,row*sides+(i+1)%sides,(row+1)*sides+(i+1)%sides,(row+1)*sides+i))
    return mesh(name,verts,faces,finish,0,0,True)
loft('Continuous armoured fuselage',[(-1.70,.12,.74,1.08),(-1.38,.26,.53,1.28),(-.95,.35,.39,1.38),(-.55,.345,.35,1.13),(0,.315,.36,.98),(.60,.285,.40,.88),(1.10,.235,.48,.86),(1.40,.18,.57,.84),(1.62,.12,.65,.81)],exponent=.77)
loft('Tapered tail boom',[(-3.46,.038,.99,1.08),(-3.05,.060,.94,1.095),(-2.35,.095,.88,1.13),(-1.65,.14,.80,1.18),(-1.31,.17,.75,1.16)],sides=18)
loft('Transmission fairing',[(-1.32,.16,1.07,1.31),(-.97,.275,1.10,1.53),(-.58,.28,1.15,1.60),(-.25,.19,1.12,1.44)],sides=20)
WINDOWS=[(-.52,.215,1.025,1.46),(-.18,.25,.99,1.53),(.16,.25,.945,1.50),(.47,.24,.91,1.28),(.92,.22,.855,1.26),(1.30,.175,.815,1.18),(1.49,.12,.80,.96)]
for a,b in zip(WINDOWS,WINDOWS[1:]):
    za,wa,ba,ta=a;zb,wb,bb,tb=b
    for sign in [-1,1]:
        verts=[(sign*wa,ba,za),(sign*wa*.91,ta-.045,za),(sign*wb*.91,tb-.045,zb),(sign*wb,bb,zb)]
        mesh('Flat optical cockpit pane',verts,[(0,1,2,3)],'AircraftGlass')
        rod('Canopy lower sill',verts[0],verts[3],.010,'AircraftPaint')
        rod('Canopy upper longeron',verts[1],verts[2],.009,'AircraftPaint')
    mesh('Canopy crown',[(-wa*.91,ta-.045,za),(wa*.91,ta-.045,za),(wb*.91,tb-.045,zb),(-wb*.91,tb-.045,zb)],[(0,1,2,3)],'AircraftGlass' if (za in [.16,1.30]) else 'AircraftPaint')
for z,w,b,t in WINDOWS:
    for sign in [-1,1]:rod('Fine cockpit frame',(sign*w,b,z),(sign*w*.91,t-.045,z),.009,'AircraftPaint')
    rod('Cockpit roof bow',(-w*.91,t-.04,z),(w*.91,t-.04,z),.009,'AircraftPaint')
for z,y in [(-.18,1.15),(.85,.96)]:
    box('Tandem seat back',(0,y,z),(.30,.25,.13),'Dark',bevel=.035)
    sphere('Crew helmet',(0,y+.16,z+.035),(.14,.15,.14),'Dark',segments=12,rings=8)
for side in [-1,1]:
    obj=loft('Rounded engine nacelle',[(-1.58,.105,1.14,1.37),(-1.42,.195,1.04,1.47),(-.92,.215,1.01,1.48),(-.50,.18,1.10,1.43),(-.35,.15,1.13,1.41)],sides=24)
    for v in obj.data.vertices:v.co.x+=side*.45
    # Deep intakes, separate lips and short exhaust ducts.
    rod('Engine inlet rim',(side*.45,1.27,-.36),(side*.45,1.27,-.31),.159,'AircraftPaint',sides=24)
    rod('Recessed intake',(side*.45,1.27,-.305),(side*.45,1.27,-.295),.133,'Dark',sides=24)
    for n in range(6):
        a=n*math.tau/6;rod('Inlet separator',(side*.45,1.27,-.288),(side*.45+math.cos(a)*.12,1.27+math.sin(a)*.12,-.288),.005,'Steel',sides=6)
    rod('Exhaust sleeve',(side*.45,1.26,-1.44),(side*.45,1.26,-1.65),.115,'Steel',sides=20)
    rod('Exhaust bore',(side*.45,1.26,-1.655),(side*.45,1.26,-1.662),.094,'Dark',sides=20)
    for z in [-1.25,-1.17,-1.09,-1.01,-.93]:box('Engine cooling vent',(side*.648,1.25,z),(.007,.10,.025),'Dark',bevel=.002)
    sphere('Avionics shoulder',(side*.285,.69,.35),(.22,.31,1.59),segments=20,rings=10)
    # Cambered stub wing, thin at the tip rather than a rectangular slab.
    sections=[(.32,.88,.08,-.68,.060),(.58,.85,-.005,-.66,.047),(1.05,.78,-.22,-.62,.025),(1.31,.75,-.27,-.60,.012)]
    verts=[]
    for x,y,lead,trail,h in sections:
        verts.extend([(side*x,y,lead),(side*x,y+h,lead*.7+trail*.3),(side*x,y+h*.13,trail),(side*x,y-h*.13,trail),(side*x,y-h*.7,lead*.7+trail*.3)])
    faces=[tuple(reversed(range(5))),tuple(range(15,20))]+[(r*5+i,(r+1)*5+i,(r+1)*5+(i+1)%5,r*5+(i+1)%5) for r in range(3) for i in range(5)]
    mesh('Tapered weapon wing',verts,faces,bevel=.008)
    for x in [.76,1.13]:box('Pylon',(side*x,.65,-.27),(.045,.20,.25),bevel=.008)
    rod('Rocket pod',(side*.76,.51,-.53),(side*.76,.51,.15),.15,'AircraftPaint',sides=24)
    rod('Dark rocket face',(side*.76,.51,.152),(side*.76,.51,.16),.134,'Dark',sides=24)
    for count,radius in [(1,0),(6,.045),(12,.091)]:
        for n in range(count):
            a=n*math.tau/count;x=side*.76+math.cos(a)*radius;y=.51+math.sin(a)*radius
            rod('Rocket tube lip',(x,y,.162),(x,y,.177),.017,'Steel',sides=8)
            rod('Rocket tube recess',(x,y,.179),(x,y,.181),.012,'Dark',sides=8)
    for dx in [-.065,.065]:
        for y in [.43,.56]:
            x=side*1.13+dx
            rod('Missile body',(x,y,-.55),(x,y,.10),.035,'Dark')
            sphere('Missile ogive',(x,y,.115),(.068,.068,.16),'Dark',segments=12,rings=8)
            for axis in [0,1]:
                verts=[(x-.08,y,-.52),(x+.08,y,-.52),(x+.025,y,-.35),(x-.025,y,-.35)] if axis==0 else [(x,y-.08,-.52),(x,y+.08,-.52),(x,y+.025,-.35),(x,y-.025,-.35)]
                mesh('Missile stabilizer',verts,[(0,1,2,3)],'Steel')
    # Fixed splayed undercarriage and profiled tyre sidewalls.
    rod('Main oleo',(side*.28,.68,.35),(side*.56,.16,.45),.024)
    rod('Gear brace',(side*.29,.59,-.05),(side*.52,.25,.44),.014)
    rod('Wheel axle',(side*.48,.16,.45),(side*.62,.16,.45),.022)
    rod('Main tyre',(side*.50,.16,.45),(side*.62,.16,.45),.15,'Rubber',sides=24)
    rod('Wheel hub',(side*.495,.16,.45),(side*.625,.16,.45),.065,'Steel',sides=16)
    for z in [-.18,.18,.56,.96]:rod('Fuselage seam',(side*.327,.58,z),(side*.31,.78,z),.0035,'Dark',sides=6)
    sphere('Position lamp',(side*1.30,.77,-.28),(.025,.026,.035),'Red' if side<0 else 'Light',segments=12,rings=8)
# Blunt layered nose optics and aimable chin gun.
sphere('Nose sensor turret',(0,.755,1.62),(.39,.29,.29),segments=24,rings=12)
for x,r in [(-.085,.045),(.073,.068)]:
    rod('Optical bezel',(x,.77,1.73),(x,.77,1.788),r+.013,'Dark',sides=20)
    rod('Blue optical lens',(x,.77,1.79),(x,.77,1.794),r,'AircraftGlass',sides=20)
rod('Gun azimuth bearing',(0,.38,1.02),(0,.49,1.02),.085,'Steel',20)
box('Gun receiver',(0,.35,1.10),(.15,.12,.26),'Dark',20,bevel=.012)
rod('Chain gun barrel',(0,.345,1.18),(0,.345,1.66),.021,'Steel',20,sides=16)
rod('Muzzle brake',(0,.345,1.63),(0,.345,1.69),.029,'Dark',20,sides=16)
rod('Ammunition feed',(.09,.38,.95),(.09,.35,1.12),.025,'Steel',20,sides=12)
# Broad swept fin with an actual thin section and rounded edges.
outline=[(.97,-3.44),(1.79,-3.36),(1.78,-3.17),(1.17,-2.88),(1.02,-3.02)]
verts=[(x,y,z) for x in [-.024,.024] for y,z in outline]
mesh('Tail fin',verts,[tuple(reversed(range(5))),tuple(range(5,10))]+[(i,(i+1)%5,(i+1)%5+5,i+5) for i in range(5)],bevel=.008)
box('Horizontal stabilizer',(0,1.055,-2.83),(1.12,.035,.37),bevel=.014)
rod('Tailwheel leg',(0,.98,-2.95),(0,.11,-2.99),.015)
rod('Tail tyre',(-.05,.105,-2.99),(.05,.105,-2.99),.103,'Rubber',sides=20)
rod('Tail hub',(-.056,.105,-2.99),(.056,.105,-2.99),.043,'Steel',sides=12)
tail=Vector(PIVOTS[33]);rod('Tail drive hub',tail-Vector((.03,0,0)),tail+Vector((.10,0,0)),.055,'Steel',33)
for a in [0,1.04,math.pi,math.pi+1.04]:
    r=Vector((0,math.cos(a),math.sin(a)));t=Vector((0,-math.sin(a),math.cos(a)))
    verts=[tail+r*rad+t*chord+Vector((dx,0,0)) for dx in [-.006,.006] for rad,chord in [(.07,-.028),(.46,-.025),(.48,.020),(.10,.032)]]
    mesh('Scissor tail rotor',verts,[(0,3,2,1),(4,5,6,7),(0,1,5,4),(1,2,6,5),(2,3,7,6),(3,0,4,7)],'Dark',33,0,False)
# Articulated mast, swashplate, pitch links and swept, twisted rotor airfoils.
rotor=Vector(PIVOTS[30]);rod('Rotor mast',(0,1.45,-.46),(0,1.99,-.46),.055,'Steel')
rod('Swash plate',(0,1.69,-.46),(0,1.75,-.46),.13,'Steel')
rod('Moving rotor hub',(0,1.84,-.46),(0,1.94,-.46),.12,'Steel',30)
for i in range(4):
    a=i*math.pi/2;r=Vector((math.cos(a),0,math.sin(a)));t=Vector((-math.sin(a),0,math.cos(a)))
    rod('Blade grip',rotor+r*.07,rotor+r*.32,.038,'Steel',30)
    rod('Pitch control link',Vector((0,1.72,-.46))+r*.1,rotor+r*.35+t*.025,.009,'Steel',30,sides=8)
    stations=[(.29,.065,0),(.53,.085,0),(1.20,.083,0),(2.20,.076,0),(2.54,.072,-.035),(2.73,.047,-.10)]
    verts=[]
    for rad,chord,sweep in stations:
        for n in range(8):
            angle=n*math.tau/8;across=math.cos(angle)*chord;h=math.sin(angle)*.008+across*(.06-rad*.018)
            verts.append(rotor+r*rad+t*(across+sweep)+Vector((0,h,0)))
    faces=[tuple(reversed(range(8))),tuple(range(40,48))]+[(row*8+n,row*8+(n+1)%8,(row+1)*8+(n+1)%8,(row+1)*8+n) for row in range(5) for n in range(8)]
    mesh('Swept main rotor blade',verts,faces,'Dark',30,0,True)
rod('Radar mast',(0,1.94,-.46),(0,2.16,-.46),.032,'Steel')
sphere('Flattened Longbow radar',(0,2.18,-.46),(.47,.17,.43),segments=24,rings=12)
rod('Tail antenna',(0,1.11,-1.50),(0,1.62,-1.66),.005,'Dark',sides=8)
for side in [-1,1]:
    rod('Wire strike / cable guide',(side*.22,1.13,-.58),(side*.25,1.72,-.61),.006,'Dark',sides=8)

# Join for a single packing density; preserve per-face rigid joint identity.
for obj in objects:obj.select_set(True)
bpy.context.view_layer.objects.active=objects[0];bpy.ops.object.join();obj=bpy.context.object;obj.name='Apache authored airframe'
obj.data.uv_layers.new(name='Atlas');obj.data.uv_layers.active_index=0
bpy.ops.object.mode_set(mode='EDIT');bpy.ops.mesh.select_all(action='SELECT')
bpy.ops.uv.smart_project(angle_limit=math.radians(70),island_margin=.001,area_weight=.5)
bpy.ops.uv.average_islands_scale();bpy.ops.object.mode_set(mode='OBJECT')
for poly in obj.data.polygons:
    factor=1.5 if obj.data.materials[poly.material_index].name=='AircraftPaint' else .7
    for loop in poly.loop_indices:obj.data.uv_layers.active.data[loop].uv*=factor
bpy.ops.object.mode_set(mode='EDIT');bpy.ops.uv.pack_islands(rotate=True,margin=.001)
bpy.ops.uv.export_layout(filepath=str(OUT/'uv_layout.svg'),export_all=True,mode='SVG',size=(2048,2048),opacity=.1)
bpy.ops.object.mode_set(mode='OBJECT')
data=obj.data;data.calc_loop_triangles();uv=data.uv_layers.active.data;groups={}
for tri in data.loop_triangles:
    points=[data.vertices[data.loops[l].vertex_index].co for l in tri.loops]
    if (points[1]-points[0]).cross(points[2]-points[0]).length<.00000022:continue
    finish=data.materials[tri.material_index].name;joint=data.attributes['runtime_joint'].data[tri.polygon_index].value
    group=groups.setdefault((finish,joint),{'vertices':[],'normals':[],'uv':[],'indices':[]})
    for loop in tri.loops:
        group['vertices'].append(native(data.vertices[data.loops[loop].vertex_index].co));group['normals'].append(native(data.corner_normals[loop].vector.normalized()))
        u,v=uv[loop].uv;group['uv'].append([round(u,8),round(1-v,8)]);group['indices'].append(len(group['indices']))
result={'format':2,'source':'apache.blend','inspiration':['https://www.boeing.com/defense/military-rotorcraft/ah-64-apache','https://www.army.mod.uk/learn-and-explore/equipment/aircraft/ah-64e-apache-attack-helicopter/'],'meshes':{},'groups':[]}
for name,finish,joint in GROUPS:
    assert (finish,joint) in groups,(name,finish,joint)
    result['meshes'][name]=groups[(finish,joint)];result['groups'].append({'shape':name,'finish':finish,'joint':joint,'pivot':PIVOTS[joint]})
(OUT/'apache_geometry.json').write_text(json.dumps(result,separators=(',',':')))
print('APACHE_GEOMETRY_READY',sum(len(m['indices'])//3 for m in result['meshes'].values()),flush=True)
# Neutral atlas underlay + identity masks; AI artwork is applied after shape review.
scene=bpy.context.scene;scene.render.engine='CYCLES';scene.cycles.device='CPU';scene.cycles.samples=1;scene.render.threads_mode='FIXED';scene.render.threads=4;scene.render.bake.margin=4
for suffix in ['source_albedo','material_data']:
    image=bpy.data.images.new('Apache '+suffix,2048,2048,alpha=True);image.colorspace_settings.name='sRGB' if suffix=='source_albedo' else 'Non-Color';image.filepath_raw=str(OUT/(suffix+'.png'));image.file_format='PNG'
    for finish,mat in MATS.items():
        tree=mat.node_tree;output=tree.nodes.get('Material Output');emit=tree.nodes.new('ShaderNodeEmission')
        if suffix=='source_albedo':emit.inputs['Color'].default_value=COLOURS[finish]
        else:
            rough=.64 if finish=='AircraftPaint' else .36;metal=.02 if finish=='AircraftPaint' else .85
            if finish in {'Rubber','Dark','AircraftGlass'}:rough=.92 if finish=='Rubber' else .5;metal=.04
            emit.inputs['Color'].default_value=(rough,metal,float(finish=='AircraftPaint'),1)
        tree.links.new(emit.outputs[0],output.inputs['Surface']);target=tree.nodes.new('ShaderNodeTexImage');target.image=image;tree.nodes.active=target
    bpy.ops.object.bake(type='EMIT');image.save()
for finish,mat in MATS.items():
    tree=mat.node_tree;tree.links.new(tree.nodes.get('Principled BSDF').outputs[0],tree.nodes.get('Material Output').inputs['Surface'])
scene.view_settings.view_transform='AgX';scene.world.use_nodes=True;scene.world.node_tree.nodes['Background'].inputs[0].default_value=(.35,.4,.48,1);scene.world.node_tree.nodes['Background'].inputs[1].default_value=.45
bpy.ops.object.light_add(type='SUN');sun=bpy.context.object;sun.data.energy=3;sun.data.angle=.05;sun.rotation_euler=Vector((-1,-1,-1.7)).to_track_quat('-Z','Y').to_euler()
bpy.ops.object.camera_add();camera=bpy.context.object;camera.data.type='ORTHO';scene.camera=camera
scene.cycles.samples=20;scene.cycles.use_denoising=True;scene.render.resolution_x=1200;scene.render.resolution_y=850;scene.render.resolution_percentage=100
for name,direction in [('shape-review',(1,-1,1.3)),('side-review',(1,0,.18))]:
    center=Vector((0,.5,1));camera.location=center+Vector(direction).normalized()*20;camera.rotation_euler=(center-camera.location).to_track_quat('-Z','Y').to_euler();camera.data.ortho_scale=7.2
    scene.render.filepath=str(OUT/(name+'.png'));bpy.ops.render.render(write_still=True)
for screen in bpy.data.screens:
    for area in screen.areas:
        if area.type=='VIEW_3D':area.spaces.active.region_3d.view_location=(0,.6,1);area.spaces.active.region_3d.view_distance=8
bpy.ops.wm.save_as_mainfile(filepath=str(OUT/'apache.blend'))
runpy.run_path(str(OUT/'tools/fit_tail_rotor.py'))
runpy.run_path(str(OUT/'tools/fit_tail_rotor_blend.py'))
print('APACHE_AUTHORING_READY',flush=True)
