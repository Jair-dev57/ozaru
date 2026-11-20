"""A small JBIG2 encoder (ITU-T T.88), to write the corpus's JBIG2 images.

It writes what the engine's decoder reads with the arithmetic coder: generic
regions with each template, typical prediction and adaptive pixels, symbol
dictionaries and text regions. Written from the standard's encoding
procedures (Annex E.2 and Annex A), so that MuPDF, which decodes the result
with its own decoder, checks both this encoder and the engine.

Bitmaps are lists of rows of 0 and 1, 1 for black.
"""

from __future__ import annotations

import struct

# Table E.1: Qe, the next state after an MPS and after an LPS, and SWITCH.
STATES = [
    (0x5601, 1, 1, 1), (0x3401, 2, 6, 0), (0x1801, 3, 9, 0), (0x0AC1, 4, 12, 0),
    (0x0521, 5, 29, 0), (0x0221, 38, 33, 0), (0x5601, 7, 6, 1), (0x5401, 8, 14, 0),
    (0x4801, 9, 14, 0), (0x3801, 10, 14, 0), (0x3001, 11, 17, 0), (0x2401, 12, 18, 0),
    (0x1C01, 13, 20, 0), (0x1601, 29, 21, 0), (0x5601, 15, 14, 1), (0x5401, 16, 14, 0),
    (0x5101, 17, 15, 0), (0x4801, 18, 16, 0), (0x3801, 19, 17, 0), (0x3401, 20, 18, 0),
    (0x3001, 21, 19, 0), (0x2801, 22, 19, 0), (0x2401, 23, 20, 0), (0x2201, 24, 21, 0),
    (0x1C01, 25, 22, 0), (0x1801, 26, 23, 0), (0x1601, 27, 24, 0), (0x1401, 28, 25, 0),
    (0x1201, 29, 26, 0), (0x1101, 30, 27, 0), (0x0AC1, 31, 28, 0), (0x09C1, 32, 29, 0),
    (0x08A1, 33, 30, 0), (0x0521, 34, 31, 0), (0x0441, 35, 32, 0), (0x02A1, 36, 33, 0),
    (0x0221, 37, 34, 0), (0x0141, 38, 35, 0), (0x0111, 39, 36, 0), (0x0085, 40, 37, 0),
    (0x0049, 41, 38, 0), (0x0025, 42, 39, 0), (0x0015, 43, 40, 0), (0x0009, 44, 41, 0),
    (0x0005, 45, 42, 0), (0x0001, 45, 43, 0), (0x5601, 46, 46, 0),
]


class Encoder:
    """The MQ arithmetic encoder of Annex E.2."""

    def __init__(self) -> None:
        self.a = 0x8000
        self.c = 0
        self.ct = 12
        self.out = bytearray()
        self.b: int | None = None

    def _emit(self, value: int) -> None:
        if self.b is not None:
            self.out.append(self.b)
        self.b = value & 0xFF

    def _byte_out(self) -> None:
        if self.b == 0xFF:
            self._emit(self.c >> 20)
            self.c &= 0xFFFFF
            self.ct = 7
        elif self.c < 0x8000000:
            self._emit(self.c >> 19)
            self.c &= 0x7FFFF
            self.ct = 8
        else:
            self.b = (self.b or 0) + 1
            if self.b == 0xFF:
                self.c &= 0x7FFFFFF
                self._emit(self.c >> 20)
                self.c &= 0xFFFFF
                self.ct = 7
            else:
                self._emit(self.c >> 19)
                self.c &= 0x7FFFF
                self.ct = 8

    def _renorm(self) -> None:
        while True:
            self.a = (self.a << 1) & 0xFFFF
            self.c = (self.c << 1) & 0xFFFFFFFF
            self.ct -= 1
            if self.ct == 0:
                self._byte_out()
            if self.a & 0x8000:
                break

    def encode(self, contexts: list[int], index: int, bit: int) -> None:
        state, mps = contexts[index] >> 1, contexts[index] & 1
        qe, next_mps, next_lps, switch = STATES[state]
        self.a -= qe
        if bit == mps:
            if self.a & 0x8000 == 0:
                if self.a < qe:
                    self.a = qe
                else:
                    self.c += qe
                contexts[index] = next_mps << 1 | mps
                self._renorm()
            else:
                self.c += qe
        else:
            if self.a < qe:
                self.c += qe
            else:
                self.a = qe
            if switch:
                mps = 1 - mps
            contexts[index] = next_lps << 1 | mps
            self._renorm()

    def flush(self) -> bytes:
        # SETBITS.
        temp = self.c + self.a
        self.c |= 0xFFFF
        if self.c >= temp:
            self.c -= 0x8000
        self.c <<= self.ct
        self._byte_out()
        self.c <<= self.ct
        self._byte_out()
        # The data ends with the marker 0xFF 0xAC.
        if self.b != 0xFF:
            self._emit(0xFF)
        self.out.append(self.b)
        self.out.append(0xAC)
        self.b = None
        return bytes(self.out)


