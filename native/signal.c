#include "bark_native_bindings.h"

static StaticSemaphore_t buffer;
static SemaphoreHandle_t handle;

void bark_sync_signal_init() {
    handle = xSemaphoreCreateMutexStatic(&buffer);
}

void bark_sync_signal_lock() {
    while (!xSemaphoreTake(handle, 1000)) ;
}

void bark_sync_signal_unlock() {
    xSemaphoreGive(handle);
}
