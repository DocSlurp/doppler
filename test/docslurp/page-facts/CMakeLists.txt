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
