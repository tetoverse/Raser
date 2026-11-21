mod audio_core;
mod bg_queue;
mod discord;
mod local_files;
mod storage;
mod taskbar;
mod updater;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutState};

static ALLOW_EXIT: AtomicBool = AtomicBool::new(false);
static WINDOW_TRANSITIONING: AtomicBool = AtomicBool::new(false);

#[tauri::command]
async fn rust_audio_load(
    audio_core: tauri::State<'_, Arc<audio_core::AudioCore>>,
    url: String,
    start_pos: Option<f64>,
    duration: Option<f64>,
    autoplay: Option<bool>,
    track_id: Option<String>,
) -> Result<audio_core::AudioState, String> {
    audio_core.load_audio_async(url, start_pos, duration, autoplay.unwrap_or(true), track_id).await
}

#[tauri::command]
fn get_cache_lyrics(track_id: String) -> Option<String> {
    storage::get_cached_lyrics(&track_id)
}

#[tauri::command]
fn save_cache_lyrics(track_id: String, lyrics_json: String) {
    storage::save_cached_lyrics(&track_id, &lyrics_json);
}

#[tauri::command]
fn get_cache_stats() -> serde_json::Value {
    let (bytes, count) = storage::get_cache_stats();
    serde_json::json!({
        "bytes": bytes,
        "count": count,
        "path": storage::get_cache_dir().to_string_lossy().to_string()
    })
}

#[tauri::command]
fn clear_disk_cache() {
    storage::clear_cache();
}

#[tauri::command]
fn open_cache_folder() {
    let p = storage::get_cache_dir();
    let _ = std::fs::create_dir_all(&p);
    open_external(p.to_string_lossy().to_string());
}

#[tauri::command]
fn rust_audio_play(audio_core: tauri::State<Arc<audio_core::AudioCore>>) -> Result<(), String> {
    audio_core.play()
}

#[tauri::command]
fn rust_audio_pause(audio_core: tauri::State<Arc<audio_core::AudioCore>>) -> Result<(), String> {
    audio_core.pause()
}

#[tauri::command]
fn rust_audio_toggle(audio_core: tauri::State<Arc<audio_core::AudioCore>>) -> Result<bool, String> {
    audio_core.toggle()
}

#[tauri::command]
async fn rust_audio_seek(
    audio_core: tauri::State<'_, Arc<audio_core::AudioCore>>,
    position: f64,
) -> Result<audio_core::AudioState, String> {
    audio_core.seek_async(position).await
}

#[tauri::command]
fn rust_audio_set_volume(audio_core: tauri::State<Arc<audio_core::AudioCore>>, volume: f32) -> Result<(), String> {
    audio_core.set_volume(volume)
}

#[tauri::command]
fn rust_audio_set_speed(audio_core: tauri::State<Arc<audio_core::AudioCore>>, speed: f32) -> Result<(), String> {
    audio_core.set_speed(speed)
}

#[tauri::command]
fn rust_audio_get_state(audio_core: tauri::State<Arc<audio_core::AudioCore>>) -> audio_core::AudioState {
    audio_core.get_state()
}

#[tauri::command]
fn rust_audio_get_frequency_data(audio_core: tauri::State<Arc<audio_core::AudioCore>>) -> Vec<u8> {
    audio_core.get_frequency_data()
}

fn set_webview_visibility(app: &tauri::AppHandle, visible: bool) {
    let _ = app.emit("window-visibility", visible);
    if let Some(w) = app.get_webview_window("main") {
        let script = format!("window.__raserSetVisibility && window.__raserSetVisibility({});", visible);
        let _ = w.eval(&script);
    }
}

#[tauri::command]
fn minimize_window(window: tauri::Window) {
    set_webview_visibility(window.app_handle(), false);
    let _ = window.minimize();
    trim_working_set();
}

