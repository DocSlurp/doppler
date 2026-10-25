# Optional evaluator dependency: keep normal PDF regression tests dependency-free.
set(DOCSLURP_TABLE_SCORER_PYTHON "${Python3_EXECUTABLE}" CACHE FILEPATH "Python with table_scoring/requirements.txt installed")
execute_process(COMMAND "${DOCSLURP_TABLE_SCORER_PYTHON}" -c "import markdown_it"
  RESULT_VARIABLE _table_scorer_dependencies OUTPUT_QUIET ERROR_QUIET)
if(_table_scorer_dependencies EQUAL 0)
  add_test(NAME docslurp-table-scorer
    COMMAND "${DOCSLURP_TABLE_SCORER_PYTHON}" -I "${CMAKE_CURRENT_SOURCE_DIR}/test_scorer.py")
else()
  message(STATUS "Skipping optional table scorer tests: install test/docslurp/table_scoring/requirements.txt and set DOCSLURP_TABLE_SCORER_PYTHON")
endif()
