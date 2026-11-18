import struct

import ozaru

# Draw the first page of the PDF at twice the size of a point: 144 dots per inch.
png = ozaru.render_page("report.pdf", 1, scale=2)

with open("page-1.png", "wb") as file:
    file.write(png)

width, height = struct.unpack(">II", png[16:24])
print(f"page-1.png is {width} x {height} pixels")
