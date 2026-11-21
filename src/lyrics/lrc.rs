use super::rtl::is_rtl_text;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LyricWord {
    pub time_secs: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LyricLine {
    pub time_secs: f64,
    pub text: String,
    pub words: Vec<LyricWord>,
    pub is_instrumental: bool,
    pub is_interlude: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ParsedLyrics {
    pub offset_ms: i64,
    pub lines: Vec<LyricLine>,
    pub plain_text: String,
    pub is_instrumental: bool,
    pub is_rtl: bool,
}

fn parse_timestamp_inner(inner: &str) -> Option<f64> {
    let trimmed = inner.trim();
    let colon = trimmed.find(':')?;
    let mins: f64 = trimmed[..colon].trim().parse().ok()?;
    let rest = &trimmed[colon + 1..];
    let (sec_str, frac_secs) = if let Some(sep) = rest.find(['.', ':']) {
        let s = &rest[..sep];
        let f_raw = rest[sep + 1..].trim();
        let ms: f64 = match f_raw.len() {
            0 => 0.0,
            1 => f_raw.parse::<f64>().ok()? * 100.0,
            2 => f_raw.parse::<f64>().ok()? * 10.0,
            _ => f_raw[..3].parse::<f64>().ok()?,
        };
        (s, ms / 1000.0)
    } else {
        (rest, 0.0)
    };
    let secs: f64 = sec_str.trim().parse().ok()?;
    if !mins.is_finite() || !secs.is_finite() || mins < 0.0 || secs < 0.0 {
        return None;
    }
    Some(mins * 60.0 + secs + frac_secs)
}

fn parse_inline_words(raw_text: &str, offset_secs: f64) -> (String, Vec<LyricWord>) {
    let mut words = Vec::new();
    let mut plain = String::new();
    let mut rest = raw_text;

    while let Some(open) = rest.find('<') {
        plain.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        if let Some(close) = after.find('>') {
            let tag = &after[..close];
            let after_tag = &after[close + 1..];
            if let Some(t) = parse_timestamp_inner(tag) {
                let next_open = after_tag.find('<').unwrap_or(after_tag.len());
                let seg = &after_tag[..next_open];
                let clean_word = seg.trim().to_string();
                if !clean_word.is_empty() {
                    words.push(LyricWord {
                        time_secs: (t + offset_secs).max(0.0),
                        text: clean_word,
                    });
                }
                plain.push_str(seg);
                rest = &after_tag[next_open..];
                continue;
            }
        }
        plain.push('<');
        rest = after;
    }
    plain.push_str(rest);
    (plain.trim().to_string(), words)
}

pub fn parse_lrc(raw: &str) -> ParsedLyrics {
    let mut offset_ms: i64 = 0;
    let mut raw_entries: Vec<(f64, String, Vec<LyricWord>, bool)> = Vec::new();
    let mut is_instrumental = false;
    let mut plain_lines = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check [offset:...]
        if let Some(inner) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            let lower = inner.to_ascii_lowercase();
            if let Some(val) = lower.strip_prefix("offset:") {
                if let Ok(parsed) = val.trim().parse::<i64>() {
                    offset_ms = parsed;
                }
                continue;
            }
            if lower.contains("instrumental") || lower.contains("enstrümantal") {
                is_instrumental = true;
            }
        }

        let mut cursor = trimmed;
        let mut timestamps = Vec::new();
        while cursor.starts_with('[') {
            if let Some(end) = cursor.find(']') {
                let tag = &cursor[1..end];
                if let Some(t) = parse_timestamp_inner(tag) {
                    timestamps.push(t);
                    cursor = cursor[end + 1..].trim_start();
                    continue;
                }
            }
            break;
        }

        if timestamps.is_empty() {
            if !trimmed.starts_with('[') {
                plain_lines.push(trimmed.to_string());
            }
            continue;
        }

        let offset_secs = offset_ms as f64 / 1000.0;
        let (clean_text, words) = parse_inline_words(cursor, offset_secs);
        let lower_txt = clean_text.to_ascii_lowercase();
        let line_inst = lower_txt == "[instrumental]"
            || lower_txt == "instrumental"
            || lower_txt == "♪";
        if line_inst {
            is_instrumental = true;
        }
        if !clean_text.is_empty() {
            plain_lines.push(clean_text.clone());
        }

        for ts in timestamps {
            raw_entries.push((ts, clean_text.clone(), words.clone(), line_inst));
        }
    }

    let offset_secs = offset_ms as f64 / 1000.0;
    for entry in &mut raw_entries {
        entry.0 = (entry.0 + offset_secs).max(0.0);
    }
    raw_entries.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut lines: Vec<LyricLine> = Vec::with_capacity(raw_entries.len());
    for (idx, (ts, txt, words, line_inst)) in raw_entries.into_iter().enumerate() {
        if idx > 0 {
            let prev_ts = lines.last().map(|l| l.time_secs).unwrap_or(0.0);
            if ts - prev_ts >= 6.0 && !txt.is_empty() {
                lines.push(LyricLine {
                    time_secs: prev_ts + 2.0,
                    text: "•••".to_string(),
                    words: Vec::new(),
                    is_instrumental: false,
                    is_interlude: true,
                });
            }
        }
        if !txt.is_empty() || line_inst {
            lines.push(LyricLine {
                time_secs: ts,
                text: if txt.is_empty() {
                    "[Instrumental]".to_string()
                } else {
                    txt
                },
                words,
                is_instrumental: line_inst,
                is_interlude: false,
            });
        }
    }

    let plain_text = plain_lines.join("\n");
    let is_rtl = is_rtl_text(&plain_text);

    ParsedLyrics {
        offset_ms,
        lines,
        plain_text,
        is_instrumental,
        is_rtl,
    }
}

pub fn active_lyric_index(lines: &[LyricLine], pos_secs: f64) -> Option<usize> {
    if lines.is_empty() || !pos_secs.is_finite() {
        return None;
    }
    let mut active = None;
    for (i, line) in lines.iter().enumerate() {
        if line.time_secs <= pos_secs + 0.08 {
            active = Some(i);
        } else {
            break;
        }
    }
    active
}

pub fn active_word_index(line: &LyricLine, pos_secs: f64) -> Option<usize> {
    if line.words.is_empty() || !pos_secs.is_finite() {
        return None;
    }
    let mut active = None;
    for (i, w) in line.words.iter().enumerate() {
        if w.time_secs <= pos_secs + 0.05 {
            active = Some(i);
        } else {
            break;
        }
    }
    active
}

pub fn serialize_lrc(lines: &[LyricLine], offset_ms: i64) -> String {
    let mut out = String::new();
    if offset_ms != 0 {
        out.push_str(&format!("[offset:{}]\n", offset_ms));
    }
    for line in lines {
        if line.is_interlude {
            continue;
        }
        let ts = line.time_secs.max(0.0);
        let mins = (ts / 60.0).floor() as u64;
        let rem = ts - (mins as f64 * 60.0);
        let secs = rem.floor() as u64;
        let centis = ((rem - secs as f64) * 100.0).round().min(99.0) as u64;
        out.push_str(&format!("[{:02}:{:02}.{:02}]{}\n", mins, secs, centis, line.text));
    }
    out
}
