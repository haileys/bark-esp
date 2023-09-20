#ifndef BARK_NATIVE_QUEUE_H
#define BARK_NATIVE_QUEUE_H

#include <stddef.h>

#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"

QueueHandle_t
rtos_queue_create(size_t queue_length, size_t item_size);

void
rtos_queue_delete(QueueHandle_t queue);

bool
rtos_queue_receive(QueueHandle_t queue, void* ptr, TickType_t wait);

bool
rtos_queue_send_to_back(QueueHandle_t queue, const void* ptr, TickType_t wait);

#endif
