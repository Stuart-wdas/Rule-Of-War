"""Export the six authored Blender surfaces for the native renderer.

blender --background assets/aircraft/a10/a10.blend --python <this script>
Also runnable from the packed EXPORT_A10.py text in the saved Blender file.
"""
import json
from pathlib import Path
import bpy
from mathutils import Vector

SHAPES = ("A10Body", "A10Canopy", "A10Wing", "A10Tail", "A10Fin", "JetNacelle")


def native(v):
    return [round(v.x, 7), round(v.z, 7), round(-v.y, 7)]


def export():
    root = Path(bpy.data.filepath).parent
    output = {"format": 1, "source": "a10.blend", "meshes": {}}
    deps = bpy.context.evaluated_depsgraph_get()
    for name in SHAPES:
        obj = next(o for o in bpy.data.objects if o.get("runtime_shape") == name)
        evaluated = obj.evaluated_get(deps)
        mesh = evaluated.to_mesh()
        mesh.calc_loop_triangles()
        local = name in ("A10Fin", "JetNacelle")
        transform = obj.matrix_world.copy()
        normal_matrix = transform.to_3x3().inverted().transposed()
        positions, normals, indices, uv = [], [], [], []
        # Export split corner normals so thin wing edges stay crisp.
        for triangle in mesh.loop_triangles:
            for loop_index in triangle.loops:
                loop = mesh.loops[loop_index]
                v = mesh.vertices[loop.vertex_index].co
                n = mesh.corner_normals[loop_index].vector
                if not local:
                    v, n = transform @ v, normal_matrix @ n
                assert all(abs(c) < 100 for c in v)
                assert n.length > .9
                positions.append(native(v))
                normals.append(native(n.normalized()))
                indices.append(len(indices))
                uv.append([round(c, 7) for c in mesh.uv_layers.active.data[loop_index].uv])
        output["meshes"][name] = dict(vertices=positions, normals=normals, indices=indices, uv=uv)
        evaluated.to_mesh_clear()
    # Attachment controls remain editable in Blender and travel with the export.
    output["engine_mount"] = native(bpy.data.objects["Engine.R"].location)
    output["fin_mount"] = native(bpy.data.objects["Fin.R"].location)
    output["canopy_stations"] = json.loads(bpy.context.scene["canopy_stations"])
    destination = root / "a10_geometry.json"
    destination.write_text(json.dumps(output, separators=(",", ":")), encoding="utf-8")
    print("A10_EXPORTED", destination, sum(len(m["indices"]) // 3 for m in output["meshes"].values()), "surface triangles", flush=True)


if __name__ == "__main__":
    export()
