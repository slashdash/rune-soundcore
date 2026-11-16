use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use macaddr::MacAddr6;
use openscq30_lib::device::OpenSCQ30Device;
use openscq30_lib::settings::{Setting, SettingId, Value};
use openscq30_lib::storage::PairedDevice;
use openscq30_lib::{DeviceModel, OpenSCQ30Session};
use rune::AppHandle;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

use crate::state::{AppState, Snapshot};

/// Everything the UI asks the Bluetooth worker to do.
#[derive(Debug)]
pub enum Cmd {
    Scan(DeviceModel),
    PairAndConnect(DeviceModel, MacAddr6),
    Connect(MacAddr6),
    Disconnect,
    Set(SettingId, Value),
    Forget(MacAddr6),
    Refresh,
}

pub fn spawn(handle: AppHandle<AppState>, rx: UnboundedReceiver<Cmd>, probe: bool) {
    std::thread::Builder::new()
        .name("soundcore-bt".to_owned())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("failed to create the tokio runtime");
            runtime.block_on(run(rx, handle, probe));
        })
        .expect("failed to spawn the bluetooth thread");
}

fn describe(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !text.is_empty() {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

/// The app's data directory, created private to this user. `None` if the system has no data dir.
fn data_dir() -> Option<PathBuf> {
    let dir = dirs::data_dir()?.join("rune-soundcore");
    std::fs::create_dir_all(&dir).ok()?;
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    Some(dir)
}

/// Keep device identifiers and pairings out of reach of other local users.
fn restrict(path: &Path) {
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

/// Probe output holds the device serial number, so it is written private and never into a
/// shared, predictable path such as `/tmp`.
fn write_snapshot(path: Option<&Path>, json: &str) {
    let Some(path) = path else {
        eprintln!("could not find a folder for the snapshot");
        return;
    };
    let opened = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path);
    match opened {
        Ok(mut file) => {
            use std::io::Write;
            if let Err(error) = file.write_all(json.as_bytes()) {
                eprintln!("could not write {}: {error}", path.display());
                return;
            }
            drop(file);
            restrict(path);
            eprintln!("snapshot written to {}", path.display());
        }
        Err(error) => eprintln!("could not write {}: {error}", path.display()),
    }
}

fn database_path() -> Option<PathBuf> {
    Some(data_dir()?.join("soundcore.sqlite3"))
}

fn snapshot_path() -> Option<PathBuf> {
    Some(data_dir()?.join("snapshot.json"))
}

fn snapshot(device: &Arc<dyn OpenSCQ30Device + Send + Sync>) -> Snapshot {
    Snapshot {
        model: device.model(),
        categories: device
            .settings_by_category()
            .into_iter()
            .map(|(category, settings)| (category, settings.into_iter().collect()))
            .collect(),
    }
}

async fn run(mut rx: UnboundedReceiver<Cmd>, handle: AppHandle<AppState>, probe: bool) {
    let Some(database) = database_path() else {
        handle.update(|state, _| {
            state.error("Could not reach the Application Support folder for settings.");
        });
        return;
    };
    let session = match OpenSCQ30Session::new(database.clone()).await {
        Ok(session) => session,
        Err(error) => {
            let message = format!("Could not open the settings database: {error}");
            handle.update(move |state, _| state.error(message));
            return;
        }
    };
    restrict(&database);

    let mut device: Option<Arc<dyn OpenSCQ30Device + Send + Sync>> = None;
    let mut watchers: Vec<JoinHandle<()>> = Vec::new();

    match session.paired_devices().await {
        Ok(paired) => {
            tracing::info!(count = paired.len(), "loaded paired devices");
            let initial = paired.clone();
            handle.update(move |state, _| state.paired = initial);
            if let Some(first) = paired.first().filter(|_| !probe) {
                let mac = first.mac_address;
                handle.update(move |state, _| state.set_connecting(mac));
                if let Some(connected) = connect_with_probe(&session, &handle, mac, probe).await {
                    start_watchers(&handle, &mut watchers, &connected);
                    device = Some(connected);
                }
            }
        }
        Err(error) => tracing::warn!(%error, "could not list paired devices"),
    }

    if probe && device.is_none() {
        let model = DeviceModel::SoundcoreA3947;
        handle.update(|state, _| state.set_busy("Scanning for Bluetooth devices…"));
        match session.list_devices(model).await {
            Ok(devices) => {
                tracing::info!(count = devices.len(), "found devices (probe)");
                for found in &devices {
                    tracing::info!(name = %found.name, mac = %found.mac_address, "candidate");
                }
                let candidate = devices
                    .iter()
                    .find(|device| device.name.to_lowercase().contains("soundcore"))
                    .or_else(|| devices.first());
                if let Some(first) = candidate {
                    let mac = first.mac_address;
                    let paired = PairedDevice {
                        mac_address: mac,
                        model,
                        is_demo: false,
                    };
                    if let Err(error) = session.pair(paired).await {
                        eprintln!("could not pair {mac}: {error}");
                    }
                    handle.update(move |state, _| {
                        if !state.paired.iter().any(|d| d.mac_address == mac) {
                            state.paired.push(paired);
                        }
                        state.set_connecting(mac);
                    });
                    if let Some(connected) = connect_with_probe(&session, &handle, mac, probe).await
                    {
                        start_watchers(&handle, &mut watchers, &connected);
                        device = Some(connected);
                    }
                } else {
                    eprintln!("no bluetooth devices found");
                    handle.quit();
                }
            }
            Err(error) => {
                eprintln!("could not list devices: {error}");
                handle.quit();
            }
        }
    }

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Cmd::Scan(model) => {
                handle.update(move |state, _| {
                    state.model = model;
                    state.page = crate::state::Page::Devices;
                    state.set_busy("Scanning for Bluetooth devices…");
                });
                match session.list_devices(model).await {
                    Ok(devices) => {
                        tracing::info!(count = devices.len(), "found devices");
                        handle.update(move |state, _| {
                            state.scanning = false;
                            state.note = if devices.is_empty() {
                                Some(
                                    "No paired devices found. Pair the earbuds in System Settings first."
                                        .to_owned(),
                                )
                            } else {
                                None
                            };
                            state.devices = devices;
                        });
                    }
                    Err(error) => {
                        let message =
                            format!("Could not list Bluetooth devices: {}", describe(&error));
                        handle.update(move |state, _| state.error(message));
                    }
                }
            }
            Cmd::PairAndConnect(model, mac) => {
                let paired = PairedDevice {
                    mac_address: mac,
                    model,
                    is_demo: false,
                };
                match session.pair(paired).await {
                    Ok(()) => {
                        handle.update(move |state, _| {
                            if !state.paired.iter().any(|d| d.mac_address == mac) {
                                state.paired.push(paired);
                            }
                        });
                        stop(&mut watchers);
                        device = None;
                        handle.update(move |state, _| state.set_connecting(mac));
                        if let Some(connected) =
                            connect_with_probe(&session, &handle, mac, probe).await
                        {
                            start_watchers(&handle, &mut watchers, &connected);
                            device = Some(connected);
                        }
                    }
                    Err(error) => {
                        let message = format!("Could not save the pairing: {error}");
                        handle.update(move |state, _| state.error(message));
                    }
                }
            }
            Cmd::Connect(mac) => {
                stop(&mut watchers);
                device = None;
                handle.update(move |state, _| state.set_connecting(mac));
                if let Some(connected) = connect_with_probe(&session, &handle, mac, probe).await {
                    start_watchers(&handle, &mut watchers, &connected);
                    device = Some(connected);
                }
            }
            Cmd::Disconnect => {
                stop(&mut watchers);
                device = None;
                handle.update(|state, _| {
                    state.set_disconnected();
                    state.note = Some("Disconnected.".to_owned());
                });
            }
            Cmd::Set(id, value) => {
                let Some(connected) = device.as_ref() else {
                    continue;
                };
                match connected.set_setting_values(vec![(id, value)]).await {
                    Ok(()) => {
                        let snapshot = snapshot(connected);
                        handle.update(move |state, _| state.apply_snapshot(snapshot, true));
                    }
                    Err(error) => {
                        let message = format!("Could not change the setting: {}", describe(&error));
                        handle.update(move |state, _| state.error(message));
                    }
                }
            }
            Cmd::Forget(mac) => {
                stop(&mut watchers);
                device = None;
                match session.unpair(mac).await {
                    Ok(()) => {
                        handle.update(move |state, _| {
                            state.paired.retain(|pair| pair.mac_address != mac);
                            state.set_disconnected();
                        });
                    }
                    Err(error) => {
                        let message = format!("Could not forget the device: {error}");
                        handle.update(move |state, _| state.error(message));
                    }
                }
            }
            Cmd::Refresh => {
                if let Some(connected) = device.as_ref() {
                    let snapshot = snapshot(connected);
                    handle.update(move |state, _| state.apply_snapshot(snapshot, true));
                }
            }
        }
    }
}

