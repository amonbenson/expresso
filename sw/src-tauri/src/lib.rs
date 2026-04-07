use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use expresso::settings::{Settings, SettingsPatch};
use expresso::sysex::{
    codec_7bit, MAX_SETTINGS_BYTES, SYSEX_CMD_SETTINGS_GET, SYSEX_CMD_SETTINGS_PATCH,
    SYSEX_CMD_STATUS, SYSEX_MAGIC, SYSEX_MFID, SYSEX_RESPONSE_BIT,
};
use midir::{MidiInput, MidiInputConnection, MidiOutput, MidiOutputConnection};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::{sync::mpsc, time::interval};

const DEVICE_NAME: &str = "Expresso";
const POLL_INTERVAL_MS: u64 = 500;
const REQUEST_TIMEOUT_MS: u64 = 1000;

// ---------------------------------------------------------------------------
// Shared app state (accessible from MIDI thread and Tauri commands)
// ---------------------------------------------------------------------------

struct SharedState {
    connected: bool,
    settings: Option<Settings>,
}

type AppState = Arc<Mutex<SharedState>>;

#[derive(Clone, Serialize)]
struct InitialState {
    connected: bool,
    settings: Option<Settings>,
}

// ---------------------------------------------------------------------------
// SysEx helpers
// ---------------------------------------------------------------------------

fn build_settings_get() -> Vec<u8> {
    let mut msg = Vec::with_capacity(8);
    msg.push(0xF0);
    msg.push(SYSEX_MFID);
    msg.extend_from_slice(&SYSEX_MAGIC);
    msg.push(SYSEX_CMD_SETTINGS_GET);
    msg.push(0xF7);
    msg
}

fn build_settings_patch(patch: &SettingsPatch) -> Option<Vec<u8>> {
    eprintln!("[midi] Building patch: {patch:?}");
    let mut postcard_buf = [0u8; MAX_SETTINGS_BYTES];
    let serialized = postcard::to_slice(patch, &mut postcard_buf).ok()?;
    let serialized_len = serialized.len();

    let mut encoded = vec![0u8; (serialized_len / 7 + 1) * 8];
    let encoded_len = codec_7bit::encode(&postcard_buf[..serialized_len], &mut encoded);

    let mut msg = Vec::with_capacity(7 + encoded_len + 1);
    msg.push(0xF0);
    msg.push(SYSEX_MFID);
    msg.extend_from_slice(&SYSEX_MAGIC);
    msg.push(SYSEX_CMD_SETTINGS_PATCH);
    msg.extend_from_slice(&encoded[..encoded_len]);
    msg.push(0xF7);
    eprintln!("[midi] Patch SysEx: {} bytes: {:02X?}", msg.len(), &msg);
    Some(msg)
}

fn is_our_sysex(data: &[u8]) -> bool {
    data.len() >= 8
        && data[0] == 0xF0
        && data[1] == SYSEX_MFID
        && data[2..6] == SYSEX_MAGIC
        && *data.last().unwrap() == 0xF7
}

fn decode_settings_payload(data: &[u8]) -> Option<Settings> {
    let payload = &data[7..data.len() - 1];
    let mut decoded = [0u8; MAX_SETTINGS_BYTES];
    let decoded_len = codec_7bit::decode(payload, &mut decoded)?;
    eprintln!("[midi] Settings decoded: {decoded_len} bytes");
    let result = postcard::from_bytes::<Settings>(&decoded[..decoded_len]);
    match &result {
        Ok(s) => eprintln!("[midi] Settings parsed OK: {s:?}"),
        Err(e) => eprintln!("[midi] Settings parse error: {e}"),
    }
    result.ok()
}

// ---------------------------------------------------------------------------
// Tauri events
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceConnectedPayload {
    name: String,
}

#[derive(Clone, Serialize)]
struct DeviceDisconnectedPayload {}

// ---------------------------------------------------------------------------
// MIDI state
// ---------------------------------------------------------------------------

struct MidiState {
    input_conn: Option<MidiInputConnection<()>>,
    output_conn: Option<MidiOutputConnection>,
    connected_port_name: Option<String>,
}

impl MidiState {
    fn new() -> Self {
        Self {
            input_conn: None,
            output_conn: None,
            connected_port_name: None,
        }
    }

