#!/usr/bin/env python3
"""site/index.fragment.html (the source) -> site/index.html (standalone page). Run after editing the fragment."""
import pathlib
here = pathlib.Path(__file__).parent
frag = (here / "index.fragment.html").read_text()
i = frag.index("</style>") + len("</style>")
page = ('<!doctype html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n'
        '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">\n'
        '<link rel="icon" href="mark.svg">\n' + frag[:i] + '\n</head>\n<body>' + frag[i:] + '</body>\n</html>\n')
(here / "index.html").write_text(page)
print("wrote", here / "index.html")
