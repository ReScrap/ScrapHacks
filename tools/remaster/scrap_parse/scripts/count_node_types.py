from pprint import pprint
import ScraplandTool
from collections import Counter
s=ScraplandTool.ScraplandTool
p=s.MultiPack(s.find_scrapland())

c={}

for e in p.entries():
    if not e['is_file']:
        continue
    ext=e['path'].split(".")[-1]
    if not (ext in ['cm3','sm3']):
        continue
    print(e['path'])
    data=p.parse_file(e['path'])
    for node in data['scene']['nodes']:
        content = node["content"]
        if content:
            c.setdefault(content.get("type"),set()).add(e['path'])
