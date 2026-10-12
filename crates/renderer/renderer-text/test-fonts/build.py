#!/usr/bin/env python3
# Regenerates TelarTest.ttf from Inter 4.1: nix shell nixpkgs#python3Packages.fonttools -c python build.py "$(nix build nixpkgs#inter --no-link --print-out-paths)/share/fonts/truetype/InterVariable.ttf"
import sys
from pathlib import Path
from fontTools import subset
from fontTools.ttLib import TTFont

UNICODES = (
    [*range(0x20, 0x7F)]
    + [0xA0, 0x2002, 0x2003, 0x2009, 0x200A, 0x202F]
    + [0xA2, 0xA3, 0xA5, 0x20AC]
    + [0xA7, 0xB6, 0x2020, 0x2021, 0x2030, 0x2032, 0x2033]
    + [0xA9, 0xAE, 0x2122, 0xB0]
    + [0x2012, 0x2013, 0x2014, 0x2026, 0xB7, 0x2022]
    + [0xD7, 0xF7, 0xB1, 0x2212, 0x2248, 0x2260, 0x2264, 0x2265]
    + [0x2190, 0x2191, 0x2192, 0x2193]
    + [0x2018, 0x2019, 0x201C, 0x201D, 0xAB, 0xBB, 0x2039, 0x203A]
    + [0x2318, 0x2325, 0x21E7, 0x2303, 0x23CE, 0x232B, 0x21E5]
)

source, target = sys.argv[1], Path(__file__).with_name("TelarTest.ttf")
options = subset.Options()
options.layout_features = ["kern"]
options.notdef_outline = True
options.name_IDs = ["*"]
options.glyph_names = True
subsetter = subset.Subsetter(options)
font = TTFont(source)
subsetter.populate(unicodes=UNICODES)
subsetter.subset(font)

names = font["name"]
copyright = str(names.getName(0, 3, 1, 0x409)).rstrip(".")
replacements = {
    0: copyright + ". Telar Test is a Basic Latin and typographic-symbol subset of Inter 4.1, renamed.",
    1: "Telar Test",
    3: "TelarTest-Regular;subset of Inter 4.1",
    4: "Telar Test Regular",
    6: "TelarTest-Regular",
    25: "TelarTest",
}
for name_id, value in replacements.items():
    names.setName(value, name_id, 3, 1, 0x409)
font.save(target)