    fn is_connected(&self) -> bool {
        self.connected_port_name.is_some()
    }
}

// ---------------------------------------------------------------------------
// MIDI background thread
// ---------------------------------------------------------------------------

fn start_midi_thread(app: AppHandle, patch_rx: mpsc::Receiver<SettingsPatch>, app_state: AppState) {
    thread::Builder::new()
        .name("midi-manager".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(midi_loop(app, patch_rx, app_state));
        })
        .expect("spawn midi thread");
}

async fn midi_loop(
    app: AppHandle,
    mut patch_rx: mpsc::Receiver<SettingsPatch>,
    app_state: AppState,
) {
    let state = Arc::new(Mutex::new(MidiState::new()));
    let (sysex_tx, mut sysex_rx) = mpsc::channel::<Vec<u8>>(8);
    let mut ticker = interval(Duration::from_millis(POLL_INTERVAL_MS));

    // The response command byte we are currently waiting for, or None.
    // Only one request may be in flight at a time.
    let mut pending_cmd: Option<u8> = None;

    // Timeout future — armed when pending_cmd is Some, disarmed otherwise.
    // The guard `if pending_cmd.is_some()` in select! prevents it from firing while disarmed.
    let timeout = tokio::time::sleep(Duration::from_secs(3600));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let just_connected = poll_devices(&app, &state, sysex_tx.clone(), &app_state);
                if just_connected && pending_cmd.is_none() {
                    let msg = build_settings_get();
                    let mut st = state.lock().unwrap();
                    if let Some(out) = st.output_conn.as_mut() {
                        eprintln!("[midi] Sending SETTINGS_GET: {:02X?}", &msg);
                        if out.send(&msg).is_ok() {
                            pending_cmd = Some(SYSEX_CMD_SETTINGS_GET | SYSEX_RESPONSE_BIT);
                            timeout.as_mut().reset(
                                tokio::time::Instant::now()
                                    + Duration::from_millis(REQUEST_TIMEOUT_MS),
                            );
                            eprintln!("[midi] Waiting for response 0x{:02X}", pending_cmd.unwrap());
                        }
                    }
                }
            }

            Some(raw) = sysex_rx.recv() => {
                if !is_our_sysex(&raw) {
                    // Not our SysEx (e.g. CC messages forwarded from input — ignore)
                }
                else if let Some(expected) = pending_cmd {
                    let cmd = raw[6];
                    if cmd == expected {
                        eprintln!("[midi] Received expected response 0x{cmd:02X}");
                        pending_cmd = None;
                        // Disarm timeout
                        timeout.as_mut().reset(
                            tokio::time::Instant::now() + Duration::from_secs(3600),
                        );
                        // Handle response payload
                        let request_cmd = expected & !SYSEX_RESPONSE_BIT;
                        if request_cmd == SYSEX_CMD_SETTINGS_GET {
                            if let Some(settings) = decode_settings_payload(&raw) {
                                app_state.lock().unwrap().settings = Some(settings);
                                eprintln!("[midi] Emitting settings-loaded to frontend");
                                app.emit("settings-loaded", &settings).ok();
                            }
                        }
                        // PATCH ack has no payload to process
                    } else {
                        eprintln!(
                            "[midi] Ignoring SysEx cmd=0x{cmd:02X}, still waiting for 0x{expected:02X}"
                        );
                    }
                } else if raw[6] != (SYSEX_CMD_STATUS | SYSEX_RESPONSE_BIT) {
                    eprintln!("[midi] Ignoring unsolicited SysEx cmd=0x{:02X}", raw[6]);
                }
            }

            _ = &mut timeout, if pending_cmd.is_some() => {
                eprintln!(
                    "[midi] Request timed out waiting for 0x{:02X}",
                    pending_cmd.unwrap()
                );
                pending_cmd = None;
            }

            // Only accept new patches when no request is in flight
            Some(patch) = patch_rx.recv(), if pending_cmd.is_none() => {
                eprintln!("[midi] Sending patch: {patch:?}");
                let mut st = state.lock().unwrap();
                if st.output_conn.is_none() {
                    eprintln!("[midi] No device connected — patch dropped");
                } else if let Some(msg) = build_settings_patch(&patch) {
                    let result = st.output_conn.as_mut().unwrap().send(&msg);
                    eprintln!("[midi] SysEx send result: {result:?}");
                    if result.is_ok() {
                        pending_cmd = Some(SYSEX_CMD_SETTINGS_PATCH | SYSEX_RESPONSE_BIT);
                        timeout.as_mut().reset(
                            tokio::time::Instant::now()
                                + Duration::from_millis(REQUEST_TIMEOUT_MS),
                        );
                        eprintln!("[midi] Waiting for ack 0x{:02X}", pending_cmd.unwrap());
                    }
                }
            }
        }
    }
}

