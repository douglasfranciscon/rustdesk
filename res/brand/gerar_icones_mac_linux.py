# CUSTOM BRANDING: gera, a partir do icone de 256 px (res/icon.ico), as artes de macOS e
# Linux: res/icon.png (1024), res/mac-icon.png (1024, tile arredondado), o AppIcon.icns do
# bundle, os icones de bandeja do macOS (template preto e branco, 60x60) e res/scalable.svg.
#
# Uso: python res/brand/gerar_icones_mac_linux.py <raiz>
#   <raiz> e este repositorio (arte da BR) ou a pasta de uma marca no repositorio privado
#   (marcas/<marca>), que espelha os mesmos caminhos: so precisa ter res/icon.ico.
# Precisa do Pillow (pip install pillow).
import base64, io, os, sys
from PIL import Image, ImageDraw, ImageFilter

repo = sys.argv[1]
for d in ("res", "flutter/macos/Runner"):
    os.makedirs(os.path.join(repo, d), exist_ok=True)
ico = Image.open(f"{repo}/res/icon.ico"); ico.size = (256, 256); ico.load()
mark256 = ico.convert("RGBA")

# 1) res/icon.png: 1024x1024, o mesmo quadrado branco com a marca.
icon1024 = mark256.resize((1024, 1024), Image.LANCZOS)
icon1024.save(f"{repo}/res/icon.png", optimize=True)

# 2) res/mac-icon.png: estilo macOS - tile arredondado centrado em tela transparente
#    (mesma geometria do upstream: 1024 com o tile ocupando ~840px).
canvas, tile = 1024, 840
off = (canvas - tile) // 2
radius = int(tile * 0.2237)  # proporcao do tile padrao da Apple
face = mark256.resize((tile, tile), Image.LANCZOS)
mask = Image.new("L", (tile, tile), 0)
ImageDraw.Draw(mask).rounded_rectangle([0, 0, tile - 1, tile - 1], radius=radius, fill=255)
# contorno cinza bem leve, para o tile branco nao sumir sobre fundo branco
outline = Image.new("RGBA", (tile, tile), (0, 0, 0, 0))
ImageDraw.Draw(outline).rounded_rectangle([0, 0, tile - 1, tile - 1], radius=radius, outline=(0, 0, 0, 28), width=3)
mac = Image.new("RGBA", (canvas, canvas), (0, 0, 0, 0))
mac.paste(face, (off, off), mask)
mac.alpha_composite(outline, (off, off))
mac.save(f"{repo}/res/mac-icon.png", optimize=True)

# 3) AppIcon.icns a partir do mac-icon.
sizes = [(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512), (1024, 1024)]
mac.save(f"{repo}/flutter/macos/Runner/AppIcon.icns", format="ICNS", sizes=sizes)

# 4) Bandeja do macOS: silhueta da marca como template (preto, e branco), 60x60.
lum = mark256.convert("L")
sil = lum.point(lambda v: 255 if v < 235 else 0)  # tudo que nao e branco vira a silhueta
sil = sil.filter(ImageFilter.GaussianBlur(0.6))
sil = sil.resize((60, 60), Image.LANCZOS)
for name, rgb in (("mac-tray-dark-x2.png", (0, 0, 0)), ("mac-tray-light-x2.png", (255, 255, 255))):
    im = Image.new("RGBA", (60, 60), rgb + (0,))
    im.putalpha(sil)
    im.save(f"{repo}/res/{name}", optimize=True)

# 5) res/scalable.svg: SVG que embute o PNG de 256px (o renderizador do tema de icones
#    aceita <image>; nao existe vetor da marca neste repositorio).
buf = io.BytesIO(); mark256.save(buf, format="PNG", optimize=True)
b64 = base64.b64encode(buf.getvalue()).decode()
svg = ('<?xml version="1.0" encoding="UTF-8"?>\n'
       '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" '
       'width="256" height="256" viewBox="0 0 256 256">\n'
       '  <!-- BR Remote: a marca embutida como PNG; gerado por gerar_icones_mac_linux.py -->\n'
       f'  <image width="256" height="256" xlink:href="data:image/png;base64,{b64}"/>\n'
       '</svg>\n')
open(f"{repo}/res/scalable.svg", "w", newline="\n").write(svg)

# Conferencia
for f in ("res/icon.png", "res/mac-icon.png", "res/mac-tray-dark-x2.png", "res/mac-tray-light-x2.png", "flutter/macos/Runner/AppIcon.icns"):
    im = Image.open(f"{repo}/{f}")
    extra = ""
    if f.endswith(".icns"):
        extra = " sizes=" + str(sorted(im.info.get("sizes", [])))
    print(f, im.size, im.mode, extra)
