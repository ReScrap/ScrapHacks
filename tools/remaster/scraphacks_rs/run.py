import subprocess as SP
import shutil as sh
import json
from pathlib import Path
import psutil
import os
import sys
os.environ['DISCORD_INSTANCE_ID']='1'
SP.check_call(["cargo","b","-r"])
info=[json.loads(line) for line in SP.check_output(["cargo","b", "-r" ,"-q","--message-format=json"]).splitlines()]
dll_path=None
for line in info:
    if line.get('reason')=="compiler-artifact" and ("dylib" in line.get("target",{}).get("crate_types",[])):
        dll_path=Path(line['filenames'][0])

sh.copy(dll_path,"E:/Games/Steam/steamapps/common/Scrapland/lib/ScrapHack.pyd")

if "--run" not in sys.argv[1:]:
    exit(0)

os.startfile("steam://run/897610/")
pid=None
while pid is None:
    for proc in psutil.process_iter():
        try:
            if proc.name()=="Scrap.exe":
                pid=proc.pid
        except:
            pass
print(f"PID: {pid:x}")
if "--dbg" in sys.argv[1:]:
    SP.run(["x32dbg","-p",str(pid)])
# cp D:/devel/Git_Repos/Scrapland-RE/tools/remaster/scraphacks_rs/target/i686-pc-windows-msvc/release/scraphacks_rs.dll E:/Games/Steam/steamapps/common/Scrapland/lib/ScrapHack.pyd
# x32dbg E:/Games/Steam/steamapps/common/Scrapland/Bin/Scrap.unpacked.exe "-debug:10 -console"