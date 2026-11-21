pub mod fetcher;
pub mod lrc;
pub mod rtl;

pub use fetcher::{compute_smart_intro_offset, fetch_lyrics_for_track, CachedLyricsRecord};
pub use lrc::{
    active_lyric_index, active_word_index, parse_lrc, serialize_lrc, LyricLine, LyricWord,
    ParsedLyrics,
};
pub use rtl::{is_rtl_language_code, is_rtl_text};
