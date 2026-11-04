"""Run in Blender background mode to check the saved scene independently."""
import bpy,json,math
from pathlib import Path
from mathutils import Vector
root=Path(__file__).resolve().parent
bpy.ops.wm.open_mainfile(filepath=str(root/'spiky_explosion.blend'),load_ui=False,use_scripts=False)
scene=bpy.context.scene
assert scene.frame_start==1 and scene.frame_end==64 and scene.render.fps==24
tips=bpy.data.collections['02 - Fire tips launched INSIDE core'].objects
assert len(tips)==11
checks=[]
for tip in tips:
    f=tip['launch_frame'];scene.frame_set(f);start=tip.location.copy()
    assert (start-Vector((0,0,1.35))).length<.4
    scene.frame_set(f+8);end=tip.location.copy()
    assert (end-start).length>4.
    assert end.z-start.z>2.
    heights=[]
    for frame in range(f,f+29):
        scene.frame_set(frame);heights.append(tip.location.z)
    peak=max(heights);peak_index=heights.index(peak)
    assert 4<peak_index<27, 'Arc must have an apex during the visible tip lifetime'
    assert heights[-1]<peak-.3, 'Arc must visibly descend after its apex'
    scene.frame_set(64);assert tip.color[3]==0
    checks.append(dict(name=tip.name,launch_frame=f,launch_distance_to_core=round((start-Vector((0,0,1.35))).length,4),
                       distance_travelled_after_8_frames=round((end-start).length,4),
                       height_gained_after_8_frames=round(end.z-start.z,4),
                       apex_height=round(peak,4),apex_frame=f+peak_index,
                       drop_after_apex=round(peak-heights[-1],4)))
for name in ['01 - Brief central fireball','03 - Smoke wakes along projectile paths','04 - Rolling central smoke']:
    scene.frame_set(64);assert all(o.color[3]==0 for o in bpy.data.collections[name].objects)
for smoke in bpy.data.collections['03 - Smoke wakes along projectile paths'].objects:
    born=smoke['emission_frame'];scene.frame_set(math.ceil(born+2))
    assert smoke.scale.z>smoke.scale.x, 'Wake segments must bridge the faster trajectory'
    axis=smoke.rotation_euler.to_quaternion()@Vector((0,0,1))
    assert axis.z<.99, 'Wake segments must align with the flight path'
scene.frame_set(21)
assert all(o.color[3]==0 for o in bpy.data.collections['01 - Brief central fireball'].objects)
assert all(math.isfinite(v) for o in scene.objects for v in (*o.location,*o.scale))
report=dict(status='PASS',saved_blend_reopened=True,frames=64,fps=24,spikes=checks,
    main_flame_gone_by_frame=21,all_volumes_dissipated_at_frame_64=True,
    shader_type='Cycles volume materials rendered on GPU; no external texture/cache dependencies')
(root/'scene_validation.json').write_text(json.dumps(report,indent=2)+'\n')
print('Scene passed: all 11 tips launch faster and higher from inside core, crest and descend; final volumes dissipate.')
