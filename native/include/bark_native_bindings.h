#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/event_groups.h"
#include "freertos/semphr.h"
#include "freertos/portmacro.h"
#include "freertos/stream_buffer.h"

#include "lwip/err.h"
#include "lwip/igmp.h"
#include "lwip/ip_addr.h"
#include "lwip/pbuf.h"
#include "lwip/udp.h"

#include "esp_netif.h"
#include "esp_netif_net_stack.h"

#include "driver/dac_continuous.h"

#include "bark_native/queue.h"

const TickType_t freertos_wait_forever = portMAX_DELAY;

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