#[tauri::command]
fn toggle_maximize_window(window: tauri::Window) {
    if let Ok(is_maximized) = window.is_maximized() {
        if is_maximized {
            let _ = window.unmaximize();
        } else {
            let _ = window.maximize();
        }
    }
}

#[tauri::command]
fn toggle_fullscreen_window(window: tauri::Window) {
    if let Ok(is_fs) = window.is_fullscreen() {
        let _ = window.set_fullscreen(!is_fs);
    }
}

#[tauri::command]
fn sync_playback_snapshot(
    bg_queue: tauri::State<Arc<bg_queue::BackgroundQueue>>,
    snapshot: bg_queue::PlaybackSnapshot,
) {
    bg_queue.update_snapshot(snapshot);
}

#[tauri::command]
fn get_resume_snapshot(
    bg_queue: tauri::State<Arc<bg_queue::BackgroundQueue>>,
    audio_core: tauri::State<Arc<audio_core::AudioCore>>,
) -> Option<bg_queue::ResumePayload> {
    bg_queue.build_resume_payload(audio_core.get_state())
}

#[tauri::command]
fn set_tray_on_close(bg_queue: tauri::State<Arc<bg_queue::BackgroundQueue>>, enabled: bool) {
    bg_queue.set_tray_on_close(enabled);
}

#[tauri::command]
fn set_low_ram_tray(bg_queue: tauri::State<Arc<bg_queue::BackgroundQueue>>, enabled: bool) {
    bg_queue.set_low_ram_tray(enabled);
}

fn attach_main_window_handlers(window: &tauri::WebviewWindow) {
    let w_clone = window.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::Resized(_) => {
            if let Ok(minimized) = w_clone.is_minimized() {
                if minimized {
                    let _ = w_clone.eval("window.__raserSetVisibility && window.__raserSetVisibility(false);");
                    trim_working_set();
                } else {
                    let _ = w_clone.eval("window.__raserSetVisibility && window.__raserSetVisibility(true);");
                }
            }
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            let app = w_clone.app_handle().clone();
            let should_tray = app
                .try_state::<Arc<bg_queue::BackgroundQueue>>()
                .map(|q| q.tray_on_close())
                .unwrap_or(true);
            if should_tray {
                api.prevent_close();
                std::thread::spawn(move || {
                    suspend_to_tray(&app);
                });
            } else {
                ALLOW_EXIT.store(true, Ordering::SeqCst);
            }
        }
        _ => {}
    });
}

fn suspend_to_tray(app: &tauri::AppHandle) {
    if WINDOW_TRANSITIONING.swap(true, Ordering::SeqCst) {
        return;
    }
    struct TransitionGuard;
    impl Drop for TransitionGuard {
        fn drop(&mut self) {
            WINDOW_TRANSITIONING.store(false, Ordering::SeqCst);
        }
    }
    let _guard = TransitionGuard;

    let low_ram = app
        .try_state::<Arc<bg_queue::BackgroundQueue>>()
        .map(|q| q.low_ram_tray())
        .unwrap_or(true);

    if low_ram {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.eval("window.__raserSyncSnapshotNow && window.__raserSyncSnapshotNow();");
            std::thread::sleep(std::time::Duration::from_millis(90));
        }
        taskbar::win::clear_hwnd();
        if let Some(q) = app.try_state::<Arc<bg_queue::BackgroundQueue>>() {
            q.set_headless(true);
        }
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.destroy();
        }
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(450));
            do_trim_working_set();
        });
    } else {
        set_webview_visibility(app, false);
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.hide();
        }
        trim_working_set();
    }
}