class Integers:
    """One integer encoding procedure, such as IADH (Annex A.2)."""

    def __init__(self) -> None:
        self.contexts = [0] * 512

    def encode(self, encoder: Encoder, value: int | None) -> None:
        previous = 1

        def bit(b: int) -> None:
            nonlocal previous
            encoder.encode(self.contexts, previous, b)
            previous = previous << 1 | b if previous < 256 else ((previous << 1 | b) & 511) | 256

        if value is None:  # OOB
            sign, magnitude = 1, 0
        else:
            sign, magnitude = (1, -value) if value < 0 else (0, value)
        bit(sign)
        for prefix, bits, offset in [
            ([0], 2, 0), ([1, 0], 4, 4), ([1, 1, 0], 6, 20),
            ([1, 1, 1, 0], 8, 84), ([1, 1, 1, 1, 0], 12, 340), ([1, 1, 1, 1, 1], 32, 4436),
        ]:
            if value is None or magnitude - offset < 1 << bits:
                for b in prefix:
                    bit(b)
                rest = 0 if value is None else magnitude - offset
                for i in reversed(range(bits)):
                    bit(rest >> i & 1)
                return


class Ids:
    """IAID, symbol numbers of `length` bits (Annex A.3)."""

    def __init__(self, length: int) -> None:
        self.length = length
        self.contexts = [0] * (1 << length)

    def encode(self, encoder: Encoder, value: int) -> None:
        previous = 1
        for i in reversed(range(self.length)):
            b = value >> i & 1
            encoder.encode(self.contexts, previous, b)
            previous = previous << 1 | b


DEFAULT_ADAPTIVE = {0: [(3, -1), (-3, -1), (2, -2), (-2, -2)], 1: [(3, -1)], 2: [(2, -1)], 3: [(2, -1)]}
TYPICAL = {0: 0x9B25, 1: 0x0795, 2: 0x00E5, 3: 0x0195}


def context(bitmap, x, y, template, adaptive) -> int:
    height, width = len(bitmap), len(bitmap[0]) if bitmap else 0

    def p(dx, dy):
        xx, yy = x + dx, y + dy
        return bitmap[yy][xx] if 0 <= xx < width and 0 <= yy < height else 0

    if template == 0:
        a1, a2, a3, a4 = adaptive
        pixels = [(-1, 0), (-2, 0), (-3, 0), (-4, 0), a1, (2, -1), (1, -1), (0, -1),
                  (-1, -1), (-2, -1), a2, a3, (1, -2), (0, -2), (-1, -2), a4]
    elif template == 1:
        pixels = [(-1, 0), (-2, 0), (-3, 0), adaptive[0], (2, -1), (1, -1), (0, -1),
                  (-1, -1), (-2, -1), (2, -2), (1, -2), (0, -2), (-1, -2)]
    elif template == 2:
        pixels = [(-1, 0), (-2, 0), adaptive[0], (1, -1), (0, -1), (-1, -1), (-2, -1),
                  (1, -2), (0, -2), (-1, -2)]
    else:
        pixels = [(-1, 0), (-2, 0), (-3, 0), (-4, 0), adaptive[0], (1, -1), (0, -1),
                  (-1, -1), (-2, -1), (-3, -1)]
    value = 0
    for shift, (dx, dy) in enumerate(pixels):
        value |= p(dx, dy) << shift
    return value


def encode_generic(encoder: Encoder, contexts: list[int], bitmap, template=0, typical=False, adaptive=None):
    """A generic region (section 6.2.5), into `encoder`."""
    adaptive = adaptive or DEFAULT_ADAPTIVE[template]
    ltp = 0
    for y, row in enumerate(bitmap):
        if typical:
            same = y > 0 and row == bitmap[y - 1]
            # LTP changes when this row's prediction differs from the last.
            encoder.encode(contexts, TYPICAL[template], int(same) ^ ltp)
            ltp = int(same)
            if same:
                continue
        for x, pixel in enumerate(row):
            encoder.encode(contexts, context(bitmap, x, y, template, adaptive), pixel)


