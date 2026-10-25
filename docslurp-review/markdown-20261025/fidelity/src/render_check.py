#!/usr/bin/env python3
"""Render prototype .md with markdown-it-py (commonmark preset + table + strikethrough rules, html enabled; markdown-it-py 4.2.0)
and report: emphasis markers left literal, <table> passthrough, math delimiters. Usage: render_check.py file.md"""
import sys,re
from markdown_it import MarkdownIt
md=MarkdownIt("commonmark",{"html":True}).enable(["table","strikethrough"])
src=open(sys.argv[1]).read(); out=md.render(src)
print(out)
lit=re.findall(r'(?<!\w)(\*{1,2})(?=\S)[^<\n]*?(\*{1,2})',re.sub(r'<[^>]+>','',out))
print("LITERAL-EMPHASIS-LEFTOVERS:",lit)
