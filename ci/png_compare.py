#!/usr/bin/env python3
"""Checks a hot reload from two sim frames of the counter app.

BEFORE and AFTER are aera-host-sim PNGs (8-bit RGBA). The app bar's colour
must change (the reload recoloured it) while the body, with the count,
stays pixel for pixel the same (state survived). Standard library only.
"""
import struct
import sys
import zlib


def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", path
    pos, idat = 8, b""
    while pos < len(data):
        length, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + length]
        if kind == b"IHDR":
            width, height, depth, colour = struct.unpack(">IIBB", body[:10])
            assert (depth, colour) == (8, 6), "expected 8-bit RGBA"
        elif kind == b"IDAT":
            idat += body
        pos += 12 + length
    raw, bpp, stride = zlib.decompress(idat), 4, width * 4
    rows, prev = [], bytearray(stride)
    for y in range(height):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for x in range(stride):
            a = line[x - bpp] if x >= bpp else 0
            b = prev[x]
            c = prev[x - bpp] if x >= bpp else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(bytes(line))
        prev = line
    return width, height, rows


def pixel(img, x, y):
    return tuple(img[2][y][x * 4:x * 4 + 3])


def region(img, x0, y0, x1, y1):
    return [img[2][y][x0 * 4:x1 * 4] for y in range(y0, y1)]


before, after = read_png(sys.argv[1]), read_png(sys.argv[2])
w, h = before[0], before[1]
bar_before, bar_after = pixel(before, w // 2, 60), pixel(after, w // 2, 60)
print(f"app bar {bar_before} -> {bar_after}")
if bar_before == bar_after:
    sys.exit("the reload did not change the app")
# Only the app bar was recoloured: the body, with the count in the middle,
# must be the same pixels (a lost state would show 0, not 3).
body = (0, h // 4, w, h * 3 // 4)
if region(before, *body) != region(after, *body):
    sys.exit("the count did not survive the reload")
print("body unchanged: state kept")
