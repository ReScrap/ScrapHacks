
int _D3DXGetFVFVertexSize(uint fvf)

{
  uint uVar1;
  uint uVar2;
  uint uVar3;
  int vert_size;
  
  uVar1 = fvf & 0xe;
  vert_size = 0;
  if (uVar1 == 2) {
    vert_size = 0xc;
  }
  else if ((uVar1 == 4) || (uVar1 == 6)) {
    vert_size = 0x10;
  }
  else if (uVar1 == 8) {
    vert_size = 0x14;
  }
  else if (uVar1 == 0xa) {
    vert_size = 0x18;
  }
  else if (uVar1 == 0xc) {
    vert_size = 0x1c;
  }
  else if (uVar1 == 0xe) {
    vert_size = 0x20;
  }
  if ((fvf & 0x10) != 0) {
    vert_size += 0xc;
  }
  if ((fvf & 0x20) != 0) {
    vert_size += 4;
  }
  if ((fvf & 0x40) != 0) {
    vert_size += 4;
  }
  if (fvf < '\0') {
    vert_size += 4;
  }
  uVar1 = fvf >> 8 & 0xf;
  uVar3 = fvf >> 16;
  if (uVar3 == 0) {
    vert_size += uVar1 * 8;
  }
  else {
    for (; uVar1 != 0; uVar1 -= 1) {
      uVar2 = uVar3 & 3;
      if (uVar2 == 0) {
        vert_size += 8;
      }
      else if (uVar2 == 1) {
        vert_size += 0xc;
      }
      else if (uVar2 == 2) {
        vert_size += 0x10;
      }
      else if (uVar2 == 3) {
        vert_size += 4;
      }
      uVar3 >>= 2;
    }
  }
  return vert_size;
}