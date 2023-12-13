import bpy
import sys
import os
import re
import gzip
import pickle
import argparse
from glob import glob
from mathutils import Vector
from pathlib import Path
import numpy as np
import itertools as ITT
from pprint import pprint
import subprocess as SP
import bmesh
from bpy.props import StringProperty, BoolProperty
from bpy_extras.io_utils import ImportHelper
from bpy.types import Operator
from . import arrange_nodes

# from .ScraplandTool import *

cmdline = None
if "--" in sys.argv:
    args = sys.argv[sys.argv.index("--") + 1 :]
    parser = argparse.ArgumentParser()
    parser.add_argument("--save", action="store_true")
    parser.add_argument("file_list", nargs="+")
    cmdline = parser.parse_args(args)


def node_arrange(obj):
    ctx = bpy.context
    old_obj = ctx.view_layer.objects.active
    area = max(ctx.screen.areas, key=lambda a: a.width * a.height)
    area_type = area.type
    area_ui_type = area.ui_type
    area.type = "NODE_EDITOR"
    area.ui_type = "ShaderNodeTree"
    ctx.view_layer.objects.active = obj
    old_mat = obj.active_material
    for ms in obj.material_slots:
        obj.active_material = ms.material
        node_tree = obj.active_material.node_tree
        bpy.ops.wm.redraw_timer(type="DRAW_WIN", iterations=1)
        arrange_nodes.run(node_tree)
    obj.active_material = old_mat
    ctx.view_layer.objects.active = old_obj
    area.type = area_type
    area.ui_type = area_ui_type


