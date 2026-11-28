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
// A canonical site name in static UTF-8 storage, or NULL.
IbisUtf8 ibis_native_capture(intptr_t window_number, int32_t expected_pid);
// Main thread, once after a capture: the short selection in the app in front,
// copied into out as UTF-8. Returns its length, or 0 when none or too long.
size_t ibis_native_take_selection(uint8_t *out, size_t capacity);
void ibis_native_cancel(void);
uint32_t ibis_native_begin(void);
// Borrow UTF-8 for this call only. NULL with length 0 means absent.
// A return value of 0 means no delivery receipt.
uint32_t ibis_native_write(uint32_t request, IbisUtf8 text, IbisUtf8 html, IbisUtf8 rtf, bool paste);
// The captured target to offer a paste to, including a target waiting for permission.
// Zero means copy. The offer does not grant permission to post a key event.
int32_t ibis_native_paste_destination(bool enabled);
// Main thread, passive. Both Accessibility and PostEvent must be granted.
bool ibis_native_paste_granted(void);
void ibis_native_request_paste_access(void);

// Worker thread, after the panel has been hidden. Consumes the write receipt.
IbisDelivery ibis_native_paste(uint32_t token);

#endif
