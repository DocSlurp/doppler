add_executable(docslurp-parenttree-probe parenttree_probe.cc)
target_link_libraries(docslurp-parenttree-probe poppler)

docslurp_add_test(tagged-mc-balance test_mc_balance.py)
docslurp_add_test(tagged-actualtext test_actualtext.py)
docslurp_add_test(tagged-struct-text-pages test_struct_text_pages.py)
docslurp_add_test(tagged-unknown-types test_unknown_types.py --probe $<TARGET_FILE:docslurp-parenttree-probe>)
