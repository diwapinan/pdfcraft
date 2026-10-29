"""A synthetic stand-in for a scanned A3 drawing: one landscape-turned (Rotate 270) A3 page that is
one large grayscale JPEG, like an office scan. No real drawing or personal data."""
import io, sys
from PIL import Image, ImageDraw
W, H = 4680, 6614
img = Image.new("RGB", (W, H), (250, 250, 250))
d = ImageDraw.Draw(img)
for x in range(0, W, 120):
    d.line([(x, 0), (x, H)], fill=(180, 180, 180), width=3)
for y in range(0, H, 120):
    d.line([(0, y), (W, y)], fill=(180, 180, 180), width=3)
buf = io.BytesIO()
img.save(buf, "JPEG", quality=70)
jpeg = buf.getvalue()
content = b"q 841.86 0 0 1190.52 0 0 cm /Im0 Do Q"
objs = [
    b"<< /Type /Catalog /Pages 2 0 R >>",
    b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 841.86 1190.52] /Rotate 270 /Contents 4 0 R /Resources << /XObject << /Im0 5 0 R >> >> >>",
    b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream",
    b"<< /Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length %d >>\nstream\n" % (W, H, len(jpeg)) + jpeg + b"\nendstream",
]
out = bytearray(b"%PDF-1.7\n")
offs = []
for i, o in enumerate(objs, 1):
    offs.append(len(out))
    out += b"%d 0 obj\n" % i + o + b"\nendobj\n"
x = len(out)
out += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
for o in offs:
    out += b"%010d 00000 n \n" % o
out += b"trailer << /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, x)
open(sys.argv[1], "wb").write(out)
