#include <math.h>

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

#include "bark_native/critical.h"
#include "bark_native/queue.h"
#include "bark_native/streambuffer.h"

const TickType_t freertos_wait_forever = portMAX_DELAY;
