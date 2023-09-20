#include "bark_native_bindings.h"

__attribute__((constructor)) static void bark_ctor() {
    printf("bark_ctor called!!\n");
}

SemaphoreHandle_t bark_create_recursive_mutex() {
    return xSemaphoreCreateRecursiveMutex();
}

void bark_lock_recursive_mutex(SemaphoreHandle_t sema) {
    while (!xSemaphoreTakeRecursive(sema, 1000)) ;
}

void bark_unlock_recursive_mutex(SemaphoreHandle_t sema) {
    xSemaphoreGiveRecursive(sema);
}

void bark_delete_recursive_mutex(SemaphoreHandle_t sema) {
    vSemaphoreDelete(sema);
}

SemaphoreHandle_t bark_create_mutex_static(StaticSemaphore_t* buffer) {
    return xSemaphoreCreateMutexStatic(buffer);
}

void bark_lock_mutex(SemaphoreHandle_t sema) {
    while (!xSemaphoreTake(sema, 1000)) ;
}

void bark_unlock_mutex(SemaphoreHandle_t sema) {
    xSemaphoreGive(sema);
}
