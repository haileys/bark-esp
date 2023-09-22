#include "bark_native/critical.h"

void
rtos_taskENTER_CRITICAL(const portMUX_TYPE* spinlock)
{
    taskENTER_CRITICAL(spinlock);
}

void
rtos_taskEXIT_CRITICAL(const portMUX_TYPE* spinlock)
{
    taskEXIT_CRITICAL(spinlock);
}
