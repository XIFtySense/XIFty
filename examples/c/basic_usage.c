#include "xifty.h"

#include <stdio.h>
#include <string.h>

static void free_result(struct XiftyResult result) {
  xifty_free_buffer(result.output);
  xifty_free_buffer(result.error_message);
}

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stderr, "usage: %s <fixture-path>\n", argv[0]);
    return 1;
  }

  struct XiftyResult result = xifty_extract_json(argv[1], XIFTY_VIEW_MODE_NORMALIZED);
  if (result.status != XIFTY_STATUS_CODE_SUCCESS) {
    if (result.error_message.ptr != NULL) {
      fprintf(stderr, "xifty error: %.*s\n", (int)result.error_message.len,
              (const char *)result.error_message.ptr);
    } else {
      fprintf(stderr, "xifty error with empty message\n");
    }
    free_result(result);
    return 1;
  }

  printf("XIFty version: %s\n", xifty_version());
  printf("%.*s\n", (int)result.output.len, (const char *)result.output.ptr);

  free_result(result);
  return 0;
}
