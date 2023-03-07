bl_info = {
    "name": "Riot Archive File (RAF)",
    "blender": (2, 71, 0),
    "location": "File &gt; Import",
    "description": "Import LoL data of an Riot Archive File",
    "category": "Import-Export"}


import bpy
from io_scene_lolraf import raf_utils
from bpy.props import (StringProperty, BoolProperty, CollectionProperty,
                       IntProperty)


class ImportFilearchives(bpy.types.Operator):
    """Import whole filearchives directory."""
    bl_idname = "import_scene.rafs"
    bl_label = 'Import LoL filearchives'
    
    directory = StringProperty(name="'filearchives' folder", 
                               subtype="DIR_PATH", options={'HIDDEN'})
    filter_folder = BoolProperty(default=True, options={'HIDDEN'})
    filter_glob = StringProperty(default="", options={'HIDDEN'})
    
    def invoke(self, context, event):
        context.window_manager.fileselect_add(self)
        return {'RUNNING_MODAL'}

    def execute(self, context):
        # TODO: Validate filepath
        bpy.ops.ui.raf_browser('INVOKE_DEFAULT',filepath=self.directory)
        return {'FINISHED'}
    

class RAFEntry(bpy.types.PropertyGroup):
    name = bpy.props.StringProperty()
    selected = bpy.props.BoolProperty(name="")


archive = None
class RAFBrowser(bpy.types.Operator):
    bl_idname = "ui.raf_browser"
    bl_label = "RAF-browser"
    bl_options = {'INTERNAL'}
    
    filepath = StringProperty()
    current_dir = CollectionProperty(type=RAFEntry)
    selected_index = IntProperty(default=0)
    
    def invoke(self, context, event):
        global archive
        archive = raf_utils.RAFArchive(self.filepath)
        return context.window_manager.invoke_props_dialog(self)
    
    def draw(self, context):
        if self.selected_index != -1:
            print("new selected_index: " + str(self.selected_index))
            global archive
            # TODO: change current directory of archive
            self.current_dir.clear()
            for dir in archive.current_dir():
                entry = self.current_dir.add()
                entry.name = dir
            self.selected_index = -1
        self.layout.template_list("RAFDirList", "", self, "current_dir", self, "selected_index")
    
    def execute(self, context):
        print("execute")
        return {'FINISHED'}


class RAFDirList(bpy.types.UIList):
    def draw_item(self, context, layout, data, item, icon, active_data, active_propname):
        operator = data
        raf_entry = item
        
        if self.layout_type in {'DEFAULT', 'COMPACT'}:
            layout.prop(raf_entry, "name", text="", emboss=False, icon_value=icon)
            layout.prop(raf_entry, "selected")
        elif self.layout_type in {'GRID'}:
            layout.alignment = 'CENTER'
            layout.label(text="", icon_value=icon)
        

def menu_func_import(self, context):
    self.layout.operator(ImportFilearchives.bl_idname, text="LoL Filearchives")


def register():
    bpy.utils.register_module(__name__)
    bpy.types.INFO_MT_file_import.append(menu_func_import)


def unregister():
    bpy.utils.unregister_module(__name__)
    bpy.types.INFO_MT_file_import.remove(menu_func_import)


if __name__ == "__main__":
    import imp
    imp.reload(raf_utils)
    bpy.utils.register_module(__name__)

