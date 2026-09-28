"""生成 AgentHub 应用图标（Tauri bundle 需要）。

用超采样绘制再降采样，避免小尺寸锯齿。仅依赖 Pillow。
"""

from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
S = 1024  # 超采样画布

BG_TOP = (11, 15, 23)
BG_BOTTOM = (18, 30, 48)
TEAL = (45, 212, 191)
VIOLET = (139, 92, 246)
EDGE = (94, 234, 212)
NODE = (226, 240, 255)


def lerp(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))


def rounded_mask(size, radius):
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, size - 1, size - 1), radius=radius, fill=255)
    return mask


def build_base():
    base = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    grad = Image.new("RGBA", (S, S))
    gd = ImageDraw.Draw(grad)
    for y in range(S):
        gd.line([(0, y), (S, y)], fill=lerp(BG_TOP, BG_BOTTOM, y / S) + (255,))
    base.paste(grad, (0, 0), rounded_mask(S, int(S * 0.22)))
    return base


def draw_hub(img):
    d = ImageDraw.Draw(img, "RGBA")
    c = S / 2
    # 四角方向节点（代表 Provider / Skill / MCP / Env 四类资源）
    radius = S * 0.30
    angles = [(-90), (0), (90), (180)]
    centers = []
    for a in angles:
        import math

        rad = math.radians(a)
        centers.append((c + radius * math.cos(rad), c + radius * math.sin(rad)))

    # 连线
    for (x, y) in centers:
        d.line([(c, c), (x, y)], fill=EDGE + (110,), width=int(S * 0.018))

    # 中心枢纽（发光圆）
    for r, alpha in ((S * 0.175, 40), (S * 0.145, 70), (S * 0.115, 255)):
        d.ellipse([c - r, c - r, c + r, c + r], fill=TEAL + (alpha,))
    d.ellipse(
        [c - S * 0.062, c - S * 0.062, c + S * 0.062, c + S * 0.062],
        fill=(255, 255, 255, 235),
    )

    # 外圈节点
    for i, (x, y) in enumerate(centers):
        r = S * 0.072
        color = VIOLET if i % 2 else NODE
        d.ellipse([x - r, y - r, x + r, y + r], fill=color + (255,))
        d.ellipse(
            [x - r * 0.55, y - r * 0.55, x + r * 0.55, y + r * 0.55],
            fill=BG_TOP + (255,),
        )


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    img = build_base()
    draw_hub(img)

    def export(size, name):
        img.resize((size, size), Image.LANCZOS).save(OUT / name)

    export(32, "32x32.png")
    export(128, "128x128.png")
    export(256, "128x128@2x.png")
    export(512, "icon.png")
    export(256, "Square150x150Logo.png")
    export(88, "Square44x44Logo.png")

    # ICO 含多尺寸，Windows 任务栏/资源管理器都用它
    ico = img.resize((256, 256), Image.LANCZOS)
    ico.save(
        OUT / "icon.ico",
        format="ICO",
        sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print("icons written to", OUT)
    for p in sorted(OUT.iterdir()):
        print(f"  {p.name:24} {p.stat().st_size:>8} bytes")


if __name__ == "__main__":
    main()