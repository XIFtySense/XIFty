#include "xifty.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int has_content(const struct XiftyBuffer buffer, const char *needle) {
  if (buffer.ptr == NULL || buffer.len == 0) {
    return 0;
  }

  const char *bytes = (const char *)buffer.ptr;
  return strstr(bytes, needle) != NULL;
}

static void free_result(struct XiftyResult result) {
  xifty_free_buffer(result.output);
  xifty_free_buffer(result.error_message);
}

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stderr, "expected fixture path\n");
    return 1;
  }

  struct XiftyResult probe = xifty_probe_json(argv[1]);
  if (probe.status != XIFTY_STATUS_CODE_SUCCESS || !has_content(probe.output, "\"detected_format\": \"jpeg\"")) {
    fprintf(stderr, "probe failed\n");
    free_result(probe);
    return 1;
  }
  free_result(probe);

  struct XiftyResult extract = xifty_extract_json(argv[1], XIFTY_VIEW_MODE_NORMALIZED);
  if (extract.status != XIFTY_STATUS_CODE_SUCCESS || !has_content(extract.output, "\"normalized\"")) {
    fprintf(stderr, "extract failed\n");
    free_result(extract);
    return 1;
  }
  free_result(extract);

  struct XiftyResult missing = xifty_probe_json("/Users/k/Projects/XIFty/fixtures/minimal/no-such-file.jpg");
  if (missing.status != XIFTY_STATUS_CODE_IO_ERROR) {
    fprintf(stderr, "expected io error for missing file\n");
    free_result(missing);
    return 1;
  }
  free_result(missing);

  if (xifty_version() == NULL || strlen(xifty_version()) == 0) {
    fprintf(stderr, "unexpected version string\n");
    return 1;
  }

  return 0;
}
