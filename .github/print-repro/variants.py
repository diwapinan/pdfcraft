"""From thai.pdf (Thai added text, which crashed the Windows print), write variants that each change
one thing about PdfCraft's Type 3 fonts or the item's stream, plus a minimal textbook Type 3 PDF."""
import re, sys
import pikepdf
from pikepdf import Name, Dictionary, Array

src, out = sys.argv[1], sys.argv[2]

def type3_fonts(pdf):
    for page in pdf.pages:
        fonts = page.obj.get("/Resources", {}).get("/Font", {})
        for _, f in fonts.items():
            if f.get("/Subtype") == Name.Type3:
                yield f

def added_streams(pdf):
    for page in pdf.pages:
        contents = page.obj.get("/Contents")
        items = contents if isinstance(contents, Array) else [contents]
        for s in items:
            if s.get("/PCMark") == Name("/Added"):
                yield s

def variant(name, change):
    pdf = pikepdf.open(src)
    change(pdf)
    pdf.save(f"{out}/{name}.pdf")

def resources(pdf):
    for f in type3_fonts(pdf):
        f.Resources = Dictionary()

def no_descriptor(pdf):
    for f in type3_fonts(pdf):
        if "/FontDescriptor" in f:
            del f["/FontDescriptor"]

def empty_glyphs_d0(pdf):
    for f in type3_fonts(pdf):
        for _, proc in f.CharProcs.items():
            data = proc.read_bytes().decode("latin1")
            if " m\n" not in data and " l\n" not in data:
                w = data.split()[0]
                proc.write(f"{w} 0 d0\n".encode())

def bbox(pdf):
    for f in type3_fonts(pdf):
        f.FontBBox = Array([0, -300, 1000, 1000])

def no_tounicode(pdf):
    for f in type3_fonts(pdf):
        if "/ToUnicode" in f:
            del f["/ToUnicode"]

def no_span(pdf):
    for s in added_streams(pdf):
        data = s.read_bytes().decode("latin1")
        data = re.sub(r"/Span <</ActualText <[0-9A-F]*>>> BDC\n", "", data).replace("EMC\n", "")
        s.write(data.encode("latin1"))

def integer_widths(pdf):
    for f in type3_fonts(pdf):
        f.Widths = Array([int(float(w)) for w in f.Widths])

def everything(pdf):
    for c in (resources, no_descriptor, empty_glyphs_d0, bbox, no_tounicode, no_span, integer_widths):
        c(pdf)

for name, change in [
    ("v-resources", resources),
    ("v-no-descriptor", no_descriptor),
    ("v-empty-glyphs-d0", empty_glyphs_d0),
    ("v-bbox", bbox),
    ("v-no-tounicode", no_tounicode),
    ("v-no-span", no_span),
    ("v-integer-widths", integer_widths),
    ("v-everything", everything),
]:
    variant(name, change)

# A textbook Type 3 font: one square glyph, code 65, with /Resources.
pdf = pikepdf.new()
proc = pdf.make_stream(b"1000 0 0 0 750 750 d1\n0 0 750 750 re f\n")
font = pdf.make_indirect(Dictionary(Type=Name.Font, Subtype=Name.Type3, FontBBox=Array([0, 0, 750, 750]),
    FontMatrix=Array([0.001, 0, 0, 0.001, 0, 0]), CharProcs=Dictionary(sq=proc),
    Encoding=Dictionary(Type=Name.Encoding, Differences=Array([65, Name.sq])), FirstChar=65, LastChar=65,
    Widths=Array([1000]), Resources=Dictionary()))
content = pdf.make_stream(b"BT /F1 24 Tf 72 720 Td (AAAA) Tj ET")
page = pikepdf.Page(Dictionary(Type=Name.Page, MediaBox=Array([0, 0, 612, 792]), Contents=content, Resources=Dictionary(Font=Dictionary(F1=font))))
pdf.pages.append(page)
pdf.save(f"{out}/v-textbook-type3.pdf")