async fn connect_with_probe(
    session: &OpenSCQ30Session,
    handle: &AppHandle<AppState>,
    mac: MacAddr6,
    probe: bool,
) -> Option<Arc<dyn OpenSCQ30Device + Send + Sync>> {
    match session.connect(mac).await {
        Ok(device) => {
            tracing::info!(%mac, "connected");
            let snapshot = snapshot(&device);
            if probe {
                report(&snapshot);
                if std::env::var("RUNE_SOUNDCORE_PROBE_WRITE").is_ok() {
                    write_test(&device).await;
                }
                if let Ok(json) = serde_json::to_string_pretty(&snapshot) {
                    let path = std::env::var_os("RUNE_SOUNDCORE_DUMP")
                        .map(PathBuf::from)
                        .or_else(snapshot_path);
                    write_snapshot(path.as_deref(), &json);
                }
            }
            handle.update(move |state, _| {
                state.set_connected(mac);
                state.apply_snapshot(snapshot, false);
            });
            if probe {
                handle.quit();
            }
            Some(device)
        }
        Err(error) => {
            tracing::warn!(%mac, %error, "connect failed");
            let message = format!("Could not connect: {}", describe(&error));
            handle.update(move |state, _| state.error(message));
            if probe {
                handle.quit();
            }
            None
        }
    }
}

