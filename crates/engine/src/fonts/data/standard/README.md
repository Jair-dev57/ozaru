# Fonts for the standard 14

PDF readers draw the standard 14 fonts, and fonts they don't find in a file,
with fonts of their own. These are the engine's:

- `OzaruStdSans-*`, `OzaruStdSerif-*` and `OzaruStdMono-*`, subsets of Liberation Sans, Serif and Mono (SIL Open
  Font License 1.1, see `LICENSE-Liberation.txt`), metric compatible with
  Helvetica or Arial, Times and Courier, for those fonts and their relatives.
- `DejaVuSans-Symbols.ttf` (Bitstream Vera and DejaVu license, see
  `LICENSE-DejaVu.txt`), for Symbol and ZapfDingbats, whose glyphs it has under
  their Unicode characters.

Their internal names are changed to `OzaruStd`, as the license asks of modified
versions that keep the Reserved Font Name. They are subsets, without hinting, to the characters of the Standard, WinAnsi
and Mac Roman encodings (the Liberation fonts) and to Greek, mathematical
operators, arrows, geometric shapes and Dingbats (DejaVu). Made with
fonttools:

    pyftsubset LiberationSans-Regular.ttf --unicodes="U+0020-007E,U+00A0-017F,U+0192,U+02C6,U+02C7,U+02D8-02DD,U+0394,U+03A9,U+03BC,U+03C0,U+2013-2014,U+2018-201E,U+2020-2022,U+2026,U+2030,U+2039-203A,U+20AC,U+2122,U+2202,U+2206,U+220F,U+2211,U+2212,U+221A,U+221E,U+222B,U+2248,U+2260,U+2264-2265,U+25CA,U+FB01-FB02,U+F8FF" --no-hinting --glyph-names --layout-features= --notdef-outline
    pyftsubset DejaVuSans.ttf --unicodes="U+0020-007E,U+00A0-00FF,U+0370-03FF,U+2000-206F,U+2190-21FF,U+2200-22FF,U+2300-23FF,U+2460-24FF,U+25A0-25FF,U+2600-27BF" --no-hinting --layout-features= --notdef-outline
