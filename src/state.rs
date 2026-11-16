use std::collections::HashMap;

use macaddr::MacAddr6;
use openscq30_lib::DeviceModel;
use openscq30_lib::connection::ConnectionDescriptor;
use openscq30_lib::settings::{CategoryId, Setting, SettingId};
use openscq30_lib::storage::PairedDevice;
use tokio::sync::mpsc::UnboundedSender;

use crate::bt::Cmd;

/// Where the app is in the connect lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Devices,
    Category(CategoryId),
}

/// Everything the device reports, flattened into a snapshot for the UI.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Snapshot {
    pub model: DeviceModel,
    pub categories: Vec<(CategoryId, Vec<(SettingId, Setting)>)>,
}

impl Snapshot {
    pub fn setting(&self, id: &SettingId) -> Option<&Setting> {
        self.categories
            .iter()
            .flat_map(|(_, settings)| settings.iter())
            .find(|(setting_id, _)| setting_id == id)
            .map(|(_, setting)| setting)
    }

    pub fn category(&self, category: &CategoryId) -> Option<&Vec<(SettingId, Setting)>> {
        self.categories
            .iter()
            .find(|(id, _)| id == category)
            .map(|(_, settings)| settings)
    }
}

pub struct AppState {
    pub tx: UnboundedSender<Cmd>,
    pub connection: Connection,
    pub connected_mac: Option<MacAddr6>,
    pub model: DeviceModel,
    pub snapshot: Option<Snapshot>,
    pub devices: Vec<ConnectionDescriptor>,
    pub paired: Vec<PairedDevice>,
    pub scanning: bool,
    /// A one-line status or error message under the device header.
    pub note: Option<String>,
    pub page: Page,
    /// Key of the open `Select`, if any.
    pub open_select: Option<String>,
    /// Values being dragged, so sliders follow the pointer without writing to the device.
    pub draft: HashMap<SettingId, f32>,
    /// Equalizer bands being dragged.
    pub eq_draft: Option<Vec<i16>>,
}

impl AppState {
    pub fn new(tx: UnboundedSender<Cmd>) -> Self {
        let mut state = Self::base(tx);
        state.load_env_snapshot();
        state
    }

    fn base(tx: UnboundedSender<Cmd>) -> Self {
        Self {
            tx,
            connection: Connection::Disconnected,
            connected_mac: None,
            model: DeviceModel::SoundcoreA3947,
            snapshot: None,
            devices: Vec::new(),
            paired: Vec::new(),
            scanning: false,
            note: None,
            page: Page::Devices,
            open_select: None,
            draft: HashMap::new(),
            eq_draft: None,
        }
    }

