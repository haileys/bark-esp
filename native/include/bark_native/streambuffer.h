#ifndef BARK_NATIVE_STREAMBUFFER_H
#define BARK_NATIVE_STREAMBUFFER_H

#include <stddef.h>

#include "freertos/FreeRTOS.h"
#include "freertos/stream_buffer.h"

StreamBufferHandle_t
rtos_xStreamBufferCreate(size_t capacity, size_t trigger_level);

#endif