/// Writes a harmless setting twice and prints what the device reports, to prove the write path
/// end to end. Leaves the earbuds as they were.
async fn write_test(device: &Arc<dyn OpenSCQ30Device + Send + Sync>) {
    use openscq30_lib::settings::SettingId;

    let original = match device.setting(&SettingId::TouchTone) {
        Some(Setting::Toggle { value }) => value,
        _ => {
            eprintln!("write test skipped: TouchTone is not a toggle");
            return;
        }
    };
    let flipped = !original;
    eprintln!("write test: TouchTone {original} -> {flipped}");
    match device
        .set_setting_values(vec![(SettingId::TouchTone, Value::Bool(flipped))])
        .await
    {
        Ok(()) => {}
        Err(error) => {
            eprintln!("write test failed: {}", describe(&error));
            return;
        }
    }
    match device.setting(&SettingId::TouchTone) {
        Some(Setting::Toggle { value }) => {
            eprintln!("write test: device now reports TouchTone {value}")
        }
        other => eprintln!("write test: unexpected read back {other:?}"),
    }
    let _ = device
        .set_setting_values(vec![(SettingId::TouchTone, Value::Bool(original))])
        .await;
    eprintln!("write test: restored TouchTone {original}");
}

fn report(snapshot: &Snapshot) {
    eprintln!("model: {:?}", snapshot.model);
    for (category, settings) in &snapshot.categories {
        eprintln!("[{category:?}]");
        for (id, setting) in settings {
            eprintln!("  {id:?} = {}", describe_setting(setting));
        }
    }
}

