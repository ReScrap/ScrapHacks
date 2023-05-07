import sys
from .. import scrap_bridge
from bpy.props import (StringProperty, BoolProperty, CollectionProperty,
                       IntProperty)

bl_info = {
    "name": "Packed Archive File",
    "blender": (2, 71, 0),
    "location": "File &gt; Import",
    "description": "Import data from Scrapland .packed Archive",
    "category": "Import-Export"}




class ImportFilearchives(bpy.types.Operator):
    """Import whole filearchives directory."""
    bl_idname = "import_scene.packed"
    bl_label = 'Import Scrapland .packed'
    
    directory = StringProperty(name="'Scrapland' folder",
                               subtype="DIR_PATH", options={'HIDDEN'})
    filter_folder = BoolProperty(default=True, options={'HIDDEN'})
    filter_glob = StringProperty(default="", options={'HIDDEN'})
    
    def invoke(self, context, event):
        context.window_manager.fileselect_add(self)
        return {'RUNNING_MODAL'}

    def execute(self, context):
        # TODO: Validate filepath
        bpy.ops.ui.packed_browser('INVOKE_DEFAULT',filepath=self.directory)
        return {'FINISHED'}
    

class PackedFile(bpy.types.PropertyGroup):
    path = bpy.props.StringProperty()
    packed_file = bpy.props.StringProperty()
    selected = bpy.props.BoolProperty(name="")
    offset = bpy.props.IntProperty()
    size = bpy.props.IntProperty()


archive = None
class PackedBrowser(bpy.types.Operator):
    bl_idname = "ui.packed_browser"
    bl_label = "Packed Browser"
    bl_options = {'INTERNAL'}
    
    files = CollectionProperty(type=PackedFile)
    selected_index = IntProperty(default=0)
    
    def invoke(self, context, event):
        scrapland_path=scrap_bridge("find-scrapland")
        print(scrapland_path)
        packed_data=scrap_bridge("parse-packed",scrapland_path)
        print(packed_data)
        self.packed_data=packed_data
        return context.window_manager.invoke_props_dialog(self)
    
    def draw(self, context):
        if self.selected_index != -1:
            print("new selected_index: " + str(self.selected_index))
            self.files.clear()
            for packed_name,files in self.archive:
                for file in files:
                    entry = self.files.add()
                    entry.packed_file = packed_name
                    [entry.path,entry.offset,entry.size]=file
            self.selected_index = -1
        self.layout.template_list("PackedDirList", "", self, "current_dir", self, "selected_index")
    
    def execute(self, context):
        print("execute")
        return {'FINISHED'}


class PackedDirList(bpy.types.UIList):
    def draw_item(self, context, layout, data, item, icon, active_data, active_propname):
        operator = data
        packed_entry = item
        
        if self.layout_type in {'DEFAULT', 'COMPACT'}:
            layout.prop(packed_entry, "name", text="", emboss=False, icon_value=icon)
            layout.prop(packed_entry, "selected")
        elif self.layout_type in {'GRID'}:
            layout.alignment = 'CENTER'
            layout.label(text="", icon_value=icon)
        

def menu_func_import(self, context):
    self.layout.operator(ImportFilearchives.bl_idname, text="Scrapland .packed")

classes=[
    PackedFile,
    PackedDirList,
    PackedBrowser,
    ImportFilearchives,
]

def register():
    for cls in classes:
        bpy.utils.regiser_class(cls)
    bpy.types.INFO_MT_file_import.append(menu_func_import)


def unregister():
    for cls in reversed(classes):
        bpy.utils.unregister_class(cls)
    bpy.types.INFO_MT_file_import.remove(menu_func_import)


if __name__ == "__main__":
    import imp
    imp.reload(sys.modules[__name__])
    for cls in classes:
        bpy.utils.regiser_class(cls)