def segment(number: int, kind: int, data: bytes, referred: list[int] = ()) -> bytes:
    """A segment header (section 7.2) and its data, of page 1."""
    header = struct.pack(">IB", number, kind)
    header += bytes([len(referred) << 5]) + bytes(referred)
    header += bytes([1]) + struct.pack(">I", len(data))
    return header + data


def page_info(width: int, height: int) -> bytes:
    return struct.pack(">IIIIBH", width, height, 0, 0, 0, 0)


def region_info(width: int, height: int, x: int = 0, y: int = 0, combination: int = 0) -> bytes:
    return struct.pack(">IIIIB", width, height, x, y, combination)


def generic_region(bitmap, x=0, y=0, template=0, typical=False, adaptive=None, combination=0) -> bytes:
    """The data of an immediate generic region segment (section 7.4.6)."""
    adaptive = adaptive or DEFAULT_ADAPTIVE[template]
    flags = template << 1 | (8 if typical else 0)
    data = region_info(len(bitmap[0]), len(bitmap), x, y, combination) + bytes([flags])
    data += b"".join(struct.pack(">bb", dx, dy) for dx, dy in adaptive)
    encoder = Encoder()
    encode_generic(encoder, [0] * (1 << 16), bitmap, template, typical, adaptive)
    return data + encoder.flush()


def mmr_region(width: int, height: int, data: bytes, x=0, y=0) -> bytes:
    """An immediate generic region coded with MMR: `data` is Group 4."""
    return region_info(width, height, x, y) + bytes([1]) + data


def symbol_dictionary(symbols) -> bytes:
    """A symbol dictionary (section 7.4.2) of `symbols`, all exported, coded
    with template 0, in height classes."""
    flags = 0
    data = struct.pack(">H", flags)
    data += b"".join(struct.pack(">bb", dx, dy) for dx, dy in DEFAULT_ADAPTIVE[0])
    data += struct.pack(">II", len(symbols), len(symbols))
    encoder = Encoder()
    dh, dw, ex = Integers(), Integers(), Integers()
    generic = [0] * (1 << 16)
    order = sorted(range(len(symbols)), key=lambda i: (len(symbols[i]), len(symbols[i][0])))
    height = 0
    i = 0
    numbering = []
    while i < len(order):
        class_height = len(symbols[order[i]])
        dh.encode(encoder, class_height - height)
        height = class_height
        width = 0
        while i < len(order) and len(symbols[order[i]]) == class_height:
            symbol = symbols[order[i]]
            dw.encode(encoder, len(symbol[0]) - width)
            width = len(symbol[0])
            encode_generic(encoder, generic, symbol)
            numbering.append(order[i])
            i += 1
        dw.encode(encoder, None)
    # None of the input symbols, which there aren't, then all the new ones.
    ex.encode(encoder, 0)
    ex.encode(encoder, len(symbols))
    return data + encoder.flush(), numbering


def text_region(width, height, symbols, instances, numbering, x=0, y=0) -> bytes:
    """A text region (section 7.4.3) placing `instances`, (symbol, x, y) of
    their top left corners, with `symbols` of a dictionary numbered as
    `numbering` says."""
    number_of = {original: new for new, original in enumerate(numbering)}
    flags = 1 << 4  # REFCORNER top left, one strip, OR.
    data = region_info(width, height, x, y) + struct.pack(">H", flags)
    data += struct.pack(">I", len(instances))
    encoder = Encoder()
    dt, fs, ds = Integers(), Integers(), Integers()
    length = max(0, (len(symbols) - 1).bit_length())
    ids = Ids(length)
    by_strip: dict[int, list] = {}
    for symbol, sx, sy in instances:
        by_strip.setdefault(sy, []).append((sx, symbol))
    dt.encode(encoder, 0)
    strip_t = 0
    first_s = 0
    for t in sorted(by_strip):
        dt.encode(encoder, t - strip_t)
        strip_t = t
        current = None
        for sx, symbol in sorted(by_strip[t]):
            if current is None:
                fs.encode(encoder, sx - first_s)
                first_s = sx
            else:
                ds.encode(encoder, sx - current)
            ids.encode(encoder, number_of[symbol])
            current = sx + len(symbols[symbol][0]) - 1
        ds.encode(encoder, None)
    return data + encoder.flush()



# Huffman coding (Annex B).