    /// A saved snapshot lets the UI render without hardware (`just snap`).
    fn load_env_snapshot(&mut self) {
        let Ok(path) = std::env::var("RUNE_SOUNDCORE_SNAPSHOT") else {
            return;
        };
        match std::fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|json| {
                serde_json::from_str::<Snapshot>(&json).map_err(|error| error.to_string())
            }) {
            Ok(snapshot) => {
                self.model = snapshot.model;
                self.apply_snapshot(snapshot, false);
                if let Ok(page) = std::env::var("RUNE_SOUNDCORE_PAGE")
                    && let Ok(category) = page.parse::<CategoryId>()
                {
                    self.page = Page::Category(category);
                }
            }
            Err(error) => self.note = Some(format!("Could not load {path}: {error}")),
        }
    }

    /// The initial state used before `App::init` replaces it; never shown.
    fn placeholder() -> Self {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        Self::base(tx)
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn is_connected(&self) -> bool {
        self.connection == Connection::Connected
    }

    pub fn apply_snapshot(&mut self, snapshot: Snapshot, keep_page: bool) {
        if !keep_page && self.page == Page::Devices && self.snapshot.is_none() {
            let first = snapshot
                .categories
                .iter()
                .map(|(id, _)| *id)
                .find(|id| *id != CategoryId::DeviceInformation)
                .or_else(|| snapshot.categories.first().map(|(id, _)| *id));
            if let Some(id) = first {
                self.page = Page::Category(id);
            }
        }
        self.connection = Connection::Connected;
        self.snapshot = Some(snapshot);
        self.draft.clear();
        self.eq_draft = None;
    }

    pub fn set_connected(&mut self, mac: MacAddr6) {
        self.connection = Connection::Connected;
        self.connected_mac = Some(mac);
        self.note = None;
    }

    pub fn set_connecting(&mut self, mac: MacAddr6) {
        self.connection = Connection::Connecting;
        self.connected_mac = Some(mac);
        self.note = Some("Connecting…".to_owned());
    }

    pub fn set_disconnected(&mut self) {
        self.connection = Connection::Disconnected;
        self.snapshot = None;
        self.draft.clear();
        self.eq_draft = None;
        self.page = Page::Devices;
        if self.note.is_none() {
            self.note = Some("Disconnected".to_owned());
        }
    }

    pub fn set_status(&mut self, connected: bool) {
        if connected {
            if self.connection == Connection::Connecting {
                self.connection = Connection::Connected;
                self.note = None;
            }
        } else {
            self.set_disconnected();
            self.note = Some("The earbuds disconnected.".to_owned());
        }
    }

    pub fn set_busy(&mut self, message: impl Into<String>) {
        self.scanning = true;
        self.note = Some(message.into());
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.scanning = false;
        self.note = Some(message.into());
        if self.connection == Connection::Connecting {
            self.connection = Connection::Disconnected;
        }
    }

    pub fn setting(&self, id: &SettingId) -> Option<&Setting> {
        self.snapshot.as_ref()?.setting(id)
    }

    /// A slider's current position: the draft while dragging, else the device's value.
    pub fn slider_value(&self, id: &SettingId, fallback: f32) -> f32 {
        self.draft
            .get(id)
            .copied()
            .or_else(|| match self.setting(id) {
                Some(Setting::I32Range { value, .. }) => Some(*value as f32),
                _ => None,
            })
            .unwrap_or(fallback)
    }

    pub fn categories(&self) -> Vec<CategoryId> {
        self.snapshot
            .as_ref()
            .map(|snapshot| {
                snapshot
                    .categories
                    .iter()
                    .filter(|(id, settings)| {
                        !settings.is_empty() && !matches!(id, CategoryId::EqualizerImportExport)
                    })
                    .map(|(id, _)| *id)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl AppState {
    pub fn placeholder_init() -> fn() -> Self {
        Self::placeholder
    }
}

#[cfg(test)]
mod tests {
    use openscq30_lib::settings::Range;

    use super::*;

    fn snapshot(categories: Vec<(CategoryId, Vec<(SettingId, Setting)>)>) -> Snapshot {
        Snapshot {
            model: DeviceModel::SoundcoreA3947,
            categories,
        }
    }

    fn information(value: &str) -> Setting {
        Setting::Information {
            value: value.to_owned(),
            translated_value: value.to_owned(),
        }
    }

    fn state() -> AppState {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        AppState::base(tx)
    }

    #[test]
    fn first_snapshot_opens_a_settings_page() {
        let mut state = state();
        state.apply_snapshot(
            snapshot(vec![
                (
                    CategoryId::DeviceInformation,
                    vec![(SettingId::SerialNumber, information("x"))],
                ),
                (
                    CategoryId::SoundModes,
                    vec![(
                        SettingId::WindNoiseSuppression,
                        Setting::Toggle { value: false },
                    )],
                ),
            ]),
            false,
        );
        assert_eq!(state.page, Page::Category(CategoryId::SoundModes));
        assert_eq!(state.connection, Connection::Connected);
    }

    #[test]
    fn reapply_keeps_the_page_the_user_chose() {
        let mut state = state();
        state.apply_snapshot(snapshot(vec![(CategoryId::SoundModes, Vec::new())]), false);
        state.page = Page::Devices;
        state.apply_snapshot(
            snapshot(vec![(CategoryId::Miscellaneous, Vec::new())]),
            true,
        );
        assert_eq!(state.page, Page::Devices);
    }

    #[test]
    fn categories_hide_empty_and_import_export() {
        let mut state = state();
        state.apply_snapshot(
            snapshot(vec![
                (CategoryId::SoundModes, Vec::new()),
                (
                    CategoryId::EqualizerImportExport,
                    vec![(SettingId::ImportCustomEqualizerProfiles, information(""))],
                ),
                (
                    CategoryId::Miscellaneous,
                    vec![(SettingId::TouchTone, Setting::Toggle { value: true })],
                ),
            ]),
            false,
        );
        assert_eq!(state.categories(), vec![CategoryId::Miscellaneous]);
    }

    #[test]
    fn slider_value_prefers_the_draft() {
        let id = SettingId::ManualNoiseCanceling;
        let mut state = state();
        state.apply_snapshot(
            snapshot(vec![(
                CategoryId::SoundModes,
                vec![(
                    id,
                    Setting::I32Range {
                        setting: Range {
                            range: 1..=5,
                            step: 1,
                        },
                        value: 2,
                    },
                )],
            )]),
            false,
        );
        assert_eq!(state.slider_value(&id, 9.0), 2.0);
        state.draft.insert(id, 4.0);
        assert_eq!(state.slider_value(&id, 9.0), 4.0);
        assert_eq!(state.slider_value(&SettingId::TouchTone, 9.0), 9.0);
    }

    #[test]
    fn disconnect_returns_to_the_devices_page() {
        let mut state = state();
        state.apply_snapshot(snapshot(vec![(CategoryId::SoundModes, Vec::new())]), false);
        state.set_status(false);
        assert_eq!(state.page, Page::Devices);
        assert_eq!(state.connection, Connection::Disconnected);
        assert!(state.snapshot.is_none());
    }

    #[test]
    fn connecting_then_connected_clears_the_note() {
        let mac: MacAddr6 = "00:00:00:00:00:01".parse().unwrap();
        let mut state = state();
        state.set_connecting(mac);
        assert_eq!(state.connection, Connection::Connecting);
        assert!(state.note.is_some());
        state.set_status(true);
        assert_eq!(state.connection, Connection::Connected);
        assert!(state.note.is_none());
    }

    #[test]
    fn snapshot_json_round_trips() {
        let original = snapshot(vec![(
            CategoryId::Miscellaneous,
            vec![
                (SettingId::TouchTone, Setting::Toggle { value: true }),
                (SettingId::FirmwareVersion, information("07.61")),
            ],
        )]);
        let json = serde_json::to_string(&original).unwrap();
        let restored: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.model, original.model);
        assert_eq!(
            restored.setting(&SettingId::TouchTone),
            original.setting(&SettingId::TouchTone)
        );
        assert_eq!(
            restored.setting(&SettingId::FirmwareVersion),
            original.setting(&SettingId::FirmwareVersion)
        );
    }

    #[test]
    fn send_forwards_commands_to_the_worker() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let state = AppState::base(tx);
        state.send(Cmd::Refresh);
        assert!(matches!(rx.try_recv(), Ok(Cmd::Refresh)));
    }
}
