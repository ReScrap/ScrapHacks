from typing import Any
import bpy
import sys
import os
import string
from . import find_scrapland, MultiPack, level_import, find_packed
from bpy.types import Panel, UIList, PropertyGroup, Operator
from bpy.props import (
    StringProperty,
    BoolProperty,
    CollectionProperty,
    IntProperty,
)
from types import SimpleNamespace
from pprint import pprint

files_to_check = [
    ["Data.packed"],
    ["Bin", "Scrap.exe"],
]


class PackedFile(PropertyGroup):
    path: StringProperty()
    label: StringProperty()
    selected: BoolProperty()


class PackedEntry(PropertyGroup):
    path: StringProperty()
    size: IntProperty()
    is_file: BoolProperty()


def chdir(path: str | None = None):
    scene = bpy.context.scene
    handle = getattr(scene.ScrapTool, "handle")
    if handle is None:
        return
    if path:
        handle.cd(path)
    file_browser_items = scene.file_browser_items
    file_browser_items.clear()
    files = handle.ls()
    start_idx = 0
    if not all(file["path"].count("/") == 1 for file in files):
        prev_dir = file_browser_items.add()
        prev_dir.path = ".."
        prev_dir.size = 0
        prev_dir.is_file = False
    for file in files:
        file_item = file_browser_items.add()
        file_item.path = file["path"]
        file_item.size = file["size"]
        file_item.is_file = file["is_file"]


class FindPacked(Operator):
    """Find and list .packed files"""

    bl_idname = "scratool.find_packed"
    bl_label = "Find and list .packed files"
    bl_options = {"INTERNAL"}

    directory: StringProperty(
        name="Scrapland folder",
        description="Folder where Scrapland is installed",
        subtype="DIR_PATH",
        options={"HIDDEN"},
    )

    def execute(self, context):
        scene = bpy.context.scene
        if not os.path.isdir(self.directory):
            raise RuntimeError(f"{self.directory} is not a folder!")
        for file in files_to_check:
            file_path = os.path.join(self.directory, *file)
            if not os.path.isfile(file_path):
                raise RuntimeError(f"Sanity check: {file_path} does not exist!")
        packed_items = scene.packed_items
        packed_items.clear()
        for path in sorted(
            find_packed(self.directory), key=lambda p: (len(p.split("/")), p)
        ):
            item = packed_items.add()
            item.path = path
            item.label = os.path.relpath(path, self.directory).replace(os.sep, "/")
            split_path = item.label.split("/")
            item.selected = split_path[-1].startswith("data") and len(split_path) == 1
        self.report({"INFO"}, f"Found {len(packed_items)} Files")
        return {"FINISHED"}

    def invoke(self, context, event):
        self.directory = find_scrapland() or None
        context.window_manager.fileselect_add(self)
        return {"RUNNING_MODAL"}


class LoadPacked(Operator):
    """Import packed file"""

    bl_idname = "import_scene.packed"
    bl_label = "Import Scrapland .packed"
    bl_options = {"INTERNAL"}

    def execute(self, context):
        scene = bpy.context.scene
        files = [item.path for item in scene.packed_items if item.selected]
        context.scene.ScrapTool.handle = MultiPack(files)
        self.report({"INFO"}, f"Loaded {len(files)} Files")
        chdir()
        return {"FINISHED"}

    def invoke(self, context, event):
        return self.execute(context)


class ClosePacked(Operator):
    """Close packed file"""

    bl_idname = "scraptool.packed_close"
    bl_label = "Close Scrapland .packed"
    bl_options = {"INTERNAL"}

    def execute(self, context):
        context.scene.ScrapTool.handle = None
        return {"FINISHED"}

    def invoke(self, context, event):
        return self.execute(context)


