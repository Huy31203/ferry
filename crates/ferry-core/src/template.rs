//! File-name templates: `%f` name without extension, `%e` extension, `%z` size
//! (`1920x1080`), `%y` `%m` `%d` year, month and day, `%P` parent folder name,
//! and `%%` for a literal percent sign. The Settings preview calls this too,
//! through a Tauri command, so there is one implementation.

use chrono::{Datelike, NaiveDate, NaiveDateTime};

use crate::i18n::{t, tf};

pub struct Ctx<'a> {
    pub stem: &'a str,
    pub ext: &'a str,
    pub width: u32,
    pub height: u32,
    pub parent: &'a str,
    pub date: NaiveDate,
}

enum Piece {
    Literal(char),
    Token(char),
    Unknown(String),
}

fn pieces(tpl: &str) -> impl Iterator<Item = Piece> + '_ {
    let mut chars = tpl.chars();
    std::iter::from_fn(move || {
        let c = chars.next()?;
        if c != '%' {
            return Some(Piece::Literal(c));
        }
        Some(match chars.next() {
            Some('%') => Piece::Literal('%'),
            Some(t @ ('f' | 'e' | 'z' | 'y' | 'm' | 'd' | 'P')) => Piece::Token(t),
            Some(other) => Piece::Unknown(format!("%{other}")),
            None => Piece::Unknown("%".into()),
        })
    })
}

/// Expand `tpl`. Unknown tokens are kept as typed, so a half-typed template
/// shows what is wrong in the live preview.
pub fn render(tpl: &str, ctx: &Ctx) -> String {
    let mut out = String::new();
    for piece in pieces(tpl) {
        match piece {
            Piece::Literal(c) => out.push(c),
            Piece::Unknown(text) => out.push_str(&text),
            Piece::Token(t) => match t {
                'f' => out.push_str(ctx.stem),
                'e' => out.push_str(ctx.ext),
                'z' => out.push_str(&format!("{}x{}", ctx.width, ctx.height)),
                'y' => out.push_str(&format!("{:04}", ctx.date.year())),
                'm' => out.push_str(&format!("{:02}", ctx.date.month())),
                'd' => out.push_str(&format!("{:02}", ctx.date.day())),
                _ => out.push_str(ctx.parent),
            },
        }
    }
    out
}

/// `Err` lists the unknown tokens (`%q`, a trailing `%`) once each, in order of appearance.
pub fn validate(tpl: &str) -> Result<(), Vec<String>> {
    let mut unknown: Vec<String> = Vec::new();
    for piece in pieces(tpl) {
        if let Piece::Unknown(text) = piece
            && !unknown.contains(&text)
        {
            unknown.push(text);
        }
    }
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(unknown)
    }
}

/// Make a rendered template safe as a single file name: path separators and
/// characters Windows forbids become `_`, trailing dots and spaces go, and an
/// empty result becomes `_`.
pub fn safe_file_name(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = replaced.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "_".into()
    } else {
        trimmed.into()
    }
}

/// The file name, without extension, that a clipboard image gets: `tpl` with
/// the "Clipboard <date> at <time>" stem, made safe for the file system.
pub fn clipboard_name(
    tpl: &str,
    ext: &str,
    (width, height): (u32, u32),
    now: NaiveDateTime,
) -> String {
    let date = now.format("%Y-%m-%d").to_string();
    let time = now.format("%H.%M.%S").to_string();
    let stem = tf("clipboard.name", &[("date", &date), ("time", &time)]);
    let ctx = Ctx {
        stem: &stem,
        ext,
        width,
        height,
        parent: &t("clipboard.folder"),
        date: now.date(),
    };
    safe_file_name(&render(tpl, &ctx))
}

/// The Settings window's live example: `tpl` for a 1920×1080 PNG copied now.
/// `Err` lists the unknown tokens, so the field can say what is wrong.
pub fn preview(tpl: &str) -> Result<String, Vec<String>> {
    validate(tpl)?;
    let now = chrono::Local::now().naive_local();
    Ok(format!(
        "{}.png",
        clipboard_name(tpl, "png", (1920, 1080), now)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 22)
            .unwrap()
            .and_hms_opt(14, 23, 11)
            .unwrap()
    }

    #[test]
    fn a_clipboard_image_is_named_like_a_macos_screenshot() {
        assert_eq!(
            clipboard_name("%f", "png", (1920, 1080), at()),
            "Clipboard 2026-10-22 at 14.23.11"
        );
        assert_eq!(
            clipboard_name("%P_%z", "png", (1920, 1080), at()),
            "Clipboard_1920x1080"
        );
    }

    #[test]
    fn a_clipboard_name_is_safe_as_a_file_name() {
        assert_eq!(clipboard_name("a/b", "png", (1, 1), at()), "a_b");
    }

    #[test]
    fn the_preview_adds_the_extension_and_refuses_unknown_tokens() {
        let example = preview("%f_%z").unwrap();
        assert!(example.starts_with("Clipboard "), "{example}");
        assert!(example.ends_with("_1920x1080.png"), "{example}");
        assert_eq!(preview("%f_%q"), Err(vec!["%q".to_string()]));
    }

    fn ctx() -> Ctx<'static> {
        Ctx {
            stem: "shot",
            ext: "png",
            width: 1920,
            height: 1080,
            parent: "Desktop",
            date: NaiveDate::from_ymd_opt(2026, 3, 5).unwrap(),
        }
    }

    #[test]
    fn every_token_expands() {
        assert_eq!(render("%f_%z.%e", &ctx()), "shot_1920x1080.png");
        assert_eq!(render("%y-%m-%d", &ctx()), "2026-03-05");
        assert_eq!(render("%P/%f", &ctx()), "Desktop/shot");
    }

    #[test]
    fn a_double_percent_is_a_literal_percent() {
        assert_eq!(render("100%%_%f", &ctx()), "100%_shot");
        assert_eq!(validate("100%%"), Ok(()));
    }

    #[test]
    fn unknown_tokens_stay_as_typed_when_rendering() {
        assert_eq!(render("%f_%q_%", &ctx()), "shot_%q_%");
    }

    #[test]
    fn a_plain_name_and_the_default_template_are_valid() {
        assert_eq!(validate("%f"), Ok(()));
        assert_eq!(validate("~/Pictures/twitter/%f_%z.%e"), Ok(()));
        assert_eq!(validate(""), Ok(()));
    }

    #[test]
    fn validate_lists_each_unknown_token_once_in_order() {
        assert_eq!(
            validate("%f_%q%x%q"),
            Err(vec!["%q".to_string(), "%x".to_string()])
        );
        assert_eq!(validate("name%"), Err(vec!["%".to_string()]));
    }

    #[test]
    fn non_ascii_text_passes_through() {
        assert_eq!(render("Ảnh_%f", &ctx()), "Ảnh_shot");
    }

    #[test]
    fn safe_file_name_blocks_separators_and_windows_characters() {
        assert_eq!(safe_file_name("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(safe_file_name(r"a\b:c*d?e"), "a_b_c_d_e");
        assert_eq!(safe_file_name("shot.png"), "shot.png");
        assert_eq!(safe_file_name("name. "), "name");
        assert_eq!(safe_file_name(".."), "_");
        assert_eq!(safe_file_name(""), "_");
        assert_eq!(safe_file_name("a\nb"), "a_b");
    }
}
