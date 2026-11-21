pub mod app;
pub mod audio_core;
pub mod discord;
pub mod i18n;
pub mod local_files;
pub mod storage;
pub mod taskbar;
pub mod ui;
pub mod updater;

use iced::window;
use iced::Size;

pub fn load_window_icon() -> Option<window::Icon> {
    let png_bytes = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(png_bytes).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    window::icon::from_rgba(img.into_raw(), w, h).ok()
}

pub fn run_with_args<I, S>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let opts = app::parse_cli_args(args);
    if opts.show_help {
        println!(
            "Raser {}\nUsage: raser [--view <discover|favorites|albums|local|history|queue|settings>] [--headless-check] [--help]",
            updater::CURRENT_VERSION
        );
        return Ok(());
    }

    if opts.headless_check {
        let (state, _task) = app::RaserApp::boot(opts.initial_view);
        log::info!(
            "Headless check ok: view={:?}, playlists={}, local_tracks={}",
            state.active_view,
            state.snapshot.playlists.len(),
            state.snapshot.local_tracks.len()
        );
        return Ok(());
    }

    let initial_view = opts.initial_view;
    let icon = load_window_icon();

    iced::application(
        move || app::RaserApp::boot(initial_view.clone()),
        app::RaserApp::update,
        app::RaserApp::view,
    )
    .title(app::RaserApp::title)
    .theme(app::RaserApp::theme)
    .subscription(app::RaserApp::subscription)
    .window(window::Settings {
        size: Size::new(1280.0, 820.0),
        min_size: Some(Size::new(400.0, 550.0)),
        decorations: false,
        resizable: true,
        transparent: false,
        icon,
        ..Default::default()
    })
    .run()
    .map_err(|e| format!("iced application error: {:?}", e))
}

pub fn run() -> Result<(), String> {
    run_with_args(std::env::args())
}