class DataImporter(object):
    def __init__(self, data):
        self.deps = data.pop("dependencies", None)
        self.data = data
        self.model_scale = 1000.0

    def make_mesh(self, name, verts, faces, pos_offset):
        from mathutils import Vector
        import numpy as np
        import bmesh

        tex_layer_names = {0: "default", 1: "lightmap"}
        if not verts["inner"]:
            return
        pos_offset = Vector(pos_offset).xzy
        me = bpy.data.meshes.new(name)
        me.use_auto_smooth = True
        pos = np.array([Vector(v["xyz"]).xzy for v in verts["inner"]["data"]])
        # pos += pos_offset
        # pos *= 50
        pos /= self.model_scale
        me.from_pydata(pos, [], faces)
        normals = [v["normal"] for v in verts["inner"]["data"]]
        vcols = [v["diffuse"] for v in verts["inner"]["data"]]
        if all(normals):
            normals = np.array(normals)
            me.vertices.foreach_set("normal", normals.flatten())
        if not me.vertex_colors:
            me.vertex_colors.new()
        if all(vcols):
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
        bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=0.0001)
        bm.to_mesh(me)
        me.update(calc_edges=True)
        bm.clear()
        for poly in me.polygons:
            poly.use_smooth = True
        ob = bpy.data.objects.new(name, me)
        bpy.context.scene.collection.objects.link(ob)
        return ob

    def make_empty(self, name, pos, rot=None):
        from mathutils import Vector

        empty = bpy.data.objects.new(name, None)
        empty.empty_display_type = "PLAIN_AXES"
        empty.empty_display_size = 0.1
        empty.location = Vector(pos).xzy / self.model_scale
        if rot:
            empty.rotation_euler = Vector(rot).xzy
        empty.name = name
        empty.show_name = True
        bpy.context.scene.collection.objects.link(empty)
        return empty

    def import_SM3(self):
        from collections import Counter

        scene = self.data["scene"]
        print("Model:", scene["model_name"])
        print("Node:", scene["node_name"])
        print("Props:", scene["node_props"])
        pprint(scene["mat"])
        nodes = {"": self.make_empty("<ROOT>", (0, 0, 0))}  # TODO: handle node flags
        edges = []
        node_names = set()
        cnt = Counter()
        for node in scene["nodes"]:
            dont_render = any(f in node["flags"] for f in ("NO_RENDER", "HIDDEN"))
            print(
                repr(node["parent"]),
                "->",
                repr(node["name"]),
                "|",
                node["flags"],
                "|",
                node["info"],
            )
            content = node["content"] or {}
            if not content or content.get("type") != "Mesh":
                nodes[node["name"]] = self.make_empty(node["name"], node["pos_offset"])
            cnt[content.get("type")] += 1
            edges.append((node["name"], node["parent"]))
            while content:
                content["name"] = content.get("name", node.get("name", "<Unknown>"))
                print(
                    content.get("type"),
                    "|",
                    content.get("info"),
                )
                node_names.add(content["name"])
                if content.get("name") != node.get("name"):
                    print(content["name"], node["name"])
                if content.get("type") == "D3DMesh" and not dont_render:
                    nodes[node["name"]] = self.make_mesh(
                        node["name"],
                        content["verts"],
                        content["tris"]["tris"],
                        node["pos_offset"],
                    )
                    content = content.get("child", {})
                else:
                    content = None
        print(sorted(node_names))
        print(cnt)
        print(len(scene["nodes"]), "Nodes total")
        for k, v in edges:
            # nodes[k].parent=nodes[v]
            print(k, "->", v)

    def run(self):
        dtype = self.data.get("type", "UNKNOWN")
        func = getattr(self, "import_{}".format(dtype))
        if callable(func):
            return func()
        print(f"Don't know how to import data type {dtype}")