# The standard tables: lines of (prefix length, range length, low), then the
# lower range line (prefix, low), the upper one (prefix, low), and the prefix
# of OOB.
STANDARD = {
    1: ([(1, 4, 0), (2, 8, 16), (3, 16, 272)], None, (3, 65808), None),
    2: ([(1, 0, 0), (2, 0, 1), (3, 0, 2), (4, 3, 3), (5, 6, 11)], None, (6, 75), 6),
    4: ([(1, 0, 1), (2, 0, 2), (3, 0, 3), (4, 3, 4), (5, 6, 12)], None, (5, 76), None),
    6: ([(5, 10, -2048), (4, 9, -1024), (4, 8, -512), (4, 7, -256), (5, 6, -128), (5, 5, -64),
         (4, 5, -32), (2, 7, 0), (3, 7, 128), (3, 8, 256), (4, 9, 512), (4, 10, 1024)],
        (6, -2049), (6, 2048), None),
    8: ([(8, 3, -15), (9, 1, -7), (8, 1, -5), (9, 0, -3), (7, 0, -2), (4, 0, -1), (2, 1, 0),
         (5, 0, 2), (6, 0, 3), (3, 4, 4), (6, 1, 20), (4, 4, 22), (4, 5, 38), (5, 6, 70),
         (5, 7, 134), (6, 7, 262), (7, 8, 390), (6, 10, 646)],
        (9, -16), (9, 1670), 2),
    11: ([(1, 0, 1), (2, 1, 2), (4, 0, 4), (4, 1, 5), (5, 1, 7), (5, 2, 9), (6, 2, 13), (7, 2, 17),
          (7, 3, 21), (7, 4, 29), (7, 5, 45), (7, 6, 77)], None, (7, 141), None),
}


class BitWriter:
    def __init__(self) -> None:
        self.bits: list[int] = []

    def write(self, value: int, count: int) -> None:
        for i in reversed(range(count)):
            self.bits.append(value >> i & 1)

    def align(self) -> None:
        while len(self.bits) % 8:
            self.bits.append(0)

    def bytes(self) -> bytes:
        self.align()
        return bytes(
            int("".join(map(str, self.bits[i : i + 8])), 2) for i in range(0, len(self.bits), 8)
        )

    def extend(self, data: bytes) -> None:
        self.align()
        for byte in data:
            self.write(byte, 8)


def canonical(prefixes: list[int]) -> list[int | None]:
    """The codes of lines of these prefix lengths (section B.3)."""
    longest = max(prefixes, default=0)
    counts = [0] * (longest + 1)
    for p in prefixes:
        counts[p] += 1
    counts[0] = 0
    first = [0] * (longest + 1)
    for length in range(1, longest + 1):
        first[length] = (first[length - 1] + counts[length - 1]) << 1
    codes = []
    for p in prefixes:
        if p == 0:
            codes.append(None)
        else:
            codes.append(first[p])
            first[p] += 1
    return codes


class HuffmanTable:
    def __init__(self, number: int) -> None:
        normal, lower, upper, oob = STANDARD[number]
        self.lines = [("normal", p, r, low) for p, r, low in normal]
        if lower:
            self.lines.append(("lower", lower[0], 32, lower[1]))
        self.lines.append(("upper", upper[0], 32, upper[1]))
        if oob is not None:
            self.lines.append(("oob", oob, 0, 0))
        self.codes = canonical([line[1] for line in self.lines])

    def write(self, out: BitWriter, value: int | None) -> None:
        for (kind, prefix, range_, low), code in zip(self.lines, self.codes):
            if code is None:
                continue
            if value is None:
                if kind == "oob":
                    out.write(code, prefix)
                    return
                continue
            if kind == "normal" and low <= value < low + (1 << range_):
                offset = value - low
            elif kind == "lower" and value <= low:
                offset = low - value
            elif kind == "upper" and value >= low:
                offset = value - low
            else:
                continue
            out.write(code, prefix)
            out.write(offset, range_)
            return
        raise ValueError(f"{value} isn't in the table")


