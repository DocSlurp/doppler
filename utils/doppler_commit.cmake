# Writes OUT (doppler_commit.h) with DOPPLER_COMMIT, the short commit of SRC's HEAD ("unknown"
# outside a git checkout). The file is rewritten only when the value changes.
execute_process(
  COMMAND git -C "${SRC}" rev-parse --short=12 HEAD
  OUTPUT_VARIABLE commit
  OUTPUT_STRIP_TRAILING_WHITESPACE
  ERROR_QUIET
  RESULT_VARIABLE result)
if(NOT result EQUAL 0 OR NOT commit MATCHES "^[0-9a-f]+$")
  set(commit "unknown")
endif()
set(content "#define DOPPLER_COMMIT \"${commit}\"\n")
if(EXISTS "${OUT}")
  file(READ "${OUT}" old)
else()
  set(old "")
endif()
if(NOT old STREQUAL content)
  file(WRITE "${OUT}" "${content}")
endif()
