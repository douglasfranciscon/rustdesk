#!/usr/bin/env python3
"""CUSTOM BRANDING: derive the app's brand palette from one colour.

Usage: python3 res/brand/brand_colors.py '#RRGGBB'

A brand folder gives only its main colour (cor.txt, the logo's). The app needs
five tones, each with a contrast job, so the rest are derived here keeping the
hue and saturation and moving only the lightness:

    kBrandColor   the colour as given: the logo colour, the ID
    kBrandAccent  buttons and highlights, white text on top: the colour itself
                  when it already reaches 4.5:1 against white, else darkened to it
    kBrandDark    brand text and icons on a light surface: >= 5:1 on kBrandGrayBg
    kBrandCmId    the ID in the connection manager: same as kBrandDark
    kBrandGrayBg  light surface tinted with the brand: lightness 0.94, saturation 0.15

Prints NAME=0xAARRGGBB lines, which apply-brand.sh writes into
flutter/lib/brand.dart. See docs/1_MarcasAlternativas.md.
"""
import colorsys
import re
import sys

WHITE = (255, 255, 255)


def luminance(rgb):
    def channel(v):
        v /= 255
        return v / 12.92 if v <= 0.03928 else ((v + 0.055) / 1.055) ** 2.4

    r, g, b = (channel(v) for v in rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a, b):
    hi, lo = sorted((luminance(a), luminance(b)), reverse=True)
    return (hi + 0.05) / (lo + 0.05)


def from_hls(h, l, s):
    return tuple(round(v * 255) for v in colorsys.hls_to_rgb(h, l, s))


def to_hls(rgb):
    return colorsys.rgb_to_hls(*(v / 255 for v in rgb))


def darken_until(rgb, good_enough):
    """Same hue and saturation, lower lightness, until good_enough(colour)."""
    h, l, s = to_hls(rgb)
    while not good_enough(rgb) and l > 0:
        l = max(0.0, l - 0.005)
        rgb = from_hls(h, l, s)
    return rgb


def derive(brand):
    h, _, _ = to_hls(brand)
    gray_bg = from_hls(h, 0.94, 0.15)
    accent = darken_until(brand, lambda c: contrast(c, WHITE) >= 4.5)
    dark = darken_until(brand, lambda c: contrast(c, gray_bg) >= 5.0)
    return [
        ("kBrandColor", brand),
        ("kBrandAccent", accent),
        ("kBrandDark", dark),
        ("kBrandCmId", dark),
        ("kBrandGrayBg", gray_bg),
    ]


def main():
    m = re.fullmatch(r"#?([0-9A-Fa-f]{6})", sys.argv[1] if len(sys.argv) == 2 else "")
    if not m:
        print("usage: brand_colors.py '#RRGGBB'", file=sys.stderr)
        return 2
    hexa = m.group(1)
    brand = tuple(int(hexa[i:i + 2], 16) for i in (0, 2, 4))
    for name, rgb in derive(brand):
        print("%s=0xFF%02X%02X%02X" % ((name,) + rgb))
    return 0


if __name__ == "__main__":
    sys.exit(main())
