mod bt;
mod state;
mod ui;

use rune::{App, AppSpec, Key, KeyEvent, Tx};
use tokio::sync::mpsc::unbounded_channel;

use crate::state::AppState;

fn on_key(state: &mut AppState, event: &KeyEvent, tx: &mut Tx) -> bool {
    match event.key {
        Key::Command('q') => tx.quit(),
        Key::Command('r') => {
            state.send(bt::Cmd::Refresh);
        }
        _ => return false,
    }
    true
}

fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,rune_soundcore=info,openscq30_lib=info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

fn main() {
    init_logging();
    openscq30_lib::i18n::init(&[]);

    let probe = std::env::args().any(|argument| argument == "--probe");
    let (tx, rx) = unbounded_channel();

    let spec = AppSpec::<AppState> {
        scenes: &[],
        on_key: Some(on_key),
        ..AppSpec::new(
            "rune-soundcore",
            (960.0, 720.0),
            AppState::placeholder_init(),
            ui::view,
        )
    };
    let app = App::new(spec)
        .init(move || AppState::new(tx))
        .standard_menus();
    let handle = app.handle();
    // A saved snapshot renders offline, so don't touch the real earbuds from a snapshot run.
    let offline = std::env::var_os("RUNE_SOUNDCORE_SNAPSHOT").is_some() && !probe;
    if offline {
        drop(rx);
    } else {
        bt::spawn(handle, rx, probe);
    }
    app.run();
}
