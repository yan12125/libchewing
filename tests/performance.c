/**
 * performance.c
 *
 * Copyright (c) 2013
 *      libchewing Core Team.
 *
 * See the file "COPYING" for information on usage and redistribution
 * of this file.
 */
#include <stdio.h>
#include <stdlib.h>

#include "chewing.h"
#include "testhelper.h"

int main()
{
    ChewingContext *ctx;
    int ch;

    /* Initialize libchewing */
    putenv("CHEWING_PATH=" CHEWING_DATA_PREFIX);
    /* for the sake of testing, we should not change existing hash data */
    putenv_test_hash_dir();

    ctx = chewing_new();

    while ((ch = getchar()) != EOF) {
        chewing_handle_Default(ctx, ch);
    }

    chewing_delete(ctx);
    return 0;
}
