#include "bark_native/streambuffer.h"

StreamBufferHandle_t
rtos_xStreamBufferCreate(size_t capacity, size_t trigger_level)
{
    return xStreamBufferCreate(capacity, trigger_level);
}
