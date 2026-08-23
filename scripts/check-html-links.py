#!/usr/bin/env python3
"""Fail when a local link in Helm's HTML documentation is missing."""

from html.parser import HTMLParser
from pathlib import Path


class Links(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.links: list[str] = []

    def handle_starttag(
        self, tag: str, attrs: list[tuple[str, str | None]]
    ) -> None:
        if tag in {"a", "link"}:
            self.links.extend(value for key, value in attrs if key == "href" and value)


pages = sorted(Path("docs").rglob("*.html")) + [Path("README.html")]
for page in pages:
    parser = Links()
    parser.feed(page.read_text(encoding="utf-8"))
    for link in parser.links:
        if "://" in link or link.startswith(("#", "mailto:")):
            continue
        target = (page.parent / link.split("#", maxsplit=1)[0]).resolve()
        if not target.exists():
            raise SystemExit(f"{page}: missing local link {link}")
print(f"validated local links in {len(pages)} HTML documents")
