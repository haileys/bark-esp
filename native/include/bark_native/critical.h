#ifndef BARK_NATIVE_CRITICAL_H
#define BARK_NATIVE_CRITICAL_H

#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

void
rtos_taskENTER_CRITICAL(const portMUX_TYPE* spinlock);

void
rtos_taskEXIT_CRITICAL(const portMUX_TYPE* spinlock);

#endif
