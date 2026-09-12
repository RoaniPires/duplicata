import struct, zlib
def encode_png(w,h,px):
    raw=bytearray()
    for y in range(h):
        raw.append(0)
        for x in range(w):
            raw += bytes(px[y*w+x])
    def chunk(t,d):
        c=struct.pack('>I',len(d))+t+d
        return c+struct.pack('>I', zlib.crc32(t+d)&0xffffffff)
    ihdr=struct.pack('>IIBBBBB', w,h,8,6,0,0,0)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR',ihdr)
            + chunk(b'IDAT', zlib.compress(bytes(raw),9)) + chunk(b'IEND',b''))

def build_ico(images):
    n=len(images)
    hdr=struct.pack('<HHH',0,1,n)
    off=6+16*n
    ents=b''; blobs=b''
    for w,h,blob in images:
        ents+=struct.pack('<BBBBHHII', 0 if w==256 else w, 0 if h==256 else h, 0,0,1,32,len(blob),off)
        off+=len(blob); blobs+=blob
    return hdr+ents+blobs