fn restore_main_window(app: &tauri::AppHandle) {
    if WINDOW_TRANSITIONING.swap(true, Ordering::SeqCst) {
        return;
    }
    struct TransitionGuard;
    impl Drop for TransitionGuard {
        fn drop(&mut self) {
            WINDOW_TRANSITIONING.store(false, Ordering::SeqCst);
        }
    }
    let _guard = TransitionGuard;

    if let Some(w) = app.get_webview_window("main") {
        if let Some(q) = app.try_state::<Arc<bg_queue::BackgroundQueue>>() {
            q.set_headless(false);
        }
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        let _ = w.eval("window.__raserSetVisibility && window.__raserSetVisibility(true);");
        return;
    }

    let initial_storage = storage::read_storage_raw();
    let resume_json = match (
        app.try_state::<Arc<bg_queue::BackgroundQueue>>(),
        app.try_state::<Arc<audio_core::AudioCore>>(),
    ) {
        (Some(q), Some(ac)) => {
            q.set_headless(false);
            q.build_resume_payload(ac.get_state())
                .and_then(|p| serde_json::to_string(&p).ok())
                .unwrap_or_else(|| "null".to_string())
        }
        _ => "null".to_string(),
    };

    let init_script = format!(
        "window.__RASER_INITIAL_STORAGE__ = {};\nwindow.__RASER_RESUME_SNAPSHOT__ = {};",
        initial_storage, resume_json
    );

    if let Some(cfg) = app.config().app.windows.first() {
        if let Ok(builder) = tauri::WebviewWindowBuilder::from_config(app, cfg) {
            if let Ok(window) = builder.initialization_script(&init_script).build() {
                attach_main_window_handlers(&window);
                let _ = window.show();
                let _ = window.set_focus();

                let is_tr = is_tr_locale();
                taskbar::win::setup_taskbar_buttons(app, is_tr);
                if let Some(ac) = app.try_state::<Arc<audio_core::AudioCore>>() {
                    taskbar::win::update_playback_state(ac.get_state().playing, is_tr);
                }
            }
        }
    }
}

#[tauri::command]
fn close_window(window: tauri::Window, bg_queue: tauri::State<Arc<bg_queue::BackgroundQueue>>) {
    if !bg_queue.tray_on_close() {
        ALLOW_EXIT.store(true, Ordering::SeqCst);
        std::process::exit(0);
    }
    let app = window.app_handle().clone();
    // Destroy WebView2 asynchronously so this IPC call completes cleanly first
    std::thread::spawn(move || {
        suspend_to_tray(&app);
    });
}

#[tauri::command]
fn exit_app() {
    ALLOW_EXIT.store(true, Ordering::SeqCst);
    std::process::exit(0);
}

#[tauri::command]
fn open_external(url: String) {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", &url])
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    }
}

#[tauri::command]
fn discord_rpc_update(track: discord::DiscordTrackInfo) {
    discord::update_presence(track);
}

#[tauri::command]
fn discord_rpc_clear() {
    discord::clear_presence();
}

#[tauri::command]
async fn check_update() -> Result<updater::UpdateInfo, String> {
    updater::check_for_update().await
}

#[tauri::command]
async fn install_update(app: tauri::AppHandle, download_url: String) -> Result<(), String> {
    updater::download_and_install_update(app, download_url).await
}

#[tauri::command]
fn pick_local_files() -> Vec<local_files::LocalTrack> {
    local_files::pick_files()
}

#[tauri::command]
fn pick_local_folder() -> Vec<local_files::LocalTrack> {
    local_files::pick_folder()
}

#[tauri::command]
fn taskbar_playback_state(playing: bool) {
    let is_tr = is_tr_locale();
    taskbar::win::update_playback_state(playing, is_tr);
}

#[tauri::command]
fn get_disk_storage() -> std::collections::HashMap<String, serde_json::Value> {
    storage::read_storage_map()
}

#[tauri::command]
fn save_disk_storage(data: std::collections::HashMap<String, serde_json::Value>) {
    storage::save_storage_map(data);
}

#[tauri::command]
fn trim_working_set() {
    do_trim_working_set();
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(250));
        do_trim_working_set();
    });
}