class HandleFile(Operator):
    bl_label = "Handle packed file entry"
    bl_idname = "scraptool.packed_handle_file"
    bl_options = {"INTERNAL", "REGISTER"}

    path: StringProperty()

    def execute(self, context):
        scene = bpy.context.scene
        print("PATH", self.path)
        for item in scene.file_browser_items:
            if item.path == self.path:
                break
        else:
            item = None
        if item is None:
            return {"FINISHED"}
        if not item.is_file:
            chdir(item.path)
            return {"FINISHED"}
        handle = None
        if hasattr(scene.ScrapTool, "handle"):
            handle = getattr(scene.ScrapTool, "handle")
        if handle is None:
            return {"FINISHED"}
        try:
            data = handle.parse_file(item.path) or {}
            print("T:", data.get("type"))
        except IOError as e:
            print(f"Error: {e}")
            print("Reading file without parsing")
            data = handle.read_file(item.path)
        if isinstance(data, bytes):
            p = bytes(string.printable, "utf8")
            if not all(c in p for c in data):
                return {"FINISHED"}
            text = bpy.data.texts.new(item.path.split("/")[-1])
            text.from_string(str(data, "utf8"))
            return {"FINISHED"}
        if not isinstance(data, dict):
            return {"FINISHED"}
        imp = DataImporter(data)
        imp.run()
        self.report({"INFO"}, f"Imported {item.path} Files")
        return {"FINISHED"}

    def invoke(self, context, event):
        return self.execute(context)


class LoadLevel(Operator):
    bl_label = "Load Level"
    bl_idname = "scraptool.load_level"
    bl_options = {"INTERNAL", "REGISTER"}

    create_dummies: BoolProperty(name="Import dummies", default=True)

    create_nodes: BoolProperty(name="Import nodes (lights, cameras, etc)", default=True)

    create_tracks: BoolProperty(name="Create track curves", default=True)

    merge_objects: BoolProperty(name="Merge objects by name", default=False)

    remove_dup_verts: BoolProperty(name="Smooth meshes (remove overlap)", default=True)

    def execute(self, context):
        scene = context.scene
        handle = None
        if hasattr(scene.ScrapTool, "handle"):
            handle = getattr(scene.ScrapTool, "handle")
        if handle is None:
            return {"FINISHED"}
        loader = level_import.ScrapImporter(
            handle, options=self.as_keywords(), context=context
        )
        loader.run()
        dg = bpy.context.evaluated_depsgraph_get()
        dg.update()
        return {"FINISHED"}

    def invoke(self, context, event):
        wm = context.window_manager
        return wm.invoke_props_dialog(self)


class FILEBROWSER_UL_packed_files(UIList):
    def draw_item(
        self, context, layout, data, item, icon, active_data, active_propname, index
    ):
        layout.prop(item, "selected", text=item.label, toggle=False)


class FILEBROWSER_UL_scrap_files(UIList):
    def draw_item(
        self, context, layout, data, item, icon, active_data, active_propname, index
    ):
        label = item.path.split("/")[-1]
        if not item.is_file:
            icon = "FILE_FOLDER"
        else:
            ext = label.split(".")[-1]
            icon = {
                "py": "FILE_SCRIPT",
                "pyc": "SCRIPT",
                "sm3": "FILE_3D",
                "dds": "FILE_IMAGE",
                "tga": "FILE_IMAGE",
                "bmp": "FILE_IMAGE",
                "cm3": "ANIM",
                "emi": "VIEW3D",
                "ogg": "FILE_SOUND",
                "ttf": "FILE_FONT",
                "mst": "RENDER_RESULT",
            }.get(ext, "QUESTION")
        op = layout.operator(HandleFile.bl_idname, text=label, icon=icon)
        op.path = item.path


