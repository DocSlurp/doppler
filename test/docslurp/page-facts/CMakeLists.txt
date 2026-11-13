# Page facts (doppler-page-facts/1): pdfpagefacts, pdftotext -page-facts-fd.
# The encrypted cases read the poppler-test checkout when TESTDATADIR is set.
# docslurp-oc-state-probe: optional-content visibility follows group state changes (OCGs caches)
add_executable(docslurp-oc-state-probe oc_state_probe.cc)
target_link_libraries(docslurp-oc-state-probe poppler)
if(TESTDATADIR)
  docslurp_add_test(page-facts test_page_facts.py --oc-probe $<TARGET_FILE:docslurp-oc-state-probe> --testdata ${TESTDATADIR})
else()
  docslurp_add_test(page-facts test_page_facts.py --oc-probe $<TARGET_FILE:docslurp-oc-state-probe>)
endif()
docslurp_add_test(page-facts-fuzz test_mutation_fuzz.py)
# Hostile-input cost bounds (facts CPU against the pdftotext pass), pdftotext -timeout, and the
# confinement probe (what confine() must deny).
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
  add_executable(docslurp-confine-probe confine_probe.cc ${CMAKE_SOURCE_DIR}/utils/Confine.cc)
  target_include_directories(docslurp-confine-probe PRIVATE ${CMAKE_SOURCE_DIR}/utils)
  target_link_libraries(docslurp-confine-probe poppler)
  if(TARGET Fontconfig::Fontconfig)
    target_link_libraries(docslurp-confine-probe Fontconfig::Fontconfig)
  endif()
  docslurp_add_test(page-facts-cost test_cost_bounds.py --probe $<TARGET_FILE:docslurp-confine-probe>)
else()
  docslurp_add_test(page-facts-cost test_cost_bounds.py)
endif()
# Issue 43: per-grid thin-rule counts and words centred inside images against DocSlurp's
# layout walker (docslurp-oss worker/scripts/pdf_layout_facts.py and its public fixtures).
# Set -DDOCSLURP_OSS_DIR=<checkout> (or the DOCSLURP_OSS_DIR environment variable at test
# time); without it the test is skipped.
if(DOCSLURP_OSS_DIR)
  docslurp_add_test(page-facts-walker-parity test_walker_parity.py --oss ${DOCSLURP_OSS_DIR})
else()
  docslurp_add_test(page-facts-walker-parity test_walker_parity.py)
endif()
set_tests_properties(docslurp-page-facts-walker-parity PROPERTIES SKIP_RETURN_CODE 77)
# Issue 45: the /Prev chain read at open and read-only lookups, under the council's hostile
# cross-reference tables (generators in xref_hostile/)
docslurp_add_test(page-facts-xref-chain test_xref_chain.py)
