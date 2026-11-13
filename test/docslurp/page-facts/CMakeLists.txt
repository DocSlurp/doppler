# Page facts (doppler-page-facts/1): pdfpagefacts, pdftotext -page-facts-fd.
# The encrypted cases read the poppler-test checkout when TESTDATADIR is set.
if(TESTDATADIR)
  docslurp_add_test(page-facts test_page_facts.py --testdata ${TESTDATADIR})
else()
  docslurp_add_test(page-facts test_page_facts.py)
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
