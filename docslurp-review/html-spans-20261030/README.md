# Current final result

See [final-validation](final-validation/README.md): 254/254 baseline-equal HTML and GFM, zero HTML degradation, 33-page checks passed; integrity blocked by missing authored provenance. This supersedes all candidate quality claims below, retained as historical evidence.

# HTML span validation — 2026-10-30

Candidate implementation: 78db5478757ab3d8437027c6ae0139b295b3330a.
Baseline private/master: b7684b572e4ca3f56361e95af748cc0d6b985988.
Tests add public conversion and real generated-PDF CLI coverage; no production
logic changed. Public coverage includes colspan, rowspan, combined spans, empty
geometric cells, hostile HTML text, RTL ordering, ambiguous and truncated rules,
external-header fallback, and page isolation. The initial 100-point row-height
fixture fell outside existing discovery thresholds; 20-point rows qualify.
External detected titles deliberately disable spans and retain legacy headers.

`cargo-test.log`: full Rust suite, 70 passing tests; `build.log`: release build.
`vm-tests.log`: four existing pure Python VM chunk tests, core unchanged.
The PDF tests ran with PDFTOTEXT pointing to this checkout's build/utils/pdftotext.

## Reproduction

Set PATH=$HOME/.cargo/bin:$PATH and TMPDIR, TMP, TEMP to /home/dan/tmp/agents.
Build private/master with git archive into an mktemp directory there and build its
Rust release converter. Put that directory in baseline-path.txt. No worktree is
needed. Build the candidate release converter in this checkout.

Run `python3 docslurp-review/html-spans-20261030/synthetic.py`.
It writes the authored source topology and expected HTML before generating the
33-page PDF. It uses compact-layout, wordmeta, rules, UTF-8/unix, 32-page ranges,
scaled furniture_min_share, and form-feed merge. Each arm retains all six labels
exactly once per page and each page marker in its page slot. Candidate preserves
all 33 combined spans. `page33.png` is the representative final-chunk page;
`synthetic-candidate-html.md` is its output, including all pages.

Five interleaved end-to-end parser+converter runs per arm measure child user+sys
CPU. Median seconds: baseline GFM 0.037786, baseline HTML 0.037504, candidate HTML
0.037942. These tiny timings are screening evidence, not a capacity claim.
Host load average was 5.23/2.48/1.46 with concurrent wild-v2 scoring.
Raw timings are in cpu.json.

`screen.py` runs all 254 authored-reference wild-v2 cases through private/master
GFM/HTML and candidate HTML, with the v3.6 adapter in judge36.py. Run it with
~/.cache/docslurp/raster-eval-venv/bin/python -I. eval36.json and screen.log retain
component scores, including hallucination and substitution separately. HTML's
0.01/table encoding penalty must not be interpreted as missing content.

## Limits

The guest script reads /opt/doppler/defaults.json; that artifact was not found in
either checkout. These runs therefore use converter defaults, with the synthetic
chunk run explicitly scaling the default furniture_min_share. They are not a
verified deployed-settings comparison. `chunk_screen.py` separately checked the VM chunk protocol on all 254 wild-v2
inputs: all 762 arm outputs were byte-identical to the scored whole-document
outputs. See chunk-identity.json and outputs/. The 33-page synthetic case
directly exercises the chunk transition.
No VM deployment was tested. No customer bytes, remote jobs or paid calls were
used. All PDFs used here are generated synthetic evidence. No quality promotion
is warranted from these runs alone.

## Completed v3.6 results

All 254 cases scored in 423 seconds. Aggregate component means:

| Arm | Quality | Content F1 | Table partial | Tolerant table | Hallucination | Substitution |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Baseline GFM | 0.7669 | 0.9217 | 0.8097 | 0.9020 | 0.0729 | 0.0458 |
| Baseline HTML | 0.7669 | 0.9217 | 0.8097 | 0.8891 | 0.0729 | 0.0458 |
| Candidate HTML | 0.7696 | 0.9222 | 0.8239 | 0.8923 | 0.0729 | 0.0458 |

Separate negative gates: hallucination and substitution are exactly unchanged
in all 254 cases, not just unchanged rounded means. gates.json records this.
Candidate vs baseline HTML improves content on 64 cases, reduces it on 18;
tolerant-table scores improve on 91 and reduce on 23 (226 scored cases).
120 HTML outputs differ. The table encoding penalty is separate from content;
compare candidate HTML with baseline HTML to avoid confusing encoding with loss.
Worst content-score deltas: quarterly-ja-03 L8 mixed −0.010987,
bank-ar-08 L0 tagged −0.006395, quarterly-de-02 L0 tagged −0.004352.
These regressions remain for parent review; aggregate gains do not erase them.
The run supplies local comparative evidence, not an unconditional promotion gate.
Full per-case scores and all three Markdown arms are retained.

No implementation defect was established by these tests. Existing candidate
selection limits and external-header behavior are explicitly covered/documented.
