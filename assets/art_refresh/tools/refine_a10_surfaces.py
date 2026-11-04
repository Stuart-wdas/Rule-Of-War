"""Increase curved airframe resolution while keeping the authored source editable."""
import bpy,runpy
from pathlib import Path
root=Path(bpy.data.filepath).parent
for obj in bpy.data.objects:
    if obj.get('runtime_shape') in ['A10Body','A10Canopy']:
        mod=obj.modifiers.get('Hero surface curvature') or obj.modifiers.new('Hero surface curvature','SUBSURF')
        mod.subdivision_type='CATMULL_CLARK';mod.levels=1;mod.render_levels=1
        mod.uv_smooth='PRESERVE_BOUNDARIES'
        # Hold cap boundaries to preserve the authored nose/tail extrema.
        crease=obj.data.attributes.get('crease_edge') or obj.data.attributes.new('crease_edge','FLOAT','EDGE')
        caps={tuple(sorted((p.vertices[i],p.vertices[(i+1)%len(p.vertices)])))
              for p in obj.data.polygons if len(p.vertices)>4 for i in range(len(p.vertices))}
        for edge in obj.data.edges:
            if tuple(sorted(edge.vertices)) in caps:crease.data[edge.index].value=1
runpy.run_path(str(root/'tools/export_a10.py'),run_name='__main__')
bpy.context.preferences.filepaths.save_version=0
bpy.ops.wm.save_as_mainfile(filepath=str(root/'a10.blend'))
