## TODO

- Handle map elements ("DM_Element_*" -> "/Models/Elements/{name}/{name}.sm3")
- Handle map doors ("DM_Door_*" -> "/Models/Doors/{name}/{name}.sm3")

## Node Flags

- 0: RAIZ
- 1: GRUPO
- 2: SELEC
- 3: OCULTO
- 4: HVISIBLE
- 5: NO_CASTSHADOWS
- 6: NO_RCVSHADOWS
- 7: NO_RENDER
- 8: BOXMODE
- 12: COLLIDE
- 13: NO_COLLIDE
- 16: NO_ANIMPOS
- 17: NO_TRANS
- 18: SLERP2
- 19: EFFECT
- 20: BONE
- 21: BIPED
- 22: NO_TABNODOR
- 24: 2LADOS
- 25: RT_LIGHTING
- 26: RT_SHADOWS
- 27: NO_LIGHTMAP
- 28: NO_SECTOR
- 29: AREA_LIGHT

## Default Matrix

```
1 0 0 0
0 1 0 0
0 0 1 0
0 0 0 1
```

## Node Rotation

/models/chars/dtritus/dtritus.sm3: Bip Detritus

- Rot eje: -0.577351 -0.577350 -0.57305 ang: 119.999949
- Rot z: 0.000000 x: 0.000082 y: 90.000000

quaternion: [-0.50000036, -0.49999964, -0.49999964, 0.50000036]

(Vector((-0.5773500204086304, -0.5773500204086304, 0.5773508548736572)), 240.0000613201218)
Euler((90.0, 89.99992370605469, 0.0), 'XYZ')

q.angle/=2
q.axis.z*=-1

```python
import math
import itertools as ITT
target_angle = 119.999949
target_axis = Vector((-0.577351,-0.577350,-0.57305))
target_euler = Euler((0,90,0))
Q = Quaternion(Vector([-0.50000036, -0.49999964, -0.49999964, 0.50000036]).wxyz)
with open("q.log","w") as fh:
    for inv in ITT.product([1,-1],repeat=4):
        p = list(enumerate(Q))
        idx, p = list(zip(*p))
        idx = [idx*i for idx,i in zip(idx,inv)]
        p = Quaternion([p*i for p,i in zip(p,inv)])
        axis, angle = p.to_axis_angle()
        dist = (axis-target_axis).length
        angle = math.degrees(angle)
        for axis_names in ITT.permutations("XYZ"):
            axis_names="".join(axis_names)
            euler = Euler(p.to_euler(axis_names), axis_names)
            for r_axis in "XYZ":
                for amt in [-90,0,90]:
                    for sign in [1,-1]:
                        euler_r=euler.copy()
                        euler_r.rotate_axis(r_axis,math.radians(amt))
                        euler_r = Euler(map(lambda r: sign*math.degrees(r),euler_r))
                        euler_diff = (Vector(euler_r)-Vector(target_euler)).length
                        num_set = set(i for i,v in enumerate(euler_r) if abs(v)>0.01)
                        axis_diff = (axis-target_axis).length
                        if num_set=={1} and euler_r.y>0 and axis_diff<1:
                            print(idx, p, axis, angle, r_axis, amt, euler_r, euler_diff, axis_diff, file=fh)
    ```