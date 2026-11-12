# Page facts (doppler-page-facts/1): pdfpagefacts, pdftotext -page-facts-fd.
# The encrypted cases read the poppler-test checkout when TESTDATADIR is set.
if(TESTDATADIR)
  docslurp_add_test(page-facts test_page_facts.py --testdata ${TESTDATADIR})
else()
  docslurp_add_test(page-facts test_page_facts.py)
endif()
docslurp_add_test(page-facts-fuzz test_mutation_fuzz.py)
