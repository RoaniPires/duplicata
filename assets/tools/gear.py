import math, sys
from pngenc import encode_png, build_ico

SS = 4

def gear_alpha(S):
    n = 8
    cx = cy = S / 2.0
    r_tip  = 0.430 * S
    r_root = 0.320 * S
    r_hole = 0.145 * S
    half   = (math.pi / n) * (0.56 if S <= 20 else 0.46)
    cov = [0.0] * (S * S)
    for py in range(S):
        for px in range(S):
            hit = 0
            for sy in range(SS):
                for sx in range(SS):
                    x = px + (sx + 0.5) / SS - cx
                    y = py + (sy + 0.5) / SS - cy
                    d = math.hypot(x, y)
                    if d < r_hole:
                        continue
                    th = math.atan2(y, x)
                    per = 2 * math.pi / n
                    ph = (th % per)
                    if ph > per / 2:
                        ph -= per
                    r = r_tip if abs(ph) <= half else r_root
                    if d <= r:
                        hit += 1
            cov[py * S + px] = hit / (SS * SS)
    return cov

def render(S, rgb):
    cov = gear_alpha(S)
    px = []
    for c in cov:
        a = round(c * 255)
        px.append((rgb[0], rgb[1], rgb[2], a))
    return encode_png(S, S, px)

SIZES = [16, 20, 24, 32]
VARIANTS = {
    "settings_light.ico": (0x1F, 0x1F, 0x22),
    "settings_dark.ico":  (0xF0, 0xF0, 0xF2),
}
for name, rgb in VARIANTS.items():
    imgs = [(s, s, render(s, rgb)) for s in SIZES]
    open(name, 'wb').write(build_ico(imgs))
    print("gravado", name, [s for s in SIZES])
