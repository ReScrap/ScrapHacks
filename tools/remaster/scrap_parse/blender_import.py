import bpy
import sys
import os
import re
import json
import gzip
import argparse
import shutil
from glob import glob
from mathutils import Vector
from pathlib import Path
import numpy as np
import itertools as ITT
from pprint import pprint
import bmesh
from bpy.props import StringProperty, BoolProperty
from bpy_extras.io_utils import ImportHelper
from bpy.types import Operator

cmdline = None
if "--" in sys.argv:
    args = sys.argv[sys.argv.index("--") + 1 :]
    parser = argparse.ArgumentParser()
    parser.add_argument("--save", action="store_true")
    parser.add_argument("file_list", nargs="+")
    cmdline = parser.parse_args(args)


def fix_pos(xyz):
    x, y, z = xyz
    return x, z, y


class ScrapImporter(object):
    def __init__(self, options):
        self.unhandled = set()
        filepath = options.pop("filepath")
        self.options = options
        self.model_scale = 1000.0
        self.spawn_pos = {}
        self.objects = {}
        print("Loading", filepath)
        with gzip.open(filepath, "r") as fh:
            data = json.load(fh)
        self.path = data.pop("path")
        self.root = data.pop("root")
        self.config = data.pop("config")
        self.dummies = data.pop("dummies")["DUM"]["dummies"]
        self.moredummies = data.pop("moredummies")
        self.emi = data.pop("emi")["EMI"]
        self.sm3 = data.pop("sm3")["SM3"]

    def make_empty(self, name, pos, rot=None):
        empty = bpy.data.objects.new(name, None)
        empty.empty_display_type = "PLAIN_AXES"
        empty.empty_display_size = 0.1
        empty.location = Vector(pos).xzy / self.model_scale
        if rot:
            empty.rotation_euler = Vector(rot).xzy
        empty.name = name
        empty.show_name = True
        bpy.context.scene.collection.objects.link(empty)

    def create_tracks(self):
        points = {}
        for dummy in self.dummies:
            if dummy["name"].startswith("DM_Track"):
                try:
                    _, name, idx = dummy["name"].split("_")
                except ValueError:
                    continue
                pos = Vector(dummy["pos"]).xzy / self.model_scale
                points.setdefault(name, []).append((int(idx), pos))
        self.dummies=[d for d in self.dummies if not d["name"].startswith("DM_Track")]
        for name, points in points.items():
            crv = bpy.data.curves.new(name, "CURVE")
            crv.dimensions = "3D"
            crv.path_duration = (
                (bpy.context.scene.frame_end - bpy.context.scene.frame_start) + 1
            )
            crv.twist_mode = "Z_UP"
            crv.twist_smooth = 1.0
            spline = crv.splines.new(type="NURBS")
            spline.points.add(len(points) - 1)
            spline.use_endpoint_u = True
            spline.use_cyclic_u = True
            spline.use_endpoint_v = True
            spline.use_cyclic_v = True
            points.sort()
            for p, (_, co) in zip(spline.points, points):
                p.co = list(co) + [1.0]
            obj = bpy.data.objects.new(name, crv)
            bpy.context.scene.collection.objects.link(obj)

    def create_dummies(self):
        for dummy in self.dummies:
            self.make_empty(dummy["name"], dummy["pos"], dummy["rot"])
            if dummy["name"].startswith("DM_Player_Spawn"):
                self.spawn_pos[dummy["name"]] = dummy["pos"]
        for name, dummy in self.moredummies.items():
            if not "Pos" in dummy:
                continue
            pos = list(float(v) for v in dummy["Pos"])
            rot = [0, 0, 0]
            if "Rot" in dummy:
                rot = list(float(v) for v in dummy["Rot"])
            self.make_empty(name, pos, rot)

    def add_light(self, name, node):
        light = bpy.data.lights.new(name, "POINT")
        light.energy = 100
        r = node["unk_10"][0] / 255  # *(node['unk_10'][3]/255)
        g = node["unk_10"][1] / 255  # *(node['unk_10'][3]/255)
        b = node["unk_10"][2] / 255  # *(node['unk_10'][3]/255)
        light.color = (r, g, b)
        light = bpy.data.objects.new(name, light)
        light.location = Vector(node["pos"]).xzy / self.model_scale
        light.rotation_euler = Vector(node["rot"]).xzy
        bpy.context.scene.collection.objects.link(light)

    def create_nodes(self):
        for node in self.sm3["scene"]["nodes"]:
            node_name = node["name"]
            node = node.get("content", {})
            if not node:
                continue
            if node["type"] == "Camera":
                print(f"CAM:{node_name}")
                pprint(node)
            elif node["type"] == "Light":
                print(f"LIGHT:{node_name}")
                # self.add_light(node_name, node)

    def run(self):
        self.import_emi()
        self.join_objects(self.options['merge_objects'])
        if self.options['create_tracks']:
            self.create_tracks()
        if self.options['create_dummies']:
            self.create_dummies()
        if self.options['create_nodes']:
            self.create_nodes()
        if self.unhandled:
            print("Unhandled textures:",self.unhandled)

    def join_objects(self, do_merge=False):
        bpy.ops.object.select_all(action="DESELECT")
        ctx = {}
        for name, objs in self.objects.items():
            if len(objs) <= 1:
                continue
            ctx = {
                "active_object": objs[0],
                "object": objs[0],
                "selected_objects": objs,
                "selected_editable_objects": objs,
            }
            with bpy.context.temp_override(**ctx):
                if do_merge:
                    bpy.ops.object.join()
                    objs[0].name=name
                else:
                    coll=bpy.data.collections.new(name)
                    bpy.context.scene.collection.children.link(coll)
                    for n,obj in enumerate(objs):
                        obj.name=f"{name}_{n:04}"
                        coll.objects.link(obj)
            bpy.ops.object.select_all(action="DESELECT")

    def import_emi(self):
        mats = {0: None}
        maps = {0: None}
        for mat in self.emi["materials"]:
            mats[mat[0]] = mat[1]
        for tex_map in self.emi["maps"]:
            maps[tex_map["key"]] = tex_map["data"]
        for tri in self.emi["tri"]:
            name = tri["name"]
            if tri["data"]:
                tris = tri["data"]["tris"]
                for n, verts in enumerate(
                    [tri["data"]["verts_1"], tri["data"]["verts_2"]], 1
                ):
                    if not (tris and verts):
                        continue
                    self.create_mesh(
                        name=f"{name}_{n}",
                        verts=verts,
                        faces=tris,
                        m_map=(tri["data"]["map_key"], maps[tri["data"]["map_key"]]),
                        m_mat=(tri["data"]["mat_key"], mats[tri["data"]["mat_key"]]),
                    )

    def normalize_path(self, path):
        return path.replace("\\", os.sep).replace("/", os.sep)

    def resolve_path(self, path):
        file_extensions = [".png", ".bmp", ".dds", ".tga", ".alpha.dds"]
        root_path = Path(self.normalize_path(self.root).lower())
        start_folder = Path(self.normalize_path(self.path).lower()).parent
        try:
            texture_path = Path(self.config["model"]["texturepath"] + "/")
        except KeyError:
            texture_path = None
        path = Path(path.replace("/", os.sep).lower())
        if texture_path:
            folders = ITT.chain(
                [start_folder],
                start_folder.parents,
                [texture_path],
                texture_path.parents,
            )
        else:
            folders = ITT.chain([start_folder], start_folder.parents)
        for folder in folders:
            for suffix in file_extensions:
                for dds in [".", "dds"]:
                    resolved_path = (
                        root_path / folder / path.parent / dds / path.name
                    ).with_suffix(suffix)
                    if resolved_path.exists():
                        return str(resolved_path)
        print(f"Failed to resolve {path}")
        return None

    def get_input(self, node, name, dtype):
        return list(filter(lambda i: (i.type, i.name) == (dtype, name), node.inputs))


    def build_material(self, mat_key, mat_def):
        mat_props = dict(m.groups() for m in re.finditer(r"\(\+(\w+)(?::(\w*))?\)",mat_key))
        for k,v in mat_props.items():
            mat_props[k]=v or True
        skip_names = ["entorno", "noise_trazado", "noise128", "pbasicometal"]
        overrides = {
            "zonaautoiluminada-a.dds" : {
                # "light"
            },
            "flecha.000.dds": {
                "shader": "hologram"
            },
            "mayor.000.dds": {
                "shader": "hologram"
            },
        }
        settings = {
            "Emission Strength": 10.0,
            "Specular": 0.0,
            "Roughness": 0.0,
            "Metallic": 0.0,
        }
        transparent_settings = {
            "Transmission": 1.0,
            "Transmission Roughness": 0.0,
            "IOR": 1.0,
        }
        glass_settings = {
            "Base Color": ( .8, .8, .8, 1.0),
            "Metallic": 0.2,
            "Roughness": 0.0,
            "Specular": 0.2,
        }
        tex_slots=[
            "Base Color",
            "Metallic",
            None, # "Clearcoat" ? env map?
            "Normal",
            "Emission"
        ]

        mat = bpy.data.materials.new(mat_key)
        mat.use_nodes = True
        node_tree = mat.node_tree
        nodes = node_tree.nodes
        imgs = {}
        animated_textures={}
        is_transparent = True
        for slot,tex in zip(tex_slots,mat_def["maps"]):
            if (slot is None)  and tex:
                self.unhandled.add(tex["texture"])
                print(f"Don't know what to do with {tex}")
            if not (tex and slot):
                continue
            tex_file = self.resolve_path(tex["texture"])
            if tex_file is None:
                continue
            tex_name = os.path.basename(tex_file)
            if ".000." in tex_name:
                tex_files=glob(tex_file.replace(".000.",".*."))
                num_frames=len(tex_files)
                animated_textures[slot]=num_frames
            mat_props.update(overrides.get(tex_name,{}))
            if any(
                tex_name.find(fragment) != -1
                for fragment in skip_names
            ):
                continue
            else:
                is_transparent = False
            imgs[slot]=bpy.data.images.load(tex_file)
        for n in nodes:
            nodes.remove(n)
        out = nodes.new("ShaderNodeOutputMaterial")
        out.name = "Output"
        shader = nodes.new("ShaderNodeBsdfPrincipled")
        is_transparent|=mat_props.get("shader")=="glass"
        is_transparent|=mat_props.get("transp") in {"premult","filter"}
        if is_transparent:
            settings.update(transparent_settings)
        if mat_props.get("shader")=="glass":
            settings.update(glass_settings)
        for name, value in settings.items():
            shader.inputs[name].default_value = value
        sockets_used = set()
        for socket,img in imgs.items():
            img_node = nodes.new("ShaderNodeTexImage")
            img_node.name = img.name
            img_node.image = img
            if socket in animated_textures:
                img.source="SEQUENCE"
                num_frames=animated_textures[socket]
                fps_div = 2 # TODO: read from .emi file
                drv=img_node.image_user.driver_add("frame_offset")
                drv.driver.type="SCRIPTED"
                drv.driver.expression=f"((frame/{fps_div})%{num_frames})-1"
                img_node.image_user.frame_duration=1
                img_node.image_user.use_cyclic=True
                img_node.image_user.use_auto_refresh=True
            tex_mix_node = nodes.new("ShaderNodeMixRGB")
            tex_mix_node.blend_type = "MULTIPLY"
            tex_mix_node.inputs["Fac"].default_value = 0.0
            node_tree.links.new(
                img_node.outputs["Color"], tex_mix_node.inputs["Color1"]
            )
            node_tree.links.new(
                img_node.outputs["Alpha"], tex_mix_node.inputs["Color2"]
            )
            imgs[socket] = tex_mix_node
            output_node = tex_mix_node.outputs["Color"]
            print(img.name, "->", socket)
            if socket == "Normal":
                normal_map = nodes.new("ShaderNodeNormalMap")
                node_tree.links.new(output_node, normal_map.inputs["Color"])
                output_node = normal_map.outputs["Normal"]
                normal_map.inputs["Strength"].default_value = 0.4
            node_tree.links.new(output_node, shader.inputs[socket])
        shader_out=shader.outputs["BSDF"]
        if mat_props.get("shader")=="hologram":
            mix_shader = nodes.new("ShaderNodeMixShader")
            transp_shader = nodes.new("ShaderNodeBsdfTransparent")
            mix_in_1 = self.get_input(mix_shader,"Shader","SHADER")[0]
            mix_in_2 = self.get_input(mix_shader,"Shader","SHADER")[1]
            node_tree.links.new(transp_shader.outputs["BSDF"], mix_in_1)
            node_tree.links.new(shader.outputs["BSDF"], mix_in_2)
            node_tree.links.new(imgs["Base Color"].outputs["Color"],mix_shader.inputs["Fac"])
            node_tree.links.new(imgs["Base Color"].outputs["Color"],shader.inputs["Emission"])
            shader.inputs["Emission Strength"].default_value=50.0
            shader_out=mix_shader.outputs["Shader"]
        if settings.get("Transmission",0.0)>0.0:
            light_path = nodes.new("ShaderNodeLightPath")
            mix_shader = nodes.new("ShaderNodeMixShader")
            transp_shader = nodes.new("ShaderNodeBsdfTransparent")
            mix_in_1 = self.get_input(mix_shader,"Shader","SHADER")[0]
            mix_in_2 = self.get_input(mix_shader,"Shader","SHADER")[1]
            node_tree.links.new(shader.outputs["BSDF"], mix_in_1)
            node_tree.links.new(transp_shader.outputs["BSDF"], mix_in_2)
            node_tree.links.new(light_path.outputs["Is Shadow Ray"], mix_shader.inputs["Fac"])
            if mat_props.get("transp")=="filter" or mat_props.get("shader")=="glass":
                node_tree.links.new(imgs["Base Color"].outputs["Color"],transp_shader.inputs["Color"])
            shader_out=mix_shader.outputs["Shader"]
        node_tree.links.new(shader_out, out.inputs["Surface"])
        return mat

    def apply_maps(self, ob, m_mat, m_map):
        mat_key, m_mat = m_mat
        map_key, m_map = m_map  # TODO?: MAP
        if mat_key == 0:
            return
        mat_name = m_mat.get("name", f"MAT:{mat_key:08X}")
        map_name = f"MAP:{map_key:08X}"
        if mat_name not in bpy.data.materials:
            ob.active_material = self.build_material(mat_name, m_mat)
        else:
            ob.active_material = bpy.data.materials[mat_name]

    def create_mesh(self, name, verts, faces, m_mat, m_map):
        if not verts["inner"]:
            return
        me = bpy.data.meshes.new(name)
        me.use_auto_smooth = True
        pos = np.array([Vector(v["xyz"]).xzy for v in verts["inner"]["data"]])
        pos /= self.model_scale
        me.from_pydata(pos, [], faces)
        normals = [v["normal"] for v in verts["inner"]["data"]]
        vcols = [v["diffuse"] for v in verts["inner"]["data"]]
        if all(normals):
            normals = np.array(normals)
            me.vertices.foreach_set("normal", normals.flatten())
        if not me.vertex_colors:
            me.vertex_colors.new()
        for (vcol, vert) in zip(vcols, me.vertex_colors[0].data):
            vert.color = [vcol["r"], vcol["g"], vcol["b"], vcol["a"]]
        uvlayers = {}
        tex = [f"tex_{n+1}" for n in range(8)]
        for face in me.polygons:
            for vert_idx, loop_idx in zip(face.vertices, face.loop_indices):
                vert = verts["inner"]["data"][vert_idx]
                for tex_name in tex:
                    if not vert[tex_name]:
                        continue
                    if not tex_name in uvlayers:
                        uvlayers[tex_name] = me.uv_layers.new(name=tex_name)
                    u, v = vert[tex_name]
                    uvlayers[tex_name].data[loop_idx].uv = (u, 1.0 - v)
        bm = bmesh.new()
        bm.from_mesh(me)
        if self.options['remove_dup_verts']:
            bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=0.0001)
        bm.to_mesh(me)
        me.update(calc_edges=True)
        bm.clear()
        for poly in me.polygons:
            poly.use_smooth = True
        ob = bpy.data.objects.new(name, me)
        self.apply_maps(ob, m_mat, m_map)
        bpy.context.scene.collection.objects.link(ob)
        self.objects.setdefault(name, []).append(ob)
        return ob


