#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/event_groups.h"
#include "freertos/semphr.h"

SemaphoreHandle_t bark_create_recursive_mutex();
void bark_lock_recursive_mutex(SemaphoreHandle_t sema);
void bark_unlock_recursive_mutex(SemaphoreHandle_t sema);
void bark_delete_recursive_mutex(SemaphoreHandle_t sema);

SemaphoreHandle_t bark_create_mutex_static(StaticSemaphore_t* buffer);
void bark_lock_mutex(SemaphoreHandle_t sema);
void bark_unlock_mutex(SemaphoreHandle_t sema);

void bark_sync_signal_init();
void bark_sync_signal_lock();
void bark_sync_signal_unlock();