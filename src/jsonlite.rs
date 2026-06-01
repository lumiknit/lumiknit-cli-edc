/// Encode a string as a JSON string literal, escaping all non-ASCII
/// as \uXXXX (or surrogate pairs for codepoints above U+FFFF).
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c if c.is_ascii() => out.push(c),
            c => {
                let u = c as u32;
                if u <= 0xFFFF {
                    out.push_str(&format!("\\u{:04x}", u));
                } else {
                    let u = u - 0x10000;
                    let high = 0xD800 + (u >> 10);
                    let low = 0xDC00 + (u & 0x3FF);
                    out.push_str(&format!("\\u{:04x}\\u{:04x}", high, low));
                }
            }
        }
    }
    out.push('"');
    out
}

/// Encode a string as a JSON string literal, keeping printable UTF-8
/// characters as-is. Only escapes control characters, `"`, and `\`.
pub fn quote_readable(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0C' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Decode a JSON string literal. Accepts input with or without
/// surrounding double-quotes.
pub fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        &s[1..s.len() - 1]
    } else {
        s
    };

    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    loop {
        match chars.next() {
            None => break,
            Some('\\') => match chars.next() {
                None => break,
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('/') => result.push('/'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('b') => result.push('\x08'),
                Some('f') => result.push('\x0C'),
                Some('u') => {
                    let hex: String =
                        (0..4).filter_map(|_| chars.next()).collect();
                    if let Ok(hi) = u16::from_str_radix(&hex, 16) {
                        if (0xD800..=0xDBFF).contains(&hi) {
                            // high surrogate — consume low surrogate
                            let low = if chars.next() == Some('\\')
                                && chars.next() == Some('u')
                            {
                                let h2: String = (0..4)
                                    .filter_map(|_| chars.next())
                                    .collect();
                                u16::from_str_radix(&h2, 16).unwrap_or(0)
                            } else {
                                0
                            };
                            if (0xDC00..=0xDFFF).contains(&low) {
                                let cp = 0x10000u32
                                    + ((hi as u32 - 0xD800) << 10)
                                    + (low as u32 - 0xDC00);
                                if let Some(c) = char::from_u32(cp) {
                                    result.push(c);
                                }
                            }
                        } else if let Some(c) = char::from_u32(hi as u32) {
                            result.push(c);
                        }
                    }
                }
                Some(c) => result.push(c),
            },
            Some(c) => result.push(c),
        }
    }
    result
}
