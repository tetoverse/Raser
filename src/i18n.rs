#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Tr,
}

impl Lang {
    pub fn as_str(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Tr => "tr",
        }
    }

    pub fn from_code(code: &str) -> Self {
        let lower = code.trim().to_ascii_lowercase();
        if lower.starts_with("tr") {
            Lang::Tr
        } else {
            Lang::En
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Tr => "Türkçe",
        }
    }
}

pub fn is_tr_locale() -> bool {
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
        if lang.to_ascii_lowercase().starts_with("tr") {
            return true;
        }
    }
    false
}

pub fn detect_os_lang() -> Lang {
    if is_tr_locale() {
        Lang::Tr
    } else {
        Lang::En
    }
}

pub fn t(lang: Lang, key: &str) -> &'static str {
    match lang {
        Lang::En => en_lookup(key),
        Lang::Tr => tr_lookup(key),
    }
}

fn en_lookup(key: &str) -> &'static str {
    match key {
        "app_name" => "Raser",
        "home" => "Home",
        "discover" => "Discover",
        "favorites" => "Favorites",
        "albums" => "Albums",
        "local_files" => "Local files",
        "history" => "History",
        "queue" => "Queue",
        "playlists" => "Playlists",
        "settings" => "Settings",
        "search_placeholder" => "Search tracks, artists, or albums...",
        "filter_placeholder" => "Filter tracks...",
        "play" => "Play",
        "pause" => "Pause",
        "next" => "Next",
        "previous" => "Previous",
        "shuffle" => "Shuffle",
        "repeat" => "Repeat",
        "repeat_off" => "Repeat off",
        "repeat_all" => "Repeat all",
        "repeat_one" => "Repeat one",
        "mute" => "Mute",
        "unmute" => "Unmute",
        "speed" => "Speed",
        "mini_player" => "Mini player",
        "fullscreen" => "Fullscreen",
        "minimize" => "Minimize",
        "maximize" => "Maximize",
        "close" => "Close",
        "no_track_playing" => "No track playing",
        "select_track" => "Select a track to listen",
        "new_playlist" => "New playlist",
        "create_playlist" => "Create playlist",
        "rename_playlist" => "Rename playlist",
        "delete_playlist" => "Delete playlist",
        "playlist" => "Playlist",
        "playlist_name" => "Playlist name",
        "description" => "Description",
        "add_to_playlist" => "Add to playlist",
        "remove_from_playlist" => "Remove from playlist",
        "add_files" => "Add files",
        "add_folder" => "Add folder",
        "clear_history" => "Clear history",
        "clear_queue" => "Clear queue",
        "clear_cache" => "Clear cache",
        "open_cache_folder" => "Open folder",
        "cache_storage" => "Audio & Lyrics Cache",
        "cache_hint" => "Clear cached streams and lyrics to free disk space.",
        "tracks" => "tracks",
        "track" => "track",
        "sort_manual" => "Manual order",
        "sort_added_desc" => "Date added (newest)",
        "sort_added_asc" => "Date added (oldest)",
        "pin_track" => "Pin to top",
        "unpin_track" => "Unpin",
        "rename_track" => "Rename track",
        "track_title" => "Title",
        "track_artist" => "Artist",
        "trim_best_part" => "Trim best part",
        "trim_desc" => "Set start and end seconds. Outside parts are skipped automatically.",
        "trim_start" => "Start (sec)",
        "trim_end" => "End (sec)",
        "save_trim" => "Save trim",
        "reset_trim" => "Reset trim",
        "save" => "Save",
        "cancel" => "Cancel",
        "create" => "Create",
        "remove" => "Remove",
        "recent" => "Recent",
        "now_playing" => "Now playing",
        "up_next" => "Up next",
        "for_you" => "For you",
        "jump_back_in" => "Jump back in",
        "empty_favorites_title" => "No favorite tracks yet",
        "empty_favorites_sub" => "Click the heart icon on any song to save it to your library",
        "empty_local_title" => "No local tracks added",
        "empty_local_sub" => "Add audio files or scan a music folder from your computer",
        "empty_history_title" => "No recently played tracks",
        "empty_history_sub" => "Songs you play will appear in your listening history",
        "empty_queue_title" => "Queue is empty",
        "empty_queue_sub" => "Play a track or add songs to queue to start listening",
        "empty_albums_title" => "No saved albums yet",
        "empty_albums_sub" => "Saved albums will appear here",
        "empty_playlist_title" => "This playlist is empty",
        "empty_playlist_sub" => "Add tracks from your library or local files",
        "discover_local_hint" => "Local & Library playback ready. Search & streaming arrive in Step 2.",
        "interface" => "Interface",
        "playback" => "Playback",
        "storage" => "Storage",
        "about" => "About",
        "interface_language" => "Interface language",
        "theme_mode" => "Theme mode",
        "dark" => "Dark",
        "light" => "Light",
        "hide_titlebar" => "Hide top titlebar",
        "hide_titlebar_desc" => "Hide the window titlebar and reveal it on top edge hover",
        "autohide_timestamps" => "Auto-hide timeline timestamps",
        "autohide_timestamps_desc" => "Fade out current time and duration on the player bar when idle",
        "classic_sidebar" => "Classic sidebar",
        "classic_sidebar_desc" => "Use the floating pill sidebar rail with rounded content panels",
        "autoplay" => "Autoplay similar tracks",
        "autoplay_desc" => "Continue playing next tracks in queue automatically",
        "sidebar_order" => "Sidebar navigation order",
        "reset" => "Reset",
        "keyboard_shortcuts" => "Keyboard shortcuts",
        "version_label" => "Version",
        "engine_label" => "UI & Audio Engine",
        "back" => "Back",
        "up" => "Up",
        "down" => "Down",
        _ => "Raser",
    }
}

