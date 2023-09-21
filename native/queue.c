#include "bark_native/queue.h"

QueueHandle_t
rtos_queue_create(size_t queue_length, size_t item_size)
{
    return xQueueCreate(queue_length, item_size);
}

void
rtos_queue_delete(QueueHandle_t queue)
{
    vQueueDelete(queue);
}

bool
rtos_queue_receive(QueueHandle_t queue, void* ptr, TickType_t wait)
{
    return xQueueReceive(queue, ptr, wait);
}

bool
rtos_queue_send_to_back(QueueHandle_t queue, const void* ptr, TickType_t wait)
{
    return xQueueSendToBack(queue, ptr, wait);
}

bool
rtos_queue_send_to_back_from_isr(QueueHandle_t queue, const void* ptr, bool* need_wake)
{
    return xQueueSendToBackFromISR(queue, ptr, need_wake);
}
