from .ScraplandTool import *
import bpy

bl_info = {
    "name": "Scrapland Tools",
    "author": "Earthnuker",
    "version": (0, 1, 0),
    "blender": (4, 0, 1),
    "location": "File > Import",
    "description": "Import data from Scrapland .packed Archive",
    "category": "Import-Export",
}

from . import packed_browser
from . import arrange_nodes


def register():
    packed_browser.register()


def unregister():
    packed_browser.unregister()
