import numpy as np
from tqdm import trange
data=open(r"E:\Games\Steam\steamapps\common\Scrapland\ext\models\chars\dtritus\dtritus.sm3","rb").read()

for ndim in [3]:
    size = ndim*ndim*4
    prog=trange(len(data)-size)
    for n in prog:
        m=np.frombuffer(data[n:][:size],dtype=np.float32).reshape(ndim,ndim)
        try:
            inv=np.linalg.inv(m)
        except np.linalg.LinAlgError:
            continue
        if abs(np.linalg.det(m)-1)<0.0001 or np.allclose(m.T,inv):
            prog.write(f"HIT @ {n} ({ndim})")