class FILEBROWSER_PT_Scraptools(Panel):
    bl_label = "Scrapland Tools"
    bl_idname = "FILEBROWSER_PT_Scraptools"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "Tools"

    directory: StringProperty(
        name="Scrapland folder",
        description="Folder where Scrapland is installed",
        subtype="DIR_PATH",
        options={"HIDDEN"},
    )

    def draw(self, context):
        layout = self.layout
        scene = bpy.context.scene
        files = context.scene.packed_items
        if not files:
            layout.operator(
                FindPacked.bl_idname, text="Load Scrapland Data", icon="FILE_FOLDER"
            )
        handle = getattr(scene.ScrapTool, "handle", None)
        if files and not handle:
            layout.template_list(
                "FILEBROWSER_UL_packed_files",
                "",
                context.scene,
                "packed_items",
                context.scene,
                "packed_item_index",
            )
            layout.operator(
                LoadPacked.bl_idname, text="Load selected files", icon="IMPORT"
            )
        if handle is None:
            return
        else:
            layout.operator(
                ClosePacked.bl_idname, text="Close opened files", icon="PANEL_CLOSE"
            )
        layout.label(text="Path: " + handle.pwd())
        layout.template_list(
            "FILEBROWSER_UL_scrap_files",
            "",
            context.scene,
            "file_browser_items",
            context.scene,
            "file_browser_item_index",
        )
        if handle.is_level():
            layout.operator(LoadLevel.bl_idname, text="Load Level", icon="IMPORT")


class DumpToJSON(Operator):
    bl_idname = "wm.button_packed_file_dump_to_json"
    bl_label = "Dump to JSON"

    path: StringProperty(
        options={"HIDDEN"},
    )
    filepath: StringProperty(subtype="FILE_PATH", options={"HIDDEN"})
    pretty: BoolProperty(name="Pretty print JSON")

    def execute(self, context):
        scene = bpy.context.scene
        handle = getattr(scene.ScrapTool, "handle")
        if handle is None:
            return {"CANCELLED"}
        if os.path.isfile(self.filepath):
            return {"CANCELLED"}
        try:
            handle.dump_to_json(self.path, self.filepath, self.pretty)
        except OSError:
            raise
        return {"FINISHED"}

    def invoke(self, context, event):
        self.filepath = self.path.split("/")[-1] + ".json"
        context.window_manager.fileselect_add(self)
        return {"RUNNING_MODAL"}


classes = [
    PackedEntry,
    PackedFile,
    HandleFile,
    FindPacked,
    LoadPacked,
    ClosePacked,
    LoadLevel,
    FILEBROWSER_UL_scrap_files,
    FILEBROWSER_UL_packed_files,
    FILEBROWSER_PT_Scraptools,
    DumpToJSON,
]


def draw_menu(self, context):
    operator = getattr(context, "button_operator", None)
    if operator.bl_rna.identifier != HandleFile.bl_rna.identifier:
        return
    layout = self.layout
    layout.separator()
    op = layout.operator(DumpToJSON.bl_idname)
    op.path = operator.path


def register():
    for cls in classes:
        bpy.utils.register_class(cls)
    bpy.types.UI_MT_button_context_menu.append(draw_menu)
    bpy.types.Scene.ScrapTool = SimpleNamespace()
    bpy.types.Scene.file_browser_items = CollectionProperty(
        type=PackedEntry, name="file_browser_items"
    )
    bpy.types.Scene.file_browser_item_index = IntProperty(
        default=0, name="file_browser_item_index"
    )
    bpy.types.Scene.packed_items = CollectionProperty(
        type=PackedFile, name="packed_items"
    )
    bpy.types.Scene.packed_item_index = IntProperty(default=0, name="packed_item_index")


def unregister():
    for attr in (
        "ScrapTool",
        "file_browser_items",
        "file_browser_item_index",
        "packed_items",
        "packed_item_index",
    ):
        if hasattr(bpy.types.Scene, attr):
            delattr(bpy.types.Scene, attr)
    for cls in reversed(classes):
        try:
            bpy.utils.unregister_class(cls)
        except RuntimeError:
            pass
    bpy.types.UI_MT_list_item_context_menu.remove(draw_menu)


if __name__ == "__main__":
    import imp

    imp.reload(sys.modules[__name__])
    for cls in classes:
        bpy.utils.register_class(cls)