// Returns true if a new device just connected this poll cycle.
fn poll_devices(
    app: &AppHandle,
    state: &Arc<Mutex<MidiState>>,
    sysex_tx: mpsc::Sender<Vec<u8>>,
    app_state: &AppState,
) -> bool {
    let Ok(probe) = MidiInput::new("expresso-probe") else {
        return false;
    };
    let port_names: Vec<String> = probe
        .ports()
        .iter()
        .filter_map(|p| probe.port_name(p).ok())
        .collect();

    let mut st = state.lock().unwrap();

    // Detect disconnection
    if st.is_connected() {
        let still_present = port_names
            .iter()
            .any(|n| Some(n) == st.connected_port_name.as_ref());

        if !still_present {
            st.input_conn = None;
            st.output_conn = None;
            st.connected_port_name = None;
            {
                let mut as_ = app_state.lock().unwrap();
                as_.connected = false;
                as_.settings = None;
            }
            app.emit("device-disconnected", DeviceDisconnectedPayload {})
                .ok();
            eprintln!("[midi] Expresso disconnected");
        }
    }

    // Detect new connection
    if !st.is_connected() {
        let target = port_names.iter().find(|n| n.contains(DEVICE_NAME)).cloned();

        if let Some(port_name) = target {
            if let (Some(in_conn), Some(out_conn)) =
                (open_input(&port_name, sysex_tx), open_output(&port_name))
            {
                eprintln!("[midi] Expresso connected: {port_name}");
                st.input_conn = Some(in_conn);
                st.output_conn = Some(out_conn);
                st.connected_port_name = Some(port_name.clone());
                app_state.lock().unwrap().connected = true;
                app.emit(
                    "device-connected",
                    DeviceConnectedPayload { name: port_name },
                )
                .ok();
                return true;
            }
        }
    }

    false
}

fn open_input(port_name: &str, sysex_tx: mpsc::Sender<Vec<u8>>) -> Option<MidiInputConnection<()>> {
    let mut mi = MidiInput::new("expresso-in").ok()?;
    // midir ignores SysEx by default — explicitly receive everything
    mi.ignore(midir::Ignore::None);
    let port = mi
        .ports()
        .into_iter()
        .find(|p| mi.port_name(p).ok().as_deref() == Some(port_name))?;

    mi.connect(
        &port,
        "expresso-rx",
        move |_ts, data, _| {
            if !data.is_empty() {
                sysex_tx.blocking_send(data.to_vec()).ok();
            }
        },
        (),
    )
    .ok()
}

fn open_output(port_name: &str) -> Option<MidiOutputConnection> {
    let mo = MidiOutput::new("expresso-out").ok()?;
    let port = mo
        .ports()
        .into_iter()
        .find(|p| mo.port_name(p).ok().as_deref() == Some(port_name))?;
    mo.connect(&port, "expresso-tx").ok()
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
async fn patch_settings(
    patch: SettingsPatch,
    patch_tx: tauri::State<'_, mpsc::Sender<SettingsPatch>>,
) -> Result<(), String> {
    patch_tx.send(patch).await.map_err(|e| e.to_string())
}

#[tauri::command]
fn get_initial_state(state: tauri::State<'_, AppState>) -> InitialState {
    let st = state.lock().unwrap();
    InitialState {
        connected: st.connected,
        settings: st.settings,
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (patch_tx, patch_rx) = mpsc::channel::<SettingsPatch>(32);
    let app_state: AppState = Arc::new(Mutex::new(SharedState {
        connected: false,
        settings: None,
    }));
    let midi_app_state = app_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(patch_tx)
        .manage(app_state)
        .setup(|app| {
            let handle = app.handle().clone();
            start_midi_thread(handle, patch_rx, midi_app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![patch_settings, get_initial_state])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
