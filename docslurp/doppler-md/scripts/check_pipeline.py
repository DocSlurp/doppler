#!/usr/bin/env python3
"""Check rule completeness across the real pdftotext -> doppler-md seam.

    python3 -I scripts/check_pipeline.py --pdftotext /path/to/pdftotext \
        --converter target/release/doppler-md

Build both programs first. The PDFs are generated locally; no external corpus is needed.
The converter's JSON page numbers are one-based, like its existing degradation report.
"""

import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET


REPO = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(REPO / "test" / "docslurp"))
import docslurp_testlib as t  # noqa: E402


def rules_pdf(counts):
    """Each page has body text and a requested number of eligible identical strokes."""
    page_ids = [4 + 2 * i for i in range(len(counts))]
    objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [%s] /Count %d >>"
        % (" ".join(f"{i} 0 R" for i in page_ids), len(counts)),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ]
    for page_id, count in zip(page_ids, counts):
        objects.append(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
            "/Resources << /Font << /F1 3 0 R >> >> /Contents %d 0 R >>"
            % (page_id + 1)
        )
        content = b"BT /F1 10 Tf 72 400 Td (Retained text) Tj ET 0 G 0.1 w "
        content += b"10 100 m 20 100 l S " * count
        objects.append(t.stream("", content, compress=True))
    return t.build_pdf(objects)


def convert(binary, xhtml, params):
    result = subprocess.run(
        [binary, "--report", "--params", json.dumps(params)],
        input=xhtml,
        capture_output=True,
        timeout=60,
        check=True,
    )
    return result.stdout.decode("utf-8"), json.loads(result.stderr)


def check_case(pdftotext, converter, directory, name, counts, kept, truncated):
    pdf = directory / f"{name}.pdf"
    pdf.write_bytes(rules_pdf(counts))
    result = subprocess.run(
        [pdftotext, "-bbox-layout", "-wordmeta", "-rules", str(pdf), "-"],
        capture_output=True,
        timeout=60,
        check=True,
    )
    pages = [e for e in ET.fromstring(result.stdout).iter() if e.tag.rsplit("}", 1)[-1] == "page"]
    actual_kept = [
        sum(e.tag.rsplit("}", 1)[-1] in ("rule", "box") for e in page)
        for page in pages
    ]
    actual_truncated = [i + 1 for i, page in enumerate(pages) if page.get("rulesTruncated") == "1"]
    assert actual_kept == kept, (name, "producer rule counts", actual_kept, kept)
    assert actual_truncated == truncated, (name, "producer completeness", actual_truncated, truncated)

    # Table inference is unrelated to propagation. Disabling it also keeps this integration
    # check small when a document deliberately reaches the capture caps.
    markdown, report = convert(converter, result.stdout, {"tables": False})
    assert report.get("rules_truncated_pages") == truncated, (
        name, "upstream truncation must survive conversion", report, truncated
    )
    assert report["degraded"] == [], (name, "truncation alone must not force fallback", report)
    assert report["pages"] == len(counts), (name, report)
    assert report["rules_skipped"] == 0, (name, report)
    converted_pages = markdown.split("\f")
    assert len(converted_pages) == len(counts), (name, len(converted_pages), len(counts))
    assert all("Retained text" in page for page in converted_pages), (name, "body text lost")

    # A layout decision remains separately observable, even on an upstream-truncated page.
    _, limited = convert(converter, result.stdout, {"tables": False, "max_page_rules": 19999})
    expected_degraded = [
        {"page": i + 1, "reason": "page_rules"} for i, count in enumerate(kept) if count > 19999
    ]
    assert limited["degraded"] == expected_degraded, (name, "layout limits", limited)
    assert limited["rules_truncated_pages"] == truncated, (name, "independent completeness", limited)
    print(f"PASS {name}: kept={kept}, truncated={truncated}, layout fallback separately reported")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pdftotext", required=True)
    parser.add_argument("--converter", required=True)
    args = parser.parse_args()
    pdftotext = str(Path(args.pdftotext).resolve())
    converter = str(Path(args.converter).resolve())
    cases = [
        ("truncated-at-page-cap", [20001, 1], [20000, 1], [1]),
        ("complete-at-page-cap", [20000], [20000], []),
        ("document-cap", [20000] * 10 + [1, 0], [20000] * 10 + [0, 0], [11]),
        # The existing document budget counts eligible attempts, including ones discarded
        # after a page fills. Keep that conservative accounting through the refactor.
        ("page-cap-still-charges-document", [200001, 1], [20000, 0], [1, 2]),
    ]
    with tempfile.TemporaryDirectory(prefix="doppler-pipeline.") as directory:
        for case in cases:
            check_case(pdftotext, converter, Path(directory), *case)
    print(f"all {len(cases)} producer-to-converter cases passed")


if __name__ == "__main__":
    main()
