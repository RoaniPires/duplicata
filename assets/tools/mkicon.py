import struct, sys
from png import decode_png, lum, contrast
from pngenc import encode_png, build_ico

KEYLINE=(0x14,0x14,0x16)

def entries(path):
    d=open(path,'rb').read()
    n=struct.unpack_from('<H',d,4)[0]
    for i in range(n):
        w,h,_,_,_,bpp,size,off = struct.unpack_from('<BBBBHHII', d, 6+16*i)
        yield (w or 256),(h or 256), d[off:off+size]

def downscale(W,H,px,nw,nh):
    out=[]
    for y in range(nh):
        y0,y1 = y*H//nh, max(y*H//nh+1,(y+1)*H//nh)
        for x in range(nw):
            x0,x1 = x*W//nw, max(x*W//nw+1,(x+1)*W//nw)
            sr=sg=sb=sa=0; n=0
            for yy in range(y0,y1):
                for xx in range(x0,x1):
                    r,g,b,a=px[yy*W+xx]
                    sr+=r*a; sg+=g*a; sb+=b*a; sa+=a; n+=1
            a=sa/n
            if sa==0: out.append((0,0,0,0))
            else: out.append((round(sr/sa), round(sg/sa), round(sb/sa), round(a)))
    return out

def make(S, blob):
    W,H,px = decode_png(blob)
    k = 1 if S<=32 else (2 if S<=64 else S//32)
    inner = S - 2*k
    art = downscale(W,H,px, inner, inner)
    canvas=[(0,0,0,0)]*(S*S)
    for y in range(inner):
        for x in range(inner):
            canvas[(y+k)*S + (x+k)] = art[y*inner+x]
    solid=[canvas[i][3]>=64 for i in range(S*S)]
    out=list(canvas)
    for y in range(S):
        for x in range(S):
            i=y*S+x
            if solid[i]: continue
            near=False
            for dy in range(-k,k+1):
                for dx in range(-k,k+1):
                    yy,xx=y+dy,x+dx
                    if 0<=xx<S and 0<=yy<S and solid[yy*S+xx]: near=True; break
                if near: break
            if near:
                out[i]=(KEYLINE[0],KEYLINE[1],KEYLINE[2],255)
    return encode_png(S,S,out)

src=sys.argv[1]; dst=sys.argv[2]
imgs=[]
for w,h,blob in entries(src):
    imgs.append((w,h,make(w,blob)))
    print(f"  {w}x{w} refeito")
open(dst,'wb').write(build_ico(imgs))
print("gravado", dst)
