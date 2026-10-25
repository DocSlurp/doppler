# DocSlurp regression tests.
#
# Each subdirectory with a CMakeLists.txt registers its own ctest cases, so
# independent branches can add tests without editing a shared file.
# Tests are Python scripts that build small PDFs with docslurp_testlib.py and
# run the freshly built utils against them.

find_package(Python3 COMPONENTS Interpreter)
if(NOT Python3_Interpreter_FOUND)
  message(STATUS "Python3 not found: skipping DocSlurp regression tests")
  return()
endif()

set(DOCSLURP_TESTLIB_DIR ${CMAKE_CURRENT_SOURCE_DIR})

# docslurp_add_test(<name> <script> [args...])
# Runs <script> with --bindir pointing at the built utils and PYTHONPATH set so
# it can import docslurp_testlib.
function(docslurp_add_test name script)
  add_test(
    NAME docslurp-${name}
    COMMAND ${Python3_EXECUTABLE} -I ${CMAKE_CURRENT_SOURCE_DIR}/${script} --bindir $<TARGET_FILE_DIR:pdftotext> ${ARGN}
  )
  set_tests_properties(docslurp-${name} PROPERTIES ENVIRONMENT "DOCSLURP_TESTLIB_DIR=${DOCSLURP_TESTLIB_DIR}")
endfunction()

file(GLOB _docslurp_children LIST_DIRECTORIES true RELATIVE ${CMAKE_CURRENT_SOURCE_DIR} ${CMAKE_CURRENT_SOURCE_DIR}/*)
list(SORT _docslurp_children)
foreach(_child ${_docslurp_children})
  if(IS_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}/${_child} AND EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/${_child}/CMakeLists.txt)
    add_subdirectory(${_child})
  endif()
endforeach()