class Scrap_Load(Operator, ImportHelper):

    bl_idname = "scrap_utils.import_json"
    bl_label = "Import JSON"

    filename_ext = ".json.gz"
    filter_glob: StringProperty(default="*.json.gz", options={"HIDDEN"})
    
    create_dummies: BoolProperty(
        name="Import dummies",
        default=True
    )

    create_nodes: BoolProperty(
        name="Import nodes (lights, cameras, etc)",
        default=True
    )

    create_tracks: BoolProperty(
            name="Create track curves",
            default=True
    )

    merge_objects: BoolProperty(
            name="Merge objects by name",
            default=False
    )

    remove_dup_verts: BoolProperty(
            name="Remove overlapping vertices\nfor smoother meshes",
            default=True
    )

    # remove_dup_verts: BoolProperty(
    #         name="Remove overlapping vertices for smoother meshes",
    #         default=False
    # )


    def execute(self, context):
        bpy.ops.preferences.addon_enable(module = "node_arrange")
        bpy.ops.outliner.orphans_purge(do_recursive=True)
        importer = ScrapImporter(self.as_keywords())
        importer.run()
        dg = bpy.context.evaluated_depsgraph_get()
        dg.update()
        return {"FINISHED"}


def register():
    bpy.utils.register_class(Scrap_Load)


def unregister():
    bpy.utils.unregister_class(Scrap_Load)


if __name__ == "__main__":
    if cmdline is None or not cmdline.file_list:
        register()
        bpy.ops.scrap_utils.import_json("INVOKE_DEFAULT")
    else:
        for file in cmdline.file_list:
            bpy.context.preferences.view.show_splash = False
            objs = bpy.data.objects
            for obj in objs.keys():
                objs.remove(objs[obj], do_unlink=True)
            cols=bpy.data.collections
            for col in cols:
                cols.remove(col)
            importer = ScrapImporter(file)
            importer.run()
            if cmdline.save:
                targetpath = Path(file).name.partition(".")[0] + ".blend"
                targetpath = os.path.abspath(targetpath)
                print("Saving", targetpath)
                bpy.ops.wm.save_as_mainfile(filepath=targetpath)
