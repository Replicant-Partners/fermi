#!/usr/bin/env python3
"""Syntax-check every inline <script> in an HTML template.

WHY THIS EXISTS
===============

A template in this repo shipped a blank page. The cause: an HTML comment
placed inside a JavaScript template literal, containing backticks.

    card.innerHTML = `
      ...
      <!-- POSITION IS LOAD-BEARING: `Tabs.init` pairs by index -->
      ...
    `;

The first backtick in the comment ENDS the template literal. Everything after
it is parsed as code, so `Tabs.init` became a bare identifier and the browser
reported `Uncaught SyntaxError: Unexpected identifier 'Tabs'`. Inside a
template literal an HTML comment is not a comment; it is string content.

WHY THE OBVIOUS CHECK MISSED IT
===============================

The check that let it through was a regex:

    re.findall(r"<script>(.*?)</script>", html, re.S)

Non-greedy, so it stops at the first `</script>` — including one appearing
inside a string — and the "biggest match" heuristic then picked a fragment
rather than the script containing the bug. `agent_detail.html` has FIVE inline
scripts and that approach checked one truncated one.

`html.parser` treats script content as CDATA and gets the boundaries right,
which is the whole difference. Use a parser, not a regex, for a parsed format.

USAGE
=====

    scripts/lint-inline-js.py templates/*.html

Exits non-zero on the first syntax error. Requires `node` on PATH; a caller
that cannot guarantee node should skip rather than assume a pass.
"""

import sys, subprocess
from html.parser import HTMLParser

class Scripts(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=False)
        self.out, self._in, self._src = [], False, None
    def handle_starttag(self, tag, attrs):
        if tag == "script":
            d = dict(attrs)
            self._src = d.get("src")
            self._in = self._src is None
    def handle_endtag(self, tag):
        if tag == "script": self._in = False
    def handle_data(self, data):
        if self._in and data.strip(): self.out.append((self.getpos()[0], data))

bad = 0
for path in sys.argv[1:]:
    p = Scripts(); p.feed(open(path, encoding="utf-8").read())
    for line, body in p.out:
        open("/tmp/_chk.js","w",encoding="utf-8").write(body)
        r = subprocess.run(["node","--check","/tmp/_chk.js"], capture_output=True, text=True)
        status = "OK" if r.returncode == 0 else "SYNTAX ERROR"
        print(f"{status:12} {path} inline script starting line {line} ({len(body)} chars)")
        if r.returncode:
            bad += 1
            for l in r.stderr.splitlines()[:6]:
                print("             ", l)
sys.exit(1 if bad else 0)
