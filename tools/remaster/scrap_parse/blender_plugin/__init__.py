import pickle
import subprocess as SP

from . import packed_browser
from . import level_import

def scrap_bridge(*cmd):
    cmd=["scrap_parse",*cmd]
    proc=SP.Popen(cmd,stderr=None,stdin=None,stdout=SP.PIPE,shell=True,text=False)
    stdout,stderr=proc.communicate()
    code=proc.wait()
    if code:
        raise RuntimeError(str(stderr,"utf8"))
    return pickle.loads(stdout)

def register():
    packed_browser.register()
    level_import.regiser()

def unregister():
    packed_browser.unregister()
    level_import.unregister()