def symbol_dictionary_huffman(symbols, mmr) -> tuple[bytes, list[int]]:
    """A symbol dictionary coded with the standard Huffman tables B.4, B.2
    and B.1, each height class's symbols as one bitmap: uncompressed, or coded
    with `mmr`, a function of a bitmap that gives its Group 4 data, for the
    classes of an odd height."""
    data = struct.pack(">H", 1)  # SDHUFF.
    data += struct.pack(">II", len(symbols), len(symbols))
    out = BitWriter()
    dh, dw, size = HuffmanTable(4), HuffmanTable(2), HuffmanTable(1)
    order = sorted(range(len(symbols)), key=lambda i: (len(symbols[i]), len(symbols[i][0])))
    numbering = []
    height = 0
    i = 0
    while i < len(order):
        class_height = len(symbols[order[i]])
        dh.write(out, class_height - height)
        height = class_height
        width = 0
        members = []
        while i < len(order) and len(symbols[order[i]]) == class_height:
            symbol = symbols[order[i]]
            dw.write(out, len(symbol[0]) - width)
            width = len(symbol[0])
            members.append(symbol)
            numbering.append(order[i])
            i += 1
        dw.write(out, None)
        collective = [sum((s[y] for s in members), []) for y in range(class_height)]
        total = len(collective[0])
        if class_height % 2:
            coded = mmr(collective)
            size.write(out, len(coded))
            out.extend(coded)
        else:
            size.write(out, 0)
            out.align()
            for row in collective:
                padded = row + [0] * (-total % 8)
                out.extend(bytes(int("".join(map(str, padded[j : j + 8])), 2) for j in range(0, len(padded), 8)))
    ex = HuffmanTable(1)
    ex.write(out, 0)
    ex.write(out, len(symbols))
    return data + out.bytes(), numbering


def text_region_huffman(width, height, symbols, instances, numbering, x=0, y=0) -> bytes:
    """A text region coded with the standard Huffman tables B.6, B.8 and
    B.11, and a table of symbol numbers of the same length each."""
    number_of = {original: new for new, original in enumerate(numbering)}
    flags = 1 | 1 << 4  # SBHUFF, REFCORNER top left, one strip, OR.
    data = region_info(width, height, x, y) + struct.pack(">H", flags)
    data += struct.pack(">H", 0)  # B.6, B.8, B.11 and B.14, B.1.
    data += struct.pack(">I", len(instances))
    out = BitWriter()
    length = max(1, (len(symbols) - 1).bit_length())
    # The run codes: only the one of `length`, of one bit.
    for code in range(35):
        out.write(1 if code == length else 0, 4)
    for _ in symbols:
        out.write(0, 1)
    out.align()
    fs, ds, dt = HuffmanTable(6), HuffmanTable(8), HuffmanTable(11)
    by_strip: dict[int, list] = {}
    for symbol, sx, sy in instances:
        by_strip.setdefault(sy, []).append((sx, symbol))
    # B.11 has no 0: the first T is -1, so each strip's DT is at least 1.
    dt.write(out, 1)
    strip_t = -1
    first_s = 0
    for t in sorted(by_strip):
        dt.write(out, t - strip_t)
        strip_t = t
        current = None
        for sx, symbol in sorted(by_strip[t]):
            if current is None:
                fs.write(out, sx - first_s)
                first_s = sx
            else:
                ds.write(out, sx - current)
            out.write(number_of[symbol], length)
            current = sx + len(symbols[symbol][0]) - 1
        ds.write(out, None)
    return data + out.bytes()


def pattern_dictionary(patterns) -> bytes:
    """A pattern dictionary (section 7.4.4) of `patterns`, all of one size,
    coded with template 0, its first adaptive pixel a pattern to the left."""
    height, width = len(patterns[0]), len(patterns[0][0])
    data = struct.pack(">BBBI", 0, width, height, len(patterns) - 1)
    collective = [sum((p[y] for p in patterns), []) for y in range(height)]
    adaptive = [(-width, 0), (-3, -1), (2, -2), (-2, -2)]
    encoder = Encoder()
    encode_generic(encoder, [0] * (1 << 16), collective, 0, False, adaptive)
    return data + encoder.flush()


def halftone_region(width, height, grays, count, x0, y0, step_x, step_y) -> bytes:
    """A halftone region (section 7.4.5) of the grid `grays`, values below
    `count`, placed from `x0`, `y0` with the steps of the grid, in 1/256 of a
    pixel. The planes are coded with template 0, in Gray code."""
    grid_height, grid_width = len(grays), len(grays[0])
    planes = max(0, (count - 1).bit_length())
    data = region_info(width, height) + bytes([0])
    data += struct.pack(">IIiiHH", grid_width, grid_height, x0, y0, step_x, step_y)
    encoder = Encoder()
    contexts = [0] * (1 << 16)
    adaptive = [(3, -1), (-3, -1), (2, -2), (-2, -2)]
    for j in reversed(range(planes)):
        # Gray code: each plane is the bit XOR the bit above it.
        plane = [
            [(g >> j & 1) ^ ((g >> (j + 1) & 1) if j + 1 < planes else 0) for g in row]
            for row in grays
        ]
        encode_generic(encoder, contexts, plane, 0, False, adaptive)
    return data + encoder.flush()
