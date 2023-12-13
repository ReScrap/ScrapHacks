import maturin
import zipfile
import os
import sys
import json
import tomllib
import subprocess as SP
import shutil
import tempfile
from pathlib import Path

meta = json.loads(SP.check_output(["cargo", "metadata", "-q", "--no-deps"]))
folder = None
for pkg in meta["packages"]:
    target = pkg["targets"][0]
    if "cdylib" in target["kind"]:
        folder = target["name"]
module_name = maturin.get_config().get("module-name")

if module_name:
    folder = module_name.split(".")[0]

if folder is None:
    exit("Rust module not found")

os.makedirs("wheels", exist_ok=True)
zip_path = maturin.build_wheel("wheels")
zip_path = os.path.join("wheels", zip_path)
with zipfile.ZipFile(zip_path, "r") as zip_file, zipfile.ZipFile(
    "scraptools.zip", "w"
) as out_file:
    out_file.mkdir(folder)
    for entry in zip_file.filelist:
        if entry.filename.split("/")[0] == folder:
            name = f"{folder}/" + entry.filename.split("/")[-1]
            with zip_file.open(entry.filename) as fh:
                print(f"Writing {name}")
                out_file.writestr(name, fh.read())
os.remove(zip_path)
print("Wrote scraptools.zip")
if "--zip" in sys.argv[1:]:
    exit(0)

addon_zip_path = os.path.abspath("scraptools.zip")
blender_path = shutil.which("blender")
blender_script = f"""
import bpy
path=bpy.utils.user_resource('SCRIPTS',path="addons")
print(f"###{{path}}###")
""".strip()

if blender_path:
    with tempfile.TemporaryDirectory() as temp:
        python_file = os.path.join(temp, "get_path.py")
        with open(python_file, "w") as fh:
            fh.write(blender_script)
        res = SP.check_output(
            ["blender", "--background", "--python", python_file]
        ).splitlines()
    for line in res:
        line = line.strip()
        if line.startswith(b"###") and line.endswith(b"###"):
            addons_path = line.decode("utf8").strip("#")
            print(addons_path, os.path.isdir(addons_path))
            addons_zip = zipfile.ZipFile(addon_zip_path)
            addons_zip.extractall(addons_path)
            del addons_zip
            os.remove(addon_zip_path)
    SP.check_call(["blender",*sys.argv[1:]])
