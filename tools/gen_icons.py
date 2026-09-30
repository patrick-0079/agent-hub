"""生成 AgentHub 应用图标（Tauri bundle 需要）：白底扁平猫咪。

深灰猫剪影 + 品牌青眼睛，白色背景；超采样绘制再降采样避免锯齿。仅依赖 Pillow。
"""

from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"
S = 1024  # 超采样画布

WHITE = (255, 255, 255, 255)
INK = (47, 51, 59, 255)      # 深灰（与界面 ink 同族）
TEAL = (47, 157, 144, 255)   # 品牌青（brand-500）


def build_cat():
    img = Image.new("RGBA", (S, S), WHITE)
    d = ImageDraw.Draw(img, "RGBA")

    # 头（椭圆，含下巴）
    d.ellipse([172, 250, 852, 870], fill=INK)

    # 耳朵（三角，与头部融合）
    d.polygon([(225, 445), (295, 75), (520, 275)], fill=INK)
    d.polygon([(799, 445), (729, 75), (504, 275)], fill=INK)

    # 眼睛（品牌青）
    d.ellipse([330, 515, 450, 635], fill=TEAL)
    d.ellipse([574, 515, 694, 635], fill=TEAL)

    # 鼻子（白色小三角）
    d.polygon([(474, 672), (550, 672), (512, 714)], fill=WHITE)

    # 嘴（两小撇）
    d.arc([400, 690, 512, 790], 20, 90, fill=WHITE, width=16)
    d.arc([512, 690, 624, 790], 90, 160, fill=WHITE, width=16)

    # 胡须（每侧三根）
    w = 18
    for y0, y1 in ((640, 655), (700, 700), (760, 745)):
        d.line([(150, y0), (330, y1)], fill=WHITE, width=w)
        d.line([(874, y0), (694, y1)], fill=WHITE, width=w)

    return img


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    img = build_cat()

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
