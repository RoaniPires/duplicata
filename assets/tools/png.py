import struct, zlib

def decode_png(data):
    assert data[:8]==b'\x89PNG\r\n\x1a\n'
    pos=8; idat=b''; w=h=bd=ct=None; plte=None; trns=None
    while pos<len(data):
        ln,=struct.unpack_from('>I',data,pos); typ=data[pos+4:pos+8]
        chunk=data[pos+8:pos+8+ln]; pos+=12+ln
        if typ==b'IHDR': w,h,bd,ct,_,_,il=struct.unpack('>IIBBBBB',chunk); assert il==0
        elif typ==b'IDAT': idat+=chunk
        elif typ==b'PLTE': plte=chunk
        elif typ==b'tRNS': trns=chunk
        elif typ==b'IEND': break
    raw=zlib.decompress(idat)
    ch={0:1,2:3,3:1,4:2,6:4}[ct]
    assert bd==8, f"bitdepth {bd}"
    bpp=ch
    stride=w*ch
    out=bytearray(); prev=bytearray(stride); p=0
    for _ in range(h):
        f=raw[p]; p+=1
        line=bytearray(raw[p:p+stride]); p+=stride
        for i in range(stride):
            a=line[i-bpp] if i>=bpp else 0
            b=prev[i]
            c=prev[i-bpp] if i>=bpp else 0
            if f==1: line[i]=(line[i]+a)&255
            elif f==2: line[i]=(line[i]+b)&255
            elif f==3: line[i]=(line[i]+(a+b)//2)&255
            elif f==4:
                pa=abs(b-c); pb=abs(a-c); pc=abs(a+b-2*c)
                pr=a if (pa<=pb and pa<=pc) else (b if pb<=pc else c)
                line[i]=(line[i]+pr)&255
        out+=line; prev=line
    px=[]
    for i in range(w*h):
        s=out[i*ch:(i+1)*ch]
        if ct==6: px.append(tuple(s))
        elif ct==2: px.append((s[0],s[1],s[2],255))
        elif ct==0: px.append((s[0],s[0],s[0],255))
        elif ct==4: px.append((s[0],s[0],s[0],s[1]))
        elif ct==3:
            idx=s[0]; r,g,b=plte[idx*3:idx*3+3]
            a=trns[idx] if trns and idx<len(trns) else 255
            px.append((r,g,b,a))
    return w,h,px

def lum(c):
    def ch(v):
        v/=255.0
        return v/12.92 if v<=0.04045 else ((v+0.055)/1.055)**2.4
    return 0.2126*ch(c[0])+0.7152*ch(c[1])+0.0722*ch(c[2])

def contrast(a,b):
    la,lb=lum(a),lum(b)
    hi,lo=max(la,lb),min(la,lb)
    return (hi+0.05)/(lo+0.05)