fn do_trim_working_set() {
    #[cfg(target_os = "windows")]
    unsafe {
        #[repr(C)]
        struct PROCESSENTRY32W {
            dw_size: u32,
            cnt_usage: u32,
            th32_process_id: u32,
            th32_default_heap_id: usize,
            th32_module_id: u32,
            cnt_threads: u32,
            th32_parent_process_id: u32,
            pc_pri_class_base: i32,
            dw_flags: u32,
            sz_exe_file: [u16; 260],
        }

        unsafe extern "system" {
            fn GetCurrentProcess() -> isize;
            fn GetCurrentProcessId() -> u32;
            fn SetProcessWorkingSetSize(h_process: isize, min_size: usize, max_size: usize) -> i32;
            fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> isize;
            fn Process32FirstW(h_snapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
            fn Process32NextW(h_snapshot: isize, lppe: *mut PROCESSENTRY32W) -> i32;
            fn CloseHandle(h_object: isize) -> i32;
            fn OpenProcess(dw_desired_access: u32, b_inherit_handle: i32, dw_process_id: u32) -> isize;
        }

        // 1. Trim current Rust process (app.exe)
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);

        // 2. Discover and trim all descendant processes (msedgewebview2.exe child & grandchild processes)
        let my_pid = GetCurrentProcessId();
        let snapshot = CreateToolhelp32Snapshot(0x00000002, 0); // TH32CS_SNAPPROCESS
        if snapshot != -1 && snapshot != 0 {
            let mut entry = PROCESSENTRY32W {
                dw_size: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                cnt_usage: 0,
                th32_process_id: 0,
                th32_default_heap_id: 0,
                th32_module_id: 0,
                cnt_threads: 0,
                th32_parent_process_id: 0,
                pc_pri_class_base: 0,
                dw_flags: 0,
                sz_exe_file: [0u16; 260],
            };

            let mut all_processes = Vec::new();
            if Process32FirstW(snapshot, &mut entry) != 0 {
                loop {
                    all_processes.push((entry.th32_process_id, entry.th32_parent_process_id));
                    if Process32NextW(snapshot, &mut entry) == 0 {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);

            let mut parent_pids = std::collections::HashSet::new();
            parent_pids.insert(my_pid);
            let mut pids_to_trim = Vec::new();

            let mut added = true;
            while added {
                added = false;
                for &(pid, ppid) in &all_processes {
                    if parent_pids.contains(&ppid) && !parent_pids.contains(&pid) {
                        parent_pids.insert(pid);
                        pids_to_trim.push(pid);
                        added = true;
                    }
                }
            }

            const PROCESS_SET_QUOTA: u32 = 0x0100;
            const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
            for pid in pids_to_trim {
                let h_proc = OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION, 0, pid);
                if h_proc != 0 {
                    let _ = SetProcessWorkingSetSize(h_proc, usize::MAX, usize::MAX);
                    let _ = CloseHandle(h_proc);
                }
            }
        }
    }
}

fn is_tr_locale() -> bool {
    #[cfg(target_os = "windows")]
    unsafe {
        unsafe extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        if GetUserDefaultUILanguage() == 0x041F {
            return true;
        }
    }
    if let Ok(lang) = std::env::var("LANG") {
        if lang.to_lowercase().starts_with("tr") {
            return true;
        }
    }
    false
}

fn send_media_command(app: &tauri::AppHandle, cmd: &str) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = app.emit("media-command", cmd);
        let script = format!("window.__raserMediaCommand && window.__raserMediaCommand('{}');", cmd);
        let _ = window.eval(&script);
        return;
    }

    // Headless tray mode (WebView2 destroyed): handle media commands directly in Rust
    if let (Some(bg_queue), Some(audio_core)) = (
        app.try_state::<Arc<bg_queue::BackgroundQueue>>(),
        app.try_state::<Arc<audio_core::AudioCore>>(),
    ) {
        match cmd {
            "next" => bg_queue::spawn_headless_step(
                Arc::clone(bg_queue.inner()),
                Arc::clone(audio_core.inner()),
                1,
                false,
            ),
            "prev" => bg_queue::spawn_headless_step(
                Arc::clone(bg_queue.inner()),
                Arc::clone(audio_core.inner()),
                -1,
                false,
            ),
            "toggle" => {
                let playing = audio_core.get_state().playing;
                if let Some(track) = bg_queue.current_track() {
                    bg_queue::update_discord_for_track(&bg_queue, &track, playing);
                } else if !playing {
                    discord::clear_presence();
                }
            }
            "stop" => {
                discord::clear_presence();
            }
            _ => {}
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let audio_core = Arc::new(audio_core::AudioCore::new());
    let bg_queue = Arc::new(bg_queue::BackgroundQueue::new());
    let audio_core_global = Arc::clone(&audio_core);
    let audio_core_tray = Arc::clone(&audio_core);
    let audio_core_monitor = Arc::clone(&audio_core);
    let bg_queue_monitor = Arc::clone(&bg_queue);

    // Load persisted tray preferences on cold startup
    let boot_storage = storage::read_storage_map();
    if let Some(v) = boot_storage.get("riff_tray_on_close") {
        let enabled = v
            .as_bool()
            .or_else(|| v.as_str().map(|s| s == "true" || s == "1"))
            .unwrap_or(true);
        bg_queue.set_tray_on_close(enabled);
    }
    if let Some(v) = boot_storage.get("riff_low_ram_tray") {
        let enabled = v
            .as_bool()
            .or_else(|| v.as_str().map(|s| s != "false" && s != "0"))
            .unwrap_or(true);
        bg_queue.set_low_ram_tray(enabled);
    }

    let app = tauri::Builder::default()
        .manage(Arc::clone(&audio_core))
        .manage(Arc::clone(&bg_queue))
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        match shortcut.key {
                            Code::MediaPlayPause => {
                                let _ = audio_core_global.toggle();
                                send_media_command(app, "toggle");
                            }
                            Code::MediaTrackNext => send_media_command(app, "next"),
                            Code::MediaTrackPrevious => send_media_command(app, "prev"),
                            Code::MediaStop => {
                                let _ = audio_core_global.pause();
                                send_media_command(app, "stop");
                            }
                            _ => {}
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            minimize_window,
            toggle_maximize_window,
            toggle_fullscreen_window,
            close_window,
            exit_app,
            open_external,
            discord_rpc_update,
            discord_rpc_clear,
            check_update,
            install_update,
            pick_local_files,
            pick_local_folder,
            taskbar_playback_state,
            get_disk_storage,
            save_disk_storage,
            trim_working_set,
            sync_playback_snapshot,
            get_resume_snapshot,
            set_tray_on_close,
            set_low_ram_tray,
            rust_audio_load,
            rust_audio_play,
            rust_audio_pause,
            rust_audio_toggle,
            rust_audio_seek,
            rust_audio_set_volume,
            rust_audio_set_speed,
            rust_audio_get_state,
            rust_audio_get_frequency_data,
            get_cache_lyrics,
            save_cache_lyrics,
            get_cache_stats,
            clear_disk_cache,
            open_cache_folder
        ])
        .setup(move |app| {
            let _ = app.global_shortcut().register(Shortcut::new(None, Code::MediaPlayPause));
            let _ = app.global_shortcut().register(Shortcut::new(None, Code::MediaTrackNext));
            let _ = app.global_shortcut().register(Shortcut::new(None, Code::MediaTrackPrevious));
            let _ = app.global_shortcut().register(Shortcut::new(None, Code::MediaStop));

            // Setup System Tray
            let is_tr = is_tr_locale();
            taskbar::win::setup_taskbar_buttons(app.handle(), is_tr);
            let show_text = if is_tr { "Raser'ı Göster" } else { "Show Raser" };
            let toggle_text = if is_tr { "Oynat / Durdur" } else { "Play / Pause" };
            let next_text = if is_tr { "Sonraki Şarkı" } else { "Next Track" };
            let prev_text = if is_tr { "Önceki Şarkı" } else { "Previous Track" };
            let quit_text = if is_tr { "Çıkış" } else { "Quit" };

            let show_i = MenuItem::with_id(app, "show", show_text, true, None::<&str>)?;
            let toggle_i = MenuItem::with_id(app, "toggle_play", toggle_text, true, None::<&str>)?;
            let next_i = MenuItem::with_id(app, "next_track", next_text, true, None::<&str>)?;
            let prev_i = MenuItem::with_id(app, "prev_track", prev_text, true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", quit_text, true, None::<&str>)?;

            let tray_menu = Menu::with_items(app, &[
                &show_i,
                &toggle_i,
                &next_i,
                &prev_i,
                &quit_i,
            ])?;

            let mut tray_builder = TrayIconBuilder::new()
                .menu(&tray_menu)
                .tooltip("Raser")
                .on_menu_event(move |app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            let app_h = app.clone();
                            std::thread::spawn(move || restore_main_window(&app_h));
                        }
                        "toggle_play" => {
                            let _ = audio_core_tray.toggle();
                            send_media_command(app, "toggle");
                        }
                        "next_track" => send_media_command(app, "next"),
                        "prev_track" => send_media_command(app, "prev"),
                        "quit" => {
                            ALLOW_EXIT.store(true, Ordering::SeqCst);
                            std::process::exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle().clone();
                        let is_visible = app
                            .get_webview_window("main")
                            .and_then(|w| w.is_visible().ok())
                            .unwrap_or(false);
                        if is_visible {
                            std::thread::spawn(move || suspend_to_tray(&app));
                        } else {
                            std::thread::spawn(move || restore_main_window(&app));
                        }
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                tray_builder = tray_builder.icon(icon.clone());
            }

            let _ = tray_builder.build(app)?;

            let initial_storage = storage::read_storage_raw();
            if let Some(window) = app.get_webview_window("main") {
                let script = format!("window.__RASER_INITIAL_STORAGE__ = {};", initial_storage);
                let _ = window.eval(&script);
                let _ = window.show();
                attach_main_window_handlers(&window);
            }

            // Continuous background audio monitor: sync taskbar thumbnail play/pause,
            // notify JS on track end, or auto-advance in Rust when WebView2 is suspended to tray.
            let app_handle_audio = app.handle().clone();
            let is_tr = is_tr_locale();
            std::thread::spawn(move || {
                let mut last_playing = false;
                let mut last_ended = false;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    let state = audio_core_monitor.get_state();
                    if state.playing != last_playing {
                        last_playing = state.playing;
                        taskbar::win::update_playback_state(state.playing, is_tr);
                    }
                    if state.ended && !last_ended {
                        last_ended = true;
                        taskbar::win::update_playback_state(false, is_tr);
                        if let Some(w) = app_handle_audio.get_webview_window("main") {
                            let _ = app_handle_audio.emit("rust-audio-ended", ());
                            let _ = w.eval("window.__raserAudioEnded && window.__raserAudioEnded();");
                        } else {
                            bg_queue::spawn_headless_step(
                                Arc::clone(&bg_queue_monitor),
                                Arc::clone(&audio_core_monitor),
                                1,
                                true,
                            );
                        }
                    } else if !state.ended {
                        last_ended = false;
                    }
                }
            });

            // Periodic working set memory trim ONLY when window is hidden/minimized or suspended to tray
            let app_handle_trim = app.handle().clone();
            let bg_queue_trim = Arc::clone(&bg_queue);
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    let should_trim = if bg_queue_trim.is_headless() {
                        true
                    } else if let Some(w) = app_handle_trim.get_webview_window("main") {
                        !w.is_visible().unwrap_or(true) || w.is_minimized().unwrap_or(false)
                    } else {
                        true
                    };
                    if should_trim {
                        trim_working_set();
                    }
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_app_handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !ALLOW_EXIT.load(Ordering::SeqCst) {
                api.prevent_exit();
            }
        }
    });
}
