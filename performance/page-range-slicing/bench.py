#!/usr/bin/env python3
"""Measure equal-length early and late pdftotext page slices on a synthetic tree."""

import argparse
import os
import statistics
import tempfile
import time


def build_pdf(path, page_count, fanout=64):
    """Write a balanced page tree whose leaf contents identify their page number."""
    objects = [None]  # object numbers are list indexes; object 1 is the catalog

    def alloc(body=None):
        objects.append(body)
        return len(objects) - 1

    catalog = alloc()
    font = alloc("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
    leaves = []
    for number in range(1, page_count + 1):
        page = alloc()
        text = "BT /F1 10 Tf 72 720 Td (page %d) Tj ET" % number
        contents = alloc("<< /Length %d >>\nstream\n%s\nendstream" % (len(text), text))
        leaves.append((page, contents, number))

    level = []
    for start in range(0, len(leaves), fanout):
        node = alloc()
        group = leaves[start : start + fanout]
        kids = " ".join("%d 0 R" % page for page, _, _ in group)
        objects[node] = "<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, len(group))
        for page, contents, _ in group:
            objects[page] = (
                "<< /Type /Page /Parent %d 0 R /MediaBox [0 0 612 792] "
                "/Resources << /Font << /F1 %d 0 R >> >> /Contents %d 0 R >>"
                % (node, font, contents)
            )
        level.append((node, len(group)))

    while len(level) > 1:
        parent_level = []
        for start in range(0, len(level), fanout):
            parent = alloc()
            group = level[start : start + fanout]
            kids = " ".join("%d 0 R" % child for child, _ in group)
            count = sum(count for _, count in group)
            objects[parent] = "<< /Type /Pages /Kids [%s] /Count %d >>" % (kids, count)
            for child, _ in group:
                objects[child] = objects[child].replace("/Type /Pages", "/Type /Pages /Parent %d 0 R" % parent, 1)
            parent_level.append((parent, count))
        level = parent_level

    root, _ = level[0]
    objects[catalog] = "<< /Type /Catalog /Pages %d 0 R >>" % root
    with open(path, "wb") as out:
        out.write(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
        offsets = []
        for number, body in enumerate(objects[1:], 1):
            offsets.append(out.tell())
            out.write(("%d 0 obj\n%s\nendobj\n" % (number, body)).encode("latin-1"))
        xref = out.tell()
        out.write(b"xref\n0 %d\n0000000000 65535 f \n" % len(objects))
        for offset in offsets:
            out.write(b"%010d 00000 n \n" % offset)
        out.write(b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objects), xref))


def measure(binary, pdf, first, last):
    started = time.monotonic()
    read_fd, write_fd = os.pipe()
    pid = os.fork()
    if pid == 0:
        os.close(read_fd)
        os.dup2(write_fd, 1)
        os.close(write_fd)
        env = os.environ.copy()
        bindir = os.path.dirname(binary)
        if os.path.exists(os.path.join(bindir, "libpoppler.so.165")):
            env["LD_LIBRARY_PATH"] = bindir + (":" + env["LD_LIBRARY_PATH"] if env.get("LD_LIBRARY_PATH") else "")
        os.execve(binary, [binary, "-q", "-f", str(first), "-l", str(last), pdf, "-"], env)
    os.close(write_fd)
    _, status, usage = os.wait4(pid, 0)
    with os.fdopen(read_fd, "rb") as output:
        text = output.read()
    if os.waitstatus_to_exitcode(status) != 0:
        raise RuntimeError("pdftotext failed for pages %d-%d" % (first, last))
    if text.count(b"\f") != last - first + 1 or ("page %d" % first).encode() not in text or ("page %d" % last).encode() not in text:
        raise RuntimeError("pdftotext returned the wrong pages for %d-%d" % (first, last))
    return time.monotonic() - started, usage.ru_utime, usage.ru_maxrss / 1024.0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", action="append", required=True, help="[LABEL=]PATH; repeat to compare builds")
    parser.add_argument("--pages", type=int, default=50000)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--fixture")
    args = parser.parse_args()
    if args.pages < 10:
        parser.error("--pages must be at least 10")
    builds = []
    for value in args.binary:
        label, path = value.split("=", 1) if "=" in value else (os.path.basename(value), value)
        builds.append((label, os.path.abspath(path)))
    fixture = args.fixture
    owned = None
    if fixture is None:
        owned = tempfile.TemporaryDirectory(prefix="poppler-page-range.")
        fixture = os.path.join(owned.name, "pages.pdf")
    if not os.path.exists(fixture):
        build_pdf(fixture, args.pages)
    slices = {"early": (1, 10), "late": (args.pages - 9, args.pages)}
    results = {(label, name): [] for label, _ in builds for name in slices}
    for run in range(args.runs):
        slice_order = ("early", "late") if run % 2 == 0 else ("late", "early")
        build_order = builds if run % 2 == 0 else list(reversed(builds))
        for name in slice_order:
            for label, binary in build_order:
                results[label, name].append(measure(binary, fixture, *slices[name]))
    print("fixture=%s bytes=%d pages=%d" % (fixture, os.path.getsize(fixture), args.pages))
    print("build       slice  wall_median_s user_median_s rss_median_MiB  user_runs_s  rss_runs_MiB")
    for label, _ in builds:
        for name in ("early", "late"):
            values = results[label, name]
            print(
                "%-11s %-5s %13.4f %13.4f %14.1f  %s  %s"
                % (
                    label,
                    name,
                    statistics.median(v[0] for v in values),
                    statistics.median(v[1] for v in values),
                    statistics.median(v[2] for v in values),
                    ",".join("%.4f" % v[1] for v in values),
                    ",".join("%.1f" % v[2] for v in values),
                )
            )


if __name__ == "__main__":
    main()
