#!/usr/bin/env python3
"""Render the Wisp logo (same geometry as assets/logo.svg) to assets/icon.ico and assets/icon64.rgba.
Requires Pillow:  pip install pillow"""
from PIL import Image, ImageDraw
import os

SEGS = [((9,17),(12,39),(17,51),(23,51)), ((23,51),(29,51),(29,31),(32,29)),
        ((32,29),(35,31),(35,51),(41,51)), ((41,51),(47,51),(52,39),(55,17))]
STOPS = [(0,(255,154,122)),(.55,(255,93,108)),(1,(194,59,143))]

def bez(p0,p1,p2,p3,t):
    u=1-t
    return tuple(u**3*a+3*u*u*t*b+3*u*t*t*c+t**3*d for a,b,c,d in zip(p0,p1,p2,p3))

def grad(c, size, k):
    # linear gradient (8,10)->(56,54) in 64-unit space
    x0,y0,x1,y1 = 8*k,10*k,56*k,54*k
    img = Image.new("RGBA",(size,size))
    px = img.load()
    dx,dy = x1-x0,y1-y0; dd = dx*dx+dy*dy
    for y in range(size):
        for x in range(size):
            t = max(0,min(1,((x-x0)*dx+(y-y0)*dy)/dd))
            for (a,ca),(b,cb) in zip(STOPS,STOPS[1:]):
                if a<=t<=b:
                    f=(t-a)/(b-a); px[x,y]=tuple(int(ca[i]+(cb[i]-ca[i])*f) for i in range(3))+(255,); break
    return img

def stroke_mask(size, width, k):
    m = Image.new("L",(size,size),0); d = ImageDraw.Draw(m); r = width*k/2
    for s in SEGS:
        for i in range(0,401):
            x,y = bez(*s,i/400); d.ellipse((x*k-r,y*k-r,x*k+r,y*k+r),fill=255)
    return m

def render(size):
    ss = 4; S = size*ss; k = S/64
    g = grad(None,S,k)
    out = Image.new("RGBA",(S,S),(0,0,0,0))
    glow = g.copy(); glow.putalpha(stroke_mask(S,12,k).point(lambda v:int(v*.28)))
    out = Image.alpha_composite(out,glow)
    core = g.copy(); core.putalpha(stroke_mask(S,6.5,k))
    out = Image.alpha_composite(out,core)
    d = ImageDraw.Draw(out); r=3.8*k
    d.ellipse((32*k-r,14*k-r,32*k+r,14*k+r),fill=(255,210,191,255))
    return out.resize((size,size),Image.LANCZOS)

os.makedirs("assets",exist_ok=True)
render(256).save("assets/icon.ico",sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])
open("assets/icon64.rgba","wb").write(render(64).tobytes())
render(256).save("assets/icon256.png")

# ---- installer artwork (24-bit BMP, as NSIS requires) ----
def gradient(w, h, top, bottom):
    img = Image.new("RGB", (w, h))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / max(1, h - 1)
        d.line([(0, y), (w, y)], fill=tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3)))
    return img

os.makedirs("installer", exist_ok=True)
logo = render(256)
head = gradient(150, 57, (23, 23, 66), (12, 12, 32))
head.paste(logo.resize((44, 44), Image.LANCZOS), (96, 7), logo.resize((44, 44), Image.LANCZOS))
head.save("installer/header.bmp")
side = gradient(164, 314, (29, 17, 64), (12, 12, 32))
side.paste(logo.resize((96, 96), Image.LANCZOS), (34, 90), logo.resize((96, 96), Image.LANCZOS))
side.save("installer/sidebar.bmp")
print("ok")
