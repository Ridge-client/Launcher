"""Rebuilds data/osint_tools.json from the Awesome-OSINT-List README.

    python3 tools/import_awesome_osint.py README.md data/extra_tools.json > data/osint_tools.json

Download the README first from
https://raw.githubusercontent.com/Astrosp/Awesome-OSINT-List/main/README.md

Each "- [Name](url) - description" list item becomes a tool. Its category is the nearest "#"/"##"
heading, plus the "###"/"####" sub-heading when there is one ("Messaging Apps & Communication / WhatsApp").
Struck-through items (~~...~~) are the list's own "dead" marks and are skipped. Ridge needs unique
tool names, so a name seen before gets its domain appended; an exact duplicate URL is kept once.
The optional second file (same schema as the output) adds tools the list lacks, after the list's own.
"""

import json
import re
import sys
from urllib.parse import urlparse

HEADING = re.compile(r"^(#{1,4})\s+(.*?)\s*$")
# "- [Name](https://...) - description", tolerating a doubled "- - " bullet
ITEM = re.compile(r"^\s*[-*]\s+(?:-\s+)?\[([^\]]+)\]\((https?://[^)\s]+)\)\s*(.*)$")
STRUCK = re.compile(r"^\s*[-*]\s+~")
MD_LINK = re.compile(r"\[([^\]]*)\]\([^)]*\)")
SKIP_SECTIONS = {"index"}


def clean(text: str) -> str:
    """Plain text: no markdown links, emphasis, inline code or HTML tags."""
    text = MD_LINK.sub(r"\1", text)
    text = re.sub(r"<[^>]+>", "", text)
    text = re.sub(r"[*_`]+", "", text)
    return re.sub(r"\s+", " ", text).strip()


ACRONYMS = {"AI", "IP", "DNS", "VIN", "LE", "ID", "CSE", "OSINT", "URL", "CAN", "LLM", "ML", "API"}
INDEX_LINK = re.compile(r"\[([^\]]+)\]\(#([^)]+)\)")


def slug(heading: str) -> str:
    """GitHub's anchor for a heading."""
    s = re.sub(r"[^\w\s-]", "", clean(heading).lower())
    return re.sub(r"\s", "-", s.strip())


def title(heading: str, index: dict[str, str]) -> str:
    """The section's name as the README's own index spells it, else the heading with SHOUTING fixed."""
    if slug(heading) in index:
        return index[slug(heading)]
    fix = lambda m: m.group(0) if m.group(0) in ACRONYMS or not m.group(0).isupper() else m.group(0).capitalize()
    return re.sub(r"[A-Za-z']+", fix, clean(heading))


def url_key(url: str) -> str:
    """Same page whatever the scheme, "www." or trailing slash."""
    u = urlparse(url.lower())
    return u.netloc.removeprefix("www.") + u.path.rstrip("/") + (f"?{u.query}" if u.query else "")


def main(path: str, extras_path: str | None = None) -> None:
    lines = open(path, encoding="utf-8").read().splitlines()
    index = {}
    for line in lines:
        if line.lstrip().startswith("[") and "](#" in line:
            index.update({anchor: name for name, anchor in INDEX_LINK.findall(line)})
    section, sub = None, None
    categories: dict[str, list[dict]] = {}
    seen_names: set[str] = set()
    seen_urls: set[str] = set()
    skipped_dead = 0

    for line in lines:
        h = HEADING.match(line)
        if h:
            level, raw = len(h.group(1)), clean(h.group(2))
            if level <= 2:
                section = "index" if raw.lower() == "index" else title(raw, index)
                sub = None
            elif section == "index":
                # "Some of other OSINT tools List" sits under the index
                section, sub = "Other OSINT Tool Lists", None
            elif section:
                sub = title(raw, index)
            continue
        if STRUCK.match(line):
            skipped_dead += 1
            continue
        m = ITEM.match(line)
        if not m or section is None or section.lower() in SKIP_SECTIONS:
            continue

        name, url, rest = clean(m.group(1)), m.group(2).rstrip(".,"), m.group(3)
        if not name or name.startswith("!"):
            continue
        key = url_key(url)
        if key in seen_urls:
            continue
        seen_urls.add(key)

        if name.lower() in seen_names:
            name = f"{name} ({urlparse(url).netloc.removeprefix('www.')})"
        n, base = 2, name
        while name.lower() in seen_names:
            name, n = f"{base} {n}", n + 1
        seen_names.add(name.lower())

        description = clean(re.sub(r"^\s*[-–—:]\s*", "", rest))
        category = f"{section} / {sub}" if sub else section
        categories.setdefault(category, []).append(
            {"name": name, "url": url, "description": description}
        )

    if extras_path:
        for cat in json.load(open(extras_path, encoding="utf-8"))["categories"]:
            for tool in cat["tools"]:
                key = url_key(tool["url"])
                if key not in seen_urls and tool["name"].lower() not in seen_names:
                    seen_urls.add(key)
                    seen_names.add(tool["name"].lower())
                    categories.setdefault(cat["name"], []).append(tool)

    out = {"categories": [{"name": c, "tools": t} for c, t in categories.items()]}
    json.dump(out, sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")
    total = sum(len(t) for t in categories.values())
    print(
        f"{total} tools in {len(categories)} categories; skipped {skipped_dead} struck-through (dead)",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else None)
