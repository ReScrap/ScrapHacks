from distutils.command.install_data import install_data
import winreg as reg
import vdf
from pathlib import Path
import pefile
app_id="897610"
try:
    key = reg.OpenKey(reg.HKEY_LOCAL_MACHINE,"SOFTWARE\\Valve\\Steam")
except FileNotFoundError:
    key = reg.OpenKey(reg.HKEY_LOCAL_MACHINE,"SOFTWARE\\Wow6432Node\\Valve\\Steam")
path=Path(reg.QueryValueEx(key,"InstallPath")[0])
libraryfolders=vdf.load((path/"steamapps"/"libraryfolders.vdf").open("r"))['libraryfolders']
for folder in libraryfolders.values():
    path=Path(folder['path'])
    if app_id in folder['apps']:
        install_dir = vdf.load((path/"steamapps"/f"appmanifest_{app_id}.acf").open("r"))['AppState']['installdir']
        install_dir=path/"steamapps"/"common"/install_dir
        for file in install_dir.glob("**/*.exe"):
            pe = pefile.PE(file, fast_load=True)
            entry = pe.OPTIONAL_HEADER.AddressOfEntryPoint
            if pe.get_dword_at_rva(entry) == 0xE8:
                print(file)