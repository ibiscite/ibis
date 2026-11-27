#ifndef IBIS_NATIVE_H
#define IBIS_NATIVE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef struct {
    const uint8_t *data;
    size_t len;
} IbisUtf8;

typedef enum {
    IbisPasted = 0,
    IbisCopied = 1,
    IbisPermissionRequired = 2,
} IbisDelivery;

// Main thread. Capture the external target before making the panel key.
int32_t ibis_native_command_key(uint16_t preferred, uint8_t lower_ascii);
void ibis_native_capture(intptr_t window_number);
void ibis_native_cancel(void);
uint32_t ibis_native_begin(void);
// Borrow UTF-8 for this call only. NULL with length 0 means absent.
// A return value of 0 means no delivery receipt.
uint32_t ibis_native_write(uint32_t request, IbisUtf8 text, IbisUtf8 html, IbisUtf8 rtf, bool paste);
bool ibis_native_paste_expected(bool enabled);
void ibis_native_request_paste_access(void);

// Worker thread, after the panel has been hidden. Consumes the write receipt.
IbisDelivery ibis_native_paste(uint32_t token);

#endif