class ScrapImporter(object):
    def __init__(self, handle, *, options=None, context=None):
        self.context = context or bpy.context
        self.unhandled = set()
        self.options = options or {}
        self.model_scale = 1000.0
        self.spawn_pos = {}
        self.objects = {}
        self.handle = handle
        data = handle.parse_file(handle.pwd())
        self.path = data.pop("path")
        self.config = data.pop("config")
        self.dummies = data.pop("dummies")["dummies"]
        self.moredummies = data.pop("moredummies")
        self.emi = data.pop("emi")
        self.sm3 = data.pop("sm3")
        self.deps = data.pop("dependencies")

    def load_texture(self, path):
        data = self.handle.read_file(path)
        img = bpy.data.images.new(path, 0, 0)
        img.pack(data=data, data_len=len(data))
        img.name = path
        img.source = "FILE"
        return img

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
        self.dummies = [d for d in self.dummies if not d["name"].startswith("DM_Track")]
        for name, points in points.items():
            crv = bpy.data.curves.new(name, "CURVE")
            crv.dimensions = "3D"
            crv.path_duration = (
                bpy.context.scene.frame_end - bpy.context.scene.frame_start
            ) + 1
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
        for sm3 in self.sm3:
            if not sm3:
                continue
            for node in sm3.get("scene", {}).get("nodes", []):
                node_name = node["name"]
                node_path = node["name"]
                node_info = node["info"]
                node = node.get("content") or {}
                node_type = node.get("type", "<Unknown>")
                print(f"[{node_type}|{node_path}]: {node_name} {node_info}")
                # if not node:
                #     continue
                # if node["type"] == "Camera":
                #     # print(f"CAM:{node_name}")
                #     pprint(node)
                # elif node["type"] == "Light":
                #     # print(f"LIGHT:{node_name}")
                #     # self.add_light(node_name, node)

    def run(self):
        self.import_emi()
        self.join_objects(self.options["merge_objects"])
        if self.options["create_tracks"]:
            self.create_tracks()
        if self.options["create_dummies"]:
            self.create_dummies()
        if self.options["create_nodes"]:
            self.create_nodes()
        if self.unhandled:
            print("Unhandled textures:", self.unhandled)
        elements = set()
        for dummy in self.dummies:
            if dummy.get("name", "DM_").startswith("DM_Element"):
                print(dummy["name"], dummy["info"])
                name = dummy["name"].split("_", 2)[-1].split("_")[0]
                elements.add(name)
        elements = sorted(elements)
        print(f"Element: {elements}")

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
                    objs[0].name = name
                else:
                    coll = bpy.data.collections.new(name)
                    bpy.context.scene.collection.children.link(coll)
                    for n, obj in enumerate(objs):
                        obj.name = f"{name}_{n:04}"
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

    def get_input(self, node, name, dtype):
        return list(filter(lambda i: (i.type, i.name) == (dtype, name), node.inputs))

    def build_material(self, mat_key, mat_def, map_def):
        mat_props = dict(
            m.groups() for m in re.finditer(r"\(\+(\w+)(?::(\w*))?\)", mat_key)
        )
        for k, v in mat_props.items():
            mat_props[k] = v or True
        skip_names = ["entorno", "noise_trazado", "noise128", "pbasicometal"]
        overrides = {
            "zonaautoiluminada-a.dds": {
                # "light"
            },
            "flecha.000.dds": {"shader": "hologram"},
            "mayor.000.dds": {"shader": "hologram"},
        }
        settings = {
            "Emission Strength": 10.0,
            "Emission Color": (0.0, 0.0, 0.0, 1.0),
            "Specular IOR Level": 0.0,
            "Roughness": 0.0,
            "Metallic": 0.0,
        }
        transparent_settings = {
            "Transmission Weight": 1.0,
            "IOR": 1.0,
        }
        glass_settings = {
            "Base Color": (0.8, 0.8, 0.8, 1.0),
            "Metallic": 0.2,
            "Specular IOR Level": 0.2,
        }
        tex_slot_map = {
            "base": "Base Color",
            "metallic": "Metallic",
            "unk_1": None,  # "Clearcoat" ? env map?
            "bump": "Normal",
            "glow": "Emission Color",
        }

        mat = bpy.data.materials.new(mat_key)
        mat.use_nodes = True
        node_tree = mat.node_tree
        nodes = node_tree.nodes
        for n in nodes:
            nodes.remove(n)
        imgs = {}
        animated_textures = {}
        is_transparent = True
        for slot, tex in mat_def["maps"].items():
            if tex is None:
                continue
            tex_file = self.deps.get(tex["texture"])
            if tex_file is None:
                print("Unresolved dependency: {}".format(tex["texture"]))
                continue
            slot = tex_slot_map.get(slot)
            if slot is None:
                self.unhandled.add(tex_file)
                print(f"Don't know what to do with {tex_file}")
                continue
            if not (tex and tex_file):
                continue
            tex_name = tex_file.split("/")[-1].lower()
            mat_props.update(overrides.get(tex_name, {}))
            if any(tex_name.find(fragment) != -1 for fragment in skip_names):
                continue
            else:
                is_transparent = False
            imgs[slot] = self.load_texture(tex_file)
        out = nodes.new("ShaderNodeOutputMaterial")
        out.name = "Output"
        shader = nodes.new("ShaderNodeBsdfPrincipled")
        is_transparent |= mat_props.get("shader") == "glass"
        is_transparent |= mat_props.get("transp") in {"premult", "filter"}
        if is_transparent:
            settings.update(transparent_settings)
        if mat_props.get("shader") == "glass":
            settings.update(glass_settings)
        for name, value in settings.items():
            shader.inputs[name].default_value = value
        lmaps = []
        for lightmap in map_def[1:]:
            continue  # TODO: handle lightmaps properly
            if not lightmap:
                continue
            img_node = nodes.new("ShaderNodeTexImage")
            img_node.name = lightmap
            img_node.image = self.load_texture(lightmap)
            uv_coords = nodes.new("ShaderNodeUVMap")
            uv_coords.uv_map = "lightmap"
            node_tree.links.new(uv_coords.outputs["UV"], img_node.inputs["Vector"])
            tex_mix_node = nodes.new("ShaderNodeMixRGB")
            tex_mix_node.blend_type = "MULTIPLY"
            tex_mix_node.inputs["Fac"].default_value = 0.0
            node_tree.links.new(
                img_node.outputs["Color"], tex_mix_node.inputs["Color1"]
            )
            node_tree.links.new(
                img_node.outputs["Alpha"], tex_mix_node.inputs["Color2"]
            )
            lmaps.append(tex_mix_node)

        for socket, img in imgs.items():
            uv_coords = nodes.new("ShaderNodeUVMap")
            uv_coords.uv_map = "default"
            img_node = nodes.new("ShaderNodeTexImage")
            img_node.name = img.name
            img_node.image = img
            node_tree.links.new(uv_coords.outputs["UV"], img_node.inputs["Vector"])
            if socket in animated_textures:
                img.source = "SEQUENCE"
                num_frames = animated_textures[socket]
                fps_div = 2  # TODO: read from .emi file
                drv = img_node.image_user.driver_add("frame_offset")
                drv.driver.type = "SCRIPTED"
                drv.driver.expression = f"((frame/{fps_div})%{num_frames})-1"
                img_node.image_user.frame_duration = 1
                img_node.image_user.use_cyclic = True
                img_node.image_user.use_auto_refresh = True
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
                normal_map.inputs["Strength"].default_value = 1.0
            node_tree.links.new(output_node, shader.inputs[socket])
        shader_out = shader.outputs["BSDF"]
        if imgs and mat_props.get("shader") == "hologram":
            mix_shader = nodes.new("ShaderNodeMixShader")
            transp_shader = nodes.new("ShaderNodeBsdfTransparent")
            mix_in_1 = self.get_input(mix_shader, "Shader", "SHADER")[0]
            mix_in_2 = self.get_input(mix_shader, "Shader", "SHADER")[1]
            node_tree.links.new(transp_shader.outputs["BSDF"], mix_in_1)
            node_tree.links.new(shader.outputs["BSDF"], mix_in_2)
            node_tree.links.new(
                imgs["Base Color"].outputs["Color"], mix_shader.inputs["Fac"]
            )
            node_tree.links.new(
                imgs["Base Color"].outputs["Color"], shader.inputs["Emission Color"]
            )
            shader.inputs["Emission Strength"].default_value = 50.0
            shader_out = mix_shader.outputs["Shader"]
        if imgs and settings.get("Transmission", 0.0) > 0.0:
            light_path = nodes.new("ShaderNodeLightPath")
            mix_shader = nodes.new("ShaderNodeMixShader")
            transp_shader = nodes.new("ShaderNodeBsdfTransparent")
            mix_in_1 = self.get_input(mix_shader, "Shader", "SHADER")[0]
            mix_in_2 = self.get_input(mix_shader, "Shader", "SHADER")[1]
            node_tree.links.new(shader.outputs["BSDF"], mix_in_1)
            node_tree.links.new(transp_shader.outputs["BSDF"], mix_in_2)
            node_tree.links.new(
                light_path.outputs["Is Shadow Ray"], mix_shader.inputs["Fac"]
            )
            if (
                mat_props.get("transp") == "filter"
                or mat_props.get("shader") == "glass"
            ):
                node_tree.links.new(
                    imgs["Base Color"].outputs["Color"], transp_shader.inputs["Color"]
                )
            shader_out = mix_shader.outputs["Shader"]
        node_tree.links.new(shader_out, out.inputs["Surface"])
        return mat

    def apply_maps(self, ob, m_mat, m_map):
        mat_key, m_mat = m_mat
        map_key, m_map = m_map
        map_1, map_2 = None, None
        m_map = m_map or [None, None, None]
        map_1 = self.deps.get(m_map[0])
        map_2 = self.deps.get(m_map[2])
        if mat_key == 0:
            return
        m_map = (m_map[1], map_1, map_2)
        mat_name = m_mat.get("name", f"MAT:{mat_key:08X}|{map_key:08X}")
        if mat_name not in bpy.data.materials:
            ob.active_material = self.build_material(mat_name, m_mat, m_map)
            node_arrange(ob)
        else:
            ob.active_material = bpy.data.materials[mat_name]

    def create_mesh(self, name, verts, faces, m_mat, m_map):
        tex_layer_names = {0: "default", 1: "lightmap"}
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
                for tex_num, tex_coords in enumerate(tex):
                    tex_layer_name = tex_layer_names.get(tex_num, tex_coords)
                    if not vert[tex_coords]:
                        continue
                    if not tex_layer_name in uvlayers:
                        uvlayers[tex_layer_name] = me.uv_layers.new(name=tex_layer_name)
                    u, v = vert[tex_coords]
                    uvlayers[tex_layer_name].data[loop_idx].uv = (u, 1.0 - v)
        bm = bmesh.new()
        bm.from_mesh(me)
        if self.options["remove_dup_verts"]:
            bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=0.0001)
        bm.to_mesh(me)
        me.update(calc_edges=True)
        bm.clear()
        for poly in me.polygons:
            poly.use_smooth = True
        ob = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(ob)
        self.apply_maps(ob, m_mat, m_map)
        self.objects.setdefault(name.split("(")[0], []).append(ob)
        return ob


# if __name__ == "__main__":
#     if cmdline is None or not cmdline.file_list:
#         register()
#         bpy.ops.scrap_utils.import_pickle("INVOKE_DEFAULT")
#     else:
#         for file in cmdline.file_list:
#             bpy.context.preferences.view.show_splash = False
#             objs = bpy.data.objects
#             for obj in objs.keys():
#                 objs.remove(objs[obj], do_unlink=True)
#             cols = bpy.data.collections
#             for col in cols:
#                 cols.remove(col)
#             importer = ScrapImporter(file)
#             importer.run()
#             if cmdline.save:
#                 targetpath = Path(file).name.partition(".")[0] + ".blend"
#                 targetpath = os.path.abspath(targetpath)
#                 print("Saving", targetpath)
#                 bpy.ops.wm.save_as_mainfile(filepath=targetpath)
