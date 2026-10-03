#!/usr/bin/env python3
"""Generuje pliki testowe ataków z rozdziału 12 (tylko biblioteka standardowa).

Bazowe zdjęcie photo.jpg (gradient 640x480, bez EXIF) jest w repo; odtwarzanie:
    convert -size 640x480 gradient:navy-orange -strip -quality 85 photo.jpg
Pozostałe pliki powstają z niego tym skryptem:
    python3 tests/security-fixtures/generate.py

Pliku EICAR celowo nie trzymamy w repo (antywirusy na komputerach z klonem
repozytorium by go usuwały); test e2e tworzy go w locie.
"""

import pathlib
import struct
import zlib

HERE = pathlib.Path(__file__).resolve().parent
XSS = b"<script>alert(document.domain)</script>"


def exif_app1(fields):
    """Segment APP1 z EXIF (TIFF little endian) z polami ASCII."""
    count = len(fields)
    data_start = 8 + 2 + count * 12 + 4
    entries, data = [], b""
    for tag, value in sorted(fields):
        value = value + b"\0"
        entries.append(struct.pack("<HHII", tag, 2, len(value), data_start + len(data)))
        data += value
    tiff = b"II*\0" + struct.pack("<I", 8) + struct.pack("<H", count) + b"".join(entries) + b"\0\0\0\0" + data
    payload = b"Exif\0\0" + tiff
    return b"\xff\xe1" + struct.pack(">H", len(payload) + 2) + payload


def png_chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)


def png(width, height, rows):
    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + png_chunk(b"IHDR", header) + png_chunk(b"IDAT", zlib.compress(rows)) + png_chunk(b"IEND", b"")


def main():
    photo = (HERE / "photo.jpg").read_bytes()
    assert photo[:2] == b"\xff\xd8", "photo.jpg musi być JPEG-iem"

    # Scenariusz 2: XSS w polach EXIF ImageDescription i Artist.
    app1 = exif_app1([(0x010E, XSS), (0x013B, XSS)])
    (HERE / "exif-xss.jpg").write_bytes(photo[:2] + app1 + photo[2:])

    # Scenariusz 3: SVG ze skryptem.
    (HERE / "svg-script.svg").write_bytes(
        b'<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><script>alert(1)</script></svg>\n'
    )

    # Scenariusz 4: nagłówek pliku wykonywalnego PE z rozszerzeniem .jpg.
    (HERE / "exe-renamed.jpg").write_bytes(b"MZ\x90\x00" + b"\x00" * 60 + b"This program cannot be run in DOS mode.\r\n" + b"\x00" * 400)

    # Scenariusz 5: poprawny PNG z doklejonym HTML-em (poliglota).
    rows = b"".join(b"\x00" + b"".join(bytes([x * 8, y * 8, 128]) for x in range(32)) for y in range(32))
    (HERE / "polyglot.png").write_bytes(png(32, 32, rows) + b"<html><body>" + XSS + b"</body></html>")

    # Scenariusz 6: bomba dekompresyjna, nagłówek deklaruje 100 000 x 100 000 px.
    (HERE / "bomb.png").write_bytes(png(100_000, 100_000, b"\x00" * 64))


if __name__ == "__main__":
    main()
