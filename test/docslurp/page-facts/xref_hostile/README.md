# Hostile cross-reference generators (issue 45)

Written by the issue 45 security council to break the `/Prev` chain read at open and the read-only
(`XRef::RepairGuard`) lookups. Each file's header describes the defect it was written to expose,
as it stood in the round that found it. `../test_xref_chain.py` runs them all and checks that
none of those defects remains: bounds, the same XHTML with and without page facts, the same
facts from `pdfpagefacts` and `pdftotext -page-facts-fd`, determinism, and per-case expectations.
