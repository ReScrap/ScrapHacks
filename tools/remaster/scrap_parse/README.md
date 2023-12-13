# ScraplandTool

ScraplandTool is a Blender Add-On to load Scrapland .packed files and import map and object geometry into Blender

## Roadmap

- [x] Importing .emi Maps
  - [ ] Lightmaps
- [ ] Importing .sm3 objects
  - [ ] Node types
    - [x] Dummy
    - [x] TriangleMesh (appears to be unused)
    - [x] D3DMesh (basic support)
    - [ ] Camera
    - [ ] Light
    - [ ] Ground
    - [ ] Particle System
    - [ ] Graphic3D
    - [?] Lens Flare (appears to be unused)
  - [ ] Node transformations
  - [ ] Materials
- [ ] Import .cm3 Animations
- [ ] Exporting .emi Maps
- [ ] Exporting .sm3 Objects
- [ ] Exporting .cm3 Animations

## Installation

Requirements:

- Python 3.x installed
- Maturin python module installed (`pip install maturin`)
- Reasonable up to date Rust toolchain installed
- Blender 4.x

To install simpyl run `build.py`, if Blender is in your PATH environment variable the built addon will automatically be installed and can be enabled under Preferences -> Add-ons -> Import-Export -> Scrapland Tools

## Usage

- open the Sidebar by pressin "N" or clicking the arrow in the top left of the 3D-View
- select the "Tools" tab and click "Load Scrapland Data"
- Confirm Scrapland installation folder and change it if neccessary
- Click "Find and load .packed", this will list all found .packed files and auto-select the "Data.packed" files in the Scrapland root folder
- select any additional .packed files you want to import (mods, languages, etc)
- click "Load selected files" to display a file browser allowing you to browser the contents of the .packed files
- if you navigate into a folder that contains a level (for example "/levels/outskirts") a "Load Level" button will show up allowing you to import the map
- currently supported formats are levels, .sm3 objects and text files (.py, .ini)
- you can dump the parsed representation of a file into a JSON file for inspection and further processing by right clicking on a file or (level) folder and selecting "Dump to JSON"