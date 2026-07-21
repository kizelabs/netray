#!/usr/bin/env python3
"""Draw the NeTray app icon as a 1024x1024 PNG (pure stdlib, 4x supersampled).

Motif: an up arrow (send) above a down arrow (receive), echoing the two-line
menu bar readout, on a rounded-rect slate background. Colors match the app:
green for up, blue for down.

Output: scripts/icon-1024.png  ->  fed to sips/iconutil by bundle.sh
"""
import struct, zlib, os

S = 4                      # supersample factor
N = 1024
W = N * S

def blank(r, g, b, a=0):
    return bytearray([r, g, b, a]) * (W * W)

buf = blank(0, 0, 0, 0)

def px(x, y, r, g, b, a=255):
    if x < 0 or y < 0 or x >= W or y >= W:
        return
    i = (y * W + x) * 4
    # source-over composite onto existing pixel
    sa = a / 255.0
    da = buf[i+3] / 255.0
    oa = sa + da * (1 - sa)
    if oa == 0:
        return
    for k, sc in enumerate((r, g, b)):
        dc = buf[i+k]
        buf[i+k] = int((sc * sa + dc * da * (1 - sa)) / oa)
    buf[i+3] = int(oa * 255)

def row_span(y, x0, y0, x1, y1, rad):
    """Inclusive [xmin, xmax] of the rounded rect at row y, or None."""
    if y < y0 or y > y1:
        return None
    dy = None
    if y < y0 + rad:
        dy = (y0 + rad) - y
    elif y > y1 - rad:
        dy = y - (y1 - rad)
    if dy is None:
        return x0, x1
    if dy > rad:
        return None
    inset = rad - int((rad*rad - dy*dy) ** 0.5)
    return x0 + inset, x1 - inset

# --- background: vertical gradient inside a rounded rect -----------------
x0, y0, x1, y1 = 0, 0, W-1, W-1
rad = int(0.2246 * W)      # macOS icon corner ratio (~185/824 of the grid)
top    = (0x25, 0x31, 0x42)   # slate, lighter at top
bottom = (0x0E, 0x16, 0x20)   # near-black slate at bottom
for y in range(W):
    span = row_span(y, x0, y0, x1, y1, rad)
    if span is None:
        continue
    t = y / (W - 1)
    r = int(top[0] + (bottom[0]-top[0])*t)
    g = int(top[1] + (bottom[1]-top[1])*t)
    b = int(top[2] + (bottom[2]-top[2])*t)
    xa, xb = span
    for x in range(xa, xb + 1):
        i = (y*W + x)*4
        buf[i], buf[i+1], buf[i+2], buf[i+3] = r, g, b, 255

def fill_arrow(cx, cy, half_w, head_h, shaft_w, shaft_h, up, color):
    """A chunky arrow: triangular head + rectangular shaft, centered at (cx,cy)."""
    r, g, b = color
    y_top = cy - (head_h + shaft_h) // 2
    for y in range(cy - (head_h+shaft_h)//2, cy + (head_h+shaft_h)//2 + 1):
        for x in range(cx - half_w, cx + half_w + 1):
            dy = (y - y_top) if up else (y_top + head_h + shaft_h - y)
            inside = False
            if 0 <= dy <= head_h:
                # triangle: full width at base (dy=head_h), point at dy=0
                frac = dy / head_h
                w = int(half_w * frac)
                if abs(x - cx) <= w:
                    inside = True
            elif head_h < dy <= head_h + shaft_h:
                if abs(x - cx) <= shaft_w:
                    inside = True
            if inside:
                px(x, y, r, g, b, 255)

green = (0x34, 0xD3, 0x99)
blue  = (0x60, 0xA5, 0xFA)

gap   = int(0.045 * W)
ah    = int(0.30 * W)        # arrow total height
head  = int(0.17 * W)
half  = int(0.155 * W)
shaft = int(0.058 * W)
cx = W // 2
fill_arrow(cx, W//2 - ah//2 - gap//2, half, head, shaft, ah-head, True,  green)
fill_arrow(cx, W//2 + ah//2 + gap//2, half, head, shaft, ah-head, False, blue)

# --- downsample SxS -> 1x (box filter) -----------------------------------
out = bytearray(N*N*4)
for y in range(N):
    for x in range(N):
        R=G=B=A=0
        for dy in range(S):
            for dx in range(S):
                i = ((y*S+dy)*W + (x*S+dx))*4
                R+=buf[i]; G+=buf[i+1]; B+=buf[i+2]; A+=buf[i+3]
        j=(y*N+x)*4
        n=S*S
        out[j]=R//n; out[j+1]=G//n; out[j+2]=B//n; out[j+3]=A//n

def write_png(path, w, h, data):
    def chunk(typ, d):
        return struct.pack('>I', len(d)) + typ + d + struct.pack('>I', zlib.crc32(typ+d) & 0xffffffff)
    raw = bytearray()
    for y in range(h):
        raw.append(0)
        raw += data[y*w*4:(y+1)*w*4]
    png = b'\x89PNG\r\n\x1a\n'
    png += chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0))
    png += chunk(b'IDAT', zlib.compress(bytes(raw), 9))
    png += chunk(b'IEND', b'')
    open(path,'wb').write(png)

dst = os.path.join(os.path.dirname(__file__), 'icon-1024.png')
write_png(dst, N, N, out)
print(dst)
