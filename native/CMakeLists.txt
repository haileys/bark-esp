idf_component_register(SRCS "native.c" "signal.c"
                       INCLUDE_DIRS "include"
                       REQUIRES freertos lwip esp_netif)
