use esp_idf_sys::{self as sys, EspError};

use crate::system;
use crate::sync::signal::Signal;
use super::event::{self, EventHandler, AttachHandlerError};

const SSID: &str = env!("BARK_WIFI_SSID");
const PASSWORD: &str = env!("BARK_WIFI_PASS");

const STATIC_RX_BUF_COUNT: i32 = 10;
const DYNAMIC_RX_BUF_COUNT: i32 = 10;

const STATIC_TX_BUF_COUNT: i32 = 10;
const DYNAMIC_TX_BUF_COUNT: i32 = 10;

// disable AMPDU, not suitable for realtime networking apparently
const AMPDU_ENABLE: i32 = 0;

#[derive(Clone, Copy, PartialEq)]
enum WifiState {
    Uninit,
    Started,
    Stopped,
}

static WIFI_STATE: Signal<WifiState> = Signal::new(WifiState::Uninit);

#[embassy_executor::task]
async fn wifi_task() {
    log::info!("Starting wifi task");
    let mut state = WIFI_STATE.watch().unwrap();

    loop {
        match state.wait().await {
            WifiState::Uninit => {
                log::info!("Starting wifi...");
                unsafe { start_wifi(); }
            }
            WifiState::Started => {
                log::info!("station started");
            }
            WifiState::Stopped => {
                log::info!("station started");
            }
        }
    }
}

pub fn init() {
    system::rt::spawner().must_spawn(wifi_task());
}

unsafe fn start_wifi() {
    let config = sys::wifi_init_config_t {
        osi_funcs: &sys::g_wifi_osi_funcs as *const _ as *mut _,
        wpa_crypto_funcs: sys::g_wifi_default_wpa_crypto_funcs,
        static_rx_buf_num: STATIC_RX_BUF_COUNT,
        dynamic_rx_buf_num: DYNAMIC_RX_BUF_COUNT,
        tx_buf_type: sys::CONFIG_ESP_WIFI_TX_BUFFER_TYPE as i32,
        static_tx_buf_num: STATIC_TX_BUF_COUNT,
        dynamic_tx_buf_num: DYNAMIC_TX_BUF_COUNT,
        cache_tx_buf_num: 0,
        csi_enable: 1,
        ampdu_rx_enable: AMPDU_ENABLE,
        ampdu_tx_enable: AMPDU_ENABLE,
        amsdu_tx_enable: AMPDU_ENABLE,
        nvs_enable: 0,
        nano_enable: sys::WIFI_NANO_FORMAT_ENABLED as i32,
        rx_ba_win: sys::WIFI_DEFAULT_RX_BA_WIN as i32,
        wifi_task_core_id: 0, // main core
        beacon_max_len: sys::WIFI_SOFTAP_BEACON_MAX_LEN as i32,
        mgmt_sbuf_num: sys::WIFI_MGMT_SBUF_NUM as i32,
        feature_caps: sys::g_wifi_feature_caps,
        sta_disconnected_pm: sys::WIFI_STA_DISCONNECTED_PM_ENABLED != 0,
        espnow_max_encrypt_num: sys::CONFIG_ESP_WIFI_ESPNOW_MAX_ENCRYPT_NUM as i32,
        magic: sys::WIFI_INIT_CONFIG_MAGIC as i32,
    };

    if let Err(e) = sys::esp!(sys::esp_wifi_init(&config)) {
        log::error!("esp_wifi_init failed: {e:?}");
        return;
    }

    if let Err(e) = sys::esp!(sys::esp_wifi_start()) {
        log::error!("esp_wifi_start failed: {e:?}");
        return;
    }

    match handle_events() {
        Ok(handler) => handler.leak(),
        Err(e) => {
            log::error!("failed to register event handler: {e:?}");
            return;
        }
    }

    if let Err(e) = configure() {
        log::error!("failed to configure wifi: {e:?}");
    }

    if let Err(e) = sys::esp!(sys::esp_wifi_start()) {
        log::error!("esp_wifi_start failed: {e:?}");
    }

    if let Err(e) = sys::esp!(sys::esp_wifi_connect()) {
        log::error!("esp_wifi_connect failed: {e:?}");
    }
}

unsafe fn configure() -> Result<(), EspError> {
    log::info!("configuring wifi with ssid: {SSID:?}");

    let config = sys::wifi_sta_config_t {
        ssid: fixed(SSID),
        password: fixed(PASSWORD),
        scan_method: sys::wifi_scan_method_t_WIFI_ALL_CHANNEL_SCAN,
        bssid_set: false,
        bssid: [0u8; 6],
        channel: 0,
        listen_interval: 3,
        sort_method: sys::wifi_sort_method_t_WIFI_CONNECT_AP_BY_SIGNAL,
        threshold: sys::wifi_scan_threshold_t {
            authmode: sys::wifi_auth_mode_t_WIFI_AUTH_WPA2_PSK,
            rssi: -99,
        },
        pmf_cfg: sys::wifi_pmf_config_t {
            capable: true,
            required: false,
        },
        sae_pwe_h2e: 3,
        failure_retry_cnt: 1,

        // zero stuff out we don't care about:
        _bitfield_1: Default::default(),
        _bitfield_2: Default::default(),
        _bitfield_align_1: Default::default(),
        _bitfield_align_2: Default::default(),
        sae_pk_mode: Default::default(),
        sae_h2e_identifier: Default::default(),
    };

    let mut config = sys::wifi_config_t { sta: config };

    sys::esp!(sys::esp_wifi_set_mode(
        sys::wifi_mode_t_WIFI_MODE_STA,
    ))?;

    sys::esp!(sys::esp_wifi_set_config(
        sys::wifi_interface_t_WIFI_IF_STA,
        &mut config,
    ))?;

    Ok(())
}

fn fixed<const N: usize>(s: &str) -> [u8; N] {
    let mut buff = [0; N];
    let len = core::cmp::min(N, s.len());
    buff[0..len].copy_from_slice(&s.as_bytes()[0..len]);
    buff
}

fn handle_events() -> Result<EventHandler, AttachHandlerError> {
    let wifi_event = unsafe { sys::WIFI_EVENT };

    event::attach(wifi_event, |msg, _data| {
        match msg as u32 {
            sys::wifi_event_t_WIFI_EVENT_STA_START => {
                WIFI_STATE.set(WifiState::Started);
            }
            sys::wifi_event_t_WIFI_EVENT_STA_DISCONNECTED => {
                WIFI_STATE.set(WifiState::Stopped);
            }
            _ => {}
        }
    })
}
