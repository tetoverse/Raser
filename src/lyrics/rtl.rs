pub fn is_rtl_language_code(code: &str) -> bool {
    let lower = code.trim().to_ascii_lowercase();
    let prefix = lower
        .split(['-', '_'])
        .next()
        .unwrap_or("");
    matches!(
        prefix,
        "ar" | "fa" | "per" | "fas" | "prs" | "pes" | "he" | "iw" | "ur" | "ps" | "ckb" | "sd"
            | "ug" | "yi"
    )
}

pub fn is_rtl_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0590..=0x05FF
            | 0x0600..=0x06FF
            | 0x0700..=0x074F
            | 0x0750..=0x077F
            | 0x0780..=0x07BF
            | 0x08A0..=0x08FF
            | 0xFB50..=0xFDFF
            | 0xFE70..=0xFEFF
    )
}

pub fn is_rtl_text(text: &str) -> bool {
    let mut rtl_count = 0usize;
    let mut ltr_count = 0usize;
    for ch in text.chars() {
        if is_rtl_char(ch) {
            rtl_count += 1;
        } else if ch.is_alphabetic() {
            ltr_count += 1;
        }
    }
    rtl_count > 0 && rtl_count * 2 >= ltr_count
}
