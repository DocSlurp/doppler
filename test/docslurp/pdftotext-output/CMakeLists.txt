docslurp_add_test(pdftotext-output-page-size test_page_size.py)
docslurp_add_test(pdftotext-output-escaping test_escaping.py)
docslurp_add_test(pdftotext-output-dehyphenate-paragraphs test_dehyphenate_paragraphs.py)

# Needs the glib typelib and PyGObject; the script skips itself (77) without them.
if(TARGET poppler-glib)
  docslurp_add_test(pdftotext-output-table-cells-glib test_table_cells_glib.py)
  set_tests_properties(docslurp-pdftotext-output-table-cells-glib PROPERTIES SKIP_RETURN_CODE 77)
endif()
docslurp_add_test(pdftotext-output-text-encoding-lookup test_text_encoding_lookup.py)
set_tests_properties(docslurp-pdftotext-output-text-encoding-lookup PROPERTIES SKIP_RETURN_CODE 77 TIMEOUT 300)
docslurp_add_test(pdftotext-output-pdftohtml-wordbreak test_pdftohtml_wordbreak.py)
docslurp_add_test(pdftotext-output-wordmeta test_wordmeta.py)
docslurp_add_test(pdftotext-output-bbox-rtl-logical test_bbox_rtl_logical.py)
