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

