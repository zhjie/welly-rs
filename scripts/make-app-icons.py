#!/usr/bin/env python3
import struct
import sys
import zlib
from pathlib import Path


TRANSPARENT = (0, 0, 0, 0)
BG = (17, 19, 24, 255)
PANEL = (23, 27, 34, 255)
GRID = (44, 52, 64, 255)
TEXT = (232, 242, 255, 255)
ACCENT = (54, 240, 194, 255)
RED = (255, 95, 87, 255)
YELLOW = (255, 189, 46, 255)
GREEN = (40, 200, 64, 255)


def blend(dst, src):
    sr, sg, sb, sa = src
    if sa == 255:
        return src
    if sa == 0:
        return dst
    dr, dg, db, da = dst
    alpha = sa / 255.0
    inv = 1.0 - alpha
    return (
        round(sr * alpha + dr * inv),
        round(sg * alpha + dg * inv),
        round(sb * alpha + db * inv),
        255 if da else sa,
    )


def rounded_rect_mask(px, py, x, y, w, h, r):
    dx = max(x - px, 0, px - (x + w - 1))
    dy = max(y - py, 0, py - (y + h - 1))
    if dx == 0 and dy == 0:
        if x + r <= px < x + w - r or y + r <= py < y + h - r:
            return True
        cx = x + r if px < x + r else x + w - r - 1
        cy = y + r if py < y + r else y + h - r - 1
        return (px - cx) * (px - cx) + (py - cy) * (py - cy) <= r * r
    return False


def fill_rounded_rect(img, size, x, y, w, h, r, color):
    for py in range(y, y + h):
        if py < 0 or py >= size:
            continue
        for px in range(x, x + w):
            if px < 0 or px >= size:
                continue
            if rounded_rect_mask(px, py, x, y, w, h, r):
                img[py][px] = blend(img[py][px], color)


def fill_rect(img, size, x, y, w, h, color):
    for py in range(max(0, y), min(size, y + h)):
        for px in range(max(0, x), min(size, x + w)):
            img[py][px] = blend(img[py][px], color)


def fill_circle(img, size, cx, cy, r, color):
    rr = r * r
    for py in range(max(0, cy - r), min(size, cy + r + 1)):
        for px in range(max(0, cx - r), min(size, cx + r + 1)):
            if (px - cx) * (px - cx) + (py - cy) * (py - cy) <= rr:
                img[py][px] = blend(img[py][px], color)


def point_in_polygon(x, y, points):
    inside = False
    j = len(points) - 1
    for i, (xi, yi) in enumerate(points):
        xj, yj = points[j]
        if (yi > y) != (yj > y):
            x_intersect = (xj - xi) * (y - yi) / (yj - yi) + xi
            if x < x_intersect:
                inside = not inside
        j = i
    return inside


def fill_polygon(img, size, points, color):
    min_x = max(0, int(min(p[0] for p in points)))
    max_x = min(size - 1, int(max(p[0] for p in points)))
    min_y = max(0, int(min(p[1] for p in points)))
    max_y = min(size - 1, int(max(p[1] for p in points)))
    for py in range(min_y, max_y + 1):
        for px in range(min_x, max_x + 1):
            if point_in_polygon(px + 0.5, py + 0.5, points):
                img[py][px] = blend(img[py][px], color)


def png_bytes(img):
    height = len(img)
    width = len(img[0])
    raw = bytearray()
    for row in img:
        raw.append(0)
        for r, g, b, a in row:
            raw.extend((r, g, b, a))

    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    png = bytearray(b"\x89PNG\r\n\x1a\n")
    png.extend(chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)))
    png.extend(chunk(b"IDAT", zlib.compress(bytes(raw), 9)))
    png.extend(chunk(b"IEND", b""))
    return bytes(png)


def write_png(path, img):
    path.write_bytes(png_bytes(img))


def write_icns(path, pngs):
    type_for_size = {
        16: b"icp4",
        32: b"icp5",
        64: b"icp6",
        128: b"ic07",
        256: b"ic08",
        512: b"ic09",
        1024: b"ic10",
    }
    chunks = []
    for size in (16, 32, 64, 128, 256, 512, 1024):
        data = pngs[size]
        chunks.append(type_for_size[size] + struct.pack(">I", len(data) + 8) + data)
    body = b"".join(chunks)
    path.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)


def write_ico(path, pngs):
    sizes = (16, 24, 32, 48, 64, 128, 256)
    entries = []
    data_offset = 6 + len(sizes) * 16
    for index, size in enumerate(sizes, start=1):
        data = pngs[size]
        width_byte = 0 if size == 256 else size
        entries.append(
            struct.pack(
                "<BBBBHHII",
                width_byte,
                width_byte,
                0,
                0,
                1,
                32,
                len(data),
                data_offset,
            )
        )
        data_offset += len(data)

    body = b"".join(entries) + b"".join(pngs[size] for size in sizes)
    path.write_bytes(struct.pack("<HHH", 0, 1, len(sizes)) + body)


def write_rgba(path, img):
    height = len(img)
    width = len(img[0])
    raw = bytearray(struct.pack(">II", width, height))
    for row in img:
        for r, g, b, a in row:
            raw.extend((r, g, b, a))
    path.write_bytes(raw)


def draw_icon(size):
    img = [[TRANSPARENT for _ in range(size)] for _ in range(size)]
    scale = size / 1024.0

    def s(value):
        return round(value * scale)

    fill_rounded_rect(img, size, 0, 0, size, size, s(208), BG)
    fill_rounded_rect(img, size, s(96), s(112), s(832), s(800), s(128), PANEL)
    fill_rounded_rect(img, size, s(168), s(240), s(688), s(32), s(16), GRID)
    fill_circle(img, size, s(236), s(184), s(24), RED)
    fill_circle(img, size, s(316), s(184), s(24), YELLOW)
    fill_circle(img, size, s(396), s(184), s(24), GREEN)

    w_points = [
        (s(116), s(356)),
        (s(236), s(356)),
        (s(326), s(668)),
        (s(416), s(356)),
        (s(486), s(356)),
        (s(576), s(668)),
        (s(666), s(356)),
        (s(788), s(356)),
        (s(644), s(788)),
        (s(546), s(788)),
        (s(452), s(486)),
        (s(358), s(788)),
        (s(260), s(788)),
    ]
    fill_polygon(img, size, w_points, TEXT)
    fill_rounded_rect(img, size, s(692), s(716), s(156), s(44), s(22), ACCENT)
    return img


def main():
    out_dir = Path(sys.argv[1])
    out_dir.mkdir(parents=True, exist_ok=True)
    pngs = {}
    for size in (16, 32, 128, 256, 512):
        base = draw_icon(size)
        retina = draw_icon(size * 2)
        write_png(out_dir / f"icon_{size}x{size}.png", base)
        write_png(out_dir / f"icon_{size}x{size}@2x.png", retina)
        pngs[size] = png_bytes(base)
        pngs[size * 2] = png_bytes(retina)
    for size in (24, 48):
        pngs[size] = png_bytes(draw_icon(size))

    if len(sys.argv) > 2:
        output_path = Path(sys.argv[2])
        if output_path.suffix == ".ico":
            write_ico(output_path, pngs)
        elif output_path.suffix == ".png":
            output_path.write_bytes(pngs[1024])
        elif output_path.suffix == ".rgba":
            write_rgba(output_path, draw_icon(1024))
        else:
            write_icns(output_path, pngs)


if __name__ == "__main__":
    main()
