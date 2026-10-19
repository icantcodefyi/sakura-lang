"""Check local Markdown links and rendered docs routes/anchors (stdlib only).

Run against `bun run start`: python3 scripts/check-doc-links.py http://localhost:3000
External websites are not fetched. GitHub main-branch file links resolve locally.
"""
from html.parser import HTMLParser
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit, urljoin
from urllib.request import urlopen

ROOT = Path(__file__).resolve().parents[2]


class Page(HTMLParser):
    def __init__(self, html):
        super().__init__()
        self.ids = set()
        self.links = set()
        self.feed(html)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            self.ids.add(attrs["id"])
        if tag == "a" and "href" in attrs:
            self.links.add(attrs["href"])


def main():
    base = sys.argv[1].rstrip("/")
    files = [ROOT / name for name in ("README.md", "CONTRIBUTING.md", "STABILITY.md", "docs/mcp.md", "docs/release.md")]
    files += sorted((ROOT / "website/content/docs").rglob("*.mdx"))
    errors = []
    local = 0
    rendered = {}
    targets = set()
    for file in files:
        text = re.sub(r"```.*?```", "", file.read_text(), flags=re.S)
        for link in re.findall(r"\]\(([^\s)]+)\)", text):
            url = urlsplit(link)
            if link.startswith("/docs") or link.startswith("/llms"):
                targets.add(link)
                continue
            prefix = "https://github.com/icantcodefyi/sakuralang/"
            if link.startswith(prefix + "blob/main/") or link.startswith(prefix + "tree/main/"):
                target = ROOT / unquote(url.path.split("/main/", 1)[1])
            elif url.scheme or not url.path or url.path.startswith("/"):
                continue
            else:
                target = file.parent / unquote(url.path)
            local += 1
            if not target.exists():
                errors.append(f"{file.relative_to(ROOT)}: missing {link}")
        if "website/content/docs" in str(file):
            route = "/docs/" + str(file.relative_to(ROOT / "website/content/docs").with_suffix(""))
            targets.add(route.removesuffix("/index"))
    for target in sorted(targets):
        url = urlsplit(target)
        try:
            if url.path not in rendered:
                with urlopen(base + url.path, timeout=30) as response:
                    rendered[url.path] = Page(response.read().decode())
            if url.fragment and unquote(url.fragment) not in rendered[url.path].ids:
                errors.append(f"missing anchor: {target}")
        except Exception as error:
            errors.append(f"{target}: {error}")
    # Include links emitted by MDX components and navigation, not just Markdown prose.
    for route, page in list(rendered.items()):
        for link in page.links:
            url = urlsplit(urljoin(route, link))
            if not url.scheme and url.path.startswith("/docs"):
                if url.path not in rendered:
                    errors.append(f"{route}: unknown docs route {link}")
                elif url.fragment and unquote(url.fragment) not in rendered[url.path].ids:
                    errors.append(f"{route}: missing anchor {link}")
    for error in sorted(set(errors)):
        print(error)
    print(f"{local} local file links; {len(rendered)} rendered routes; {len(set(errors))} failures")
    return bool(errors)


if __name__ == "__main__":
    raise SystemExit(main())