fn describe_setting(setting: &Setting) -> String {
    match setting {
        Setting::Toggle { value } => value.to_string(),
        Setting::I32Range { value, setting } => format!("{value} ({setting:?})"),
        Setting::Select { value, .. } => format!("{value:?}"),
        Setting::OptionalSelect { value, .. } | Setting::ModifiableSelect { value, .. } => {
            format!("{value:?}")
        }
        Setting::MultiSelect { values, .. } | Setting::MultiSelectWithRemove { values, .. } => {
            format!("{values:?}")
        }
        Setting::Equalizer { value, .. } => format!("{value:?}"),
        Setting::PresetEqualizerProfileSelect { value, .. } => format!("{value:?}"),
        Setting::Information { value, .. } => value.clone(),
        Setting::ImportString { .. } => "import".to_owned(),
        Setting::HueColorPicker { hue } => format!("hue {hue}"),
        Setting::Action => "action".to_owned(),
        Setting::TimeOfDay {
            minutes_after_midnight,
        } => format!("{minutes_after_midnight}m"),
    }
}

fn start_watchers(
    handle: &AppHandle<AppState>,
    watchers: &mut Vec<JoinHandle<()>>,
    device: &Arc<dyn OpenSCQ30Device + Send + Sync>,
) {
    let mut changes = device.watch_for_changes();
    let changed_device = device.clone();
    let changed_handle = handle.clone();
    watchers.push(tokio::spawn(async move {
        while changes.changed().await.is_ok() {
            let snapshot = snapshot(&changed_device);
            changed_handle.update(move |state, _| state.apply_snapshot(snapshot, true));
        }
    }));

    let mut status = device.connection_status();
    let status_handle = handle.clone();
    watchers.push(tokio::spawn(async move {
        while status.changed().await.is_ok() {
            let connected = *status.borrow_and_update()
                == openscq30_lib::connection::ConnectionStatus::Connected;
            status_handle.update(move |state, _| state.set_status(connected));
        }
    }));
}

fn stop(watchers: &mut Vec<JoinHandle<()>>) {
    for watcher in watchers.drain(..) {
        watcher.abort();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use openscq30_lib::settings::Range;

    use super::*;

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "rune-soundcore-test-{}-{nanos}-{id}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn snapshot_is_private_and_truncates() {
        let dir = temp_dir();
        let path = dir.join("snapshot.json");
        write_snapshot(Some(&path), r#"{"long":true}"#);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), r#"{"long":true}"#);
        assert_eq!(mode(&path), 0o600);
        write_snapshot(Some(&path), "{}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn snapshot_into_a_missing_folder_does_not_panic() {
        let dir = temp_dir();
        let path = dir.join("missing").join("snapshot.json");
        write_snapshot(Some(&path), "{}");
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restrict_makes_files_private() {
        let dir = temp_dir();
        let path = dir.join("soundcore.sqlite3");
        std::fs::write(&path, "x").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        restrict(&path);
        assert_eq!(mode(&path), 0o600);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn describe_setting_covers_the_rendered_kinds() {
        let setting = Setting::I32Range {
            setting: Range {
                range: 1..=5,
                step: 1,
            },
            value: 3,
        };
        assert_eq!(
            describe_setting(&setting),
            "3 (Range { range: 1..=5, step: 1 })"
        );
        assert_eq!(describe_setting(&Setting::Toggle { value: true }), "true");
        assert_eq!(
            describe_setting(&Setting::Information {
                value: "07.61".to_owned(),
                translated_value: "07.61".to_owned(),
            }),
            "07.61"
        );
        assert_eq!(
            describe_setting(&Setting::TimeOfDay {
                minutes_after_midnight: 90
            }),
            "90m"
        );
        assert_eq!(describe_setting(&Setting::Action), "action");
    }
}