fn tr_lookup(key: &str) -> &'static str {
    match key {
        "app_name" => "Raser",
        "home" => "Ana Sayfa",
        "discover" => "Ana Sayfa",
        "favorites" => "Favoriler",
        "albums" => "Albümler",
        "local_files" => "Yerel dosyalar",
        "history" => "Geçmiş",
        "queue" => "Kuyruk",
        "playlists" => "Çalma listeleri",
        "settings" => "Ayarlar",
        "search_placeholder" => "Şarkı, albüm, sanatçı ara...",
        "filter_placeholder" => "Parçalarda ara...",
        "play" => "Oynat",
        "pause" => "Duraklat",
        "next" => "Sonraki",
        "previous" => "Önceki",
        "shuffle" => "Karıştır",
        "repeat" => "Tekrarla",
        "repeat_off" => "Tekrar kapalı",
        "repeat_all" => "Tümünü tekrarla",
        "repeat_one" => "Tek parça tekrarla",
        "mute" => "Sessiz",
        "unmute" => "Sesi aç",
        "speed" => "Hız",
        "mini_player" => "Mini oynatıcı",
        "fullscreen" => "Tam ekran",
        "minimize" => "Küçült",
        "maximize" => "Büyüt",
        "close" => "Kapat",
        "no_track_playing" => "Çalan parça yok",
        "select_track" => "Dinlemek için bir parça seçin",
        "new_playlist" => "Yeni oynatma listesi",
        "create_playlist" => "Oynatma listesi oluştur",
        "rename_playlist" => "Listeyi yeniden adlandır",
        "delete_playlist" => "Çalma listesini sil",
        "playlist" => "Çalma Listesi",
        "playlist_name" => "Liste adı",
        "description" => "Açıklama",
        "add_to_playlist" => "Çalma listesine ekle",
        "remove_from_playlist" => "Oynatma listesinden kaldır",
        "add_files" => "Dosya ekle",
        "add_folder" => "Klasör ekle",
        "clear_history" => "Geçmişi temizle",
        "clear_queue" => "Kuyruğu temizle",
        "clear_cache" => "Önbelleği temizle",
        "open_cache_folder" => "Klasörü aç",
        "cache_storage" => "Ses ve Şarkı Sözü Önbelleği",
        "cache_hint" => "Disk alanı açmak için önbelleğe alınmış akışları ve sözleri temizleyin.",
        "tracks" => "şarkı",
        "track" => "parça",
        "sort_manual" => "Manuel sıra",
        "sort_added_desc" => "Eklenme tarihi (en yeni)",
        "sort_added_asc" => "Eklenme tarihi (en eski)",
        "pin_track" => "Üste sabitle",
        "unpin_track" => "Sabitlemeyi kaldır",
        "rename_track" => "Parçayı yeniden adlandır",
        "track_title" => "Başlık",
        "track_artist" => "Sanatçı",
        "trim_best_part" => "En iyi kısmı kırp",
        "trim_desc" => "Başlangıç ve bitiş saniyelerini ayarlayın. Dışındaki kısımlar otomatik atlanır.",
        "trim_start" => "Başlangıç (sn)",
        "trim_end" => "Bitiş (sn)",
        "save_trim" => "Kırpmayı kaydet",
        "reset_trim" => "Trimi sıfırla",
        "save" => "Kaydet",
        "cancel" => "İptal",
        "create" => "Oluştur",
        "remove" => "Kaldır",
        "recent" => "Son çalınanlar",
        "now_playing" => "Şimdi yürütülüyor",
        "up_next" => "Sıradaki",
        "for_you" => "Senin için",
        "jump_back_in" => "Geri atla",
        "empty_favorites_title" => "Henüz favori parça yok",
        "empty_favorites_sub" => "Kütüphanenize eklemek için herhangi bir şarkıdaki kalp simgesine tıklayın",
        "empty_local_title" => "Yerel parça eklenmedi",
        "empty_local_sub" => "Bilgisayarınızdan ses dosyası ekleyin veya müzik klasörü tarayın",
        "empty_history_title" => "Son çalınan parça yok",
        "empty_history_sub" => "Çaldığınız şarkılar dinleme geçmişinizde görünecektir",
        "empty_queue_title" => "Kuyruk boş",
        "empty_queue_sub" => "Dinlemeye başlamak için bir parça çalın veya kuyruğa ekleyin",
        "empty_albums_title" => "Henüz kayıtlı albüm yok",
        "empty_albums_sub" => "Kayıtlı albümleriniz burada görünecektir",
        "empty_playlist_title" => "Bu çalma listesi boş",
        "empty_playlist_sub" => "Kütüphanenizden veya yerel dosyalardan parça ekleyin",
        "discover_local_hint" => "Yerel ve Kütüphane oynatma hazır. Arama ve akış Adım 2'de eklenecektir.",
        "interface" => "Arayüz",
        "playback" => "Oynatma",
        "storage" => "Depolama",
        "about" => "Hakkında",
        "interface_language" => "Arayüz dili",
        "theme_mode" => "Tema modu",
        "dark" => "Koyu",
        "light" => "Açık",
        "hide_titlebar" => "Üst başlık çubuğunu gizle",
        "hide_titlebar_desc" => "Pencere başlık çubuğunu gizleyin ve üst kenarın üzerine gelindiğinde ortaya çıkarın",
        "autohide_timestamps" => "Zaman çizelgesi zaman damgalarını otomatik olarak gizle",
        "autohide_timestamps_desc" => "Boştayken oynatıcı çubuğunda geçerli saat ve sürenin kaybolması",
        "classic_sidebar" => "Klasik kenar çubuğu",
        "classic_sidebar_desc" => "Yuvarlatılmış içerik panelleriyle yüzen hap kenar çubuğunu kullanın",
        "autoplay" => "Benzer parçaları otomatik oynat",
        "autoplay_desc" => "Kuyruktaki sonraki parçaları otomatik olarak çalmaya devam et",
        "sidebar_order" => "Kenar çubuğu sıralaması",
        "reset" => "Sıfırla",
        "keyboard_shortcuts" => "Klavye kısayolları",
        "version_label" => "Sürüm",
        "engine_label" => "Arayüz ve Ses Motoru",
        "back" => "Geri",
        "up" => "Yukarı",
        "down" => "Aşağı",
        _ => "Raser",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_i18n_en_tr_coverage() {
        let keys = [
            "home",
            "favorites",
            "albums",
            "local_files",
            "history",
            "queue",
            "playlists",
            "settings",
            "play",
            "pause",
            "next",
            "previous",
            "shuffle",
            "repeat",
            "clear_cache",
            "save_trim",
            "reset_trim",
            "new_playlist",
            "add_folder",
            "hide_titlebar",
        ];
        for key in keys {
            let en = t(Lang::En, key);
            let tr = t(Lang::Tr, key);
            assert!(!en.is_empty(), "EN missing for {}", key);
            assert!(!tr.is_empty(), "TR missing for {}", key);
            assert_ne!(en, "Raser", "EN fell back to default for {}", key);
            assert_ne!(tr, "Raser", "TR fell back to default for {}", key);
        }
        assert_eq!(t(Lang::Tr, "settings"), "Ayarlar");
        assert_eq!(t(Lang::Tr, "play"), "Oynat");
        assert_eq!(t(Lang::Tr, "pause"), "Duraklat");
        assert_eq!(Lang::from_code("tr-TR"), Lang::Tr);
        assert_eq!(Lang::from_code("en-US"), Lang::En);
    }
}
