//! The editor's text mode: one step per line, `step setting=value ...`.
//! `#` starts a comment line. Values are numbers, `true`/`false`, a bare word,
//! or a "quoted string" with `\"` and `\\` escapes.

use serde::Serialize;
use serde_json::Value;

use crate::{
    i18n::{t, tf},
    pipeline_def::Step,
    pipeline_steps::{Media, Params, Registry},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineError {
    /// 1-based.
    pub line: usize,
    pub message: String,
}

/// Every line's error, or the steps. Unknown step ids (for `media`) and
/// unknown params are errors; param values are not validated here
/// (`pipeline_plan::check` does that).
pub fn parse(media: Media, text: &str, r: &Registry) -> Result<Vec<Step>, Vec<LineError>> {
    let mut steps = Vec::new();
    let mut errors = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match parse_line(media, trimmed, r) {
            Ok(step) => steps.push(step),
            Err(message) => errors.push(LineError {
                line: index + 1,
                message,
            }),
        }
    }
    if errors.is_empty() {
        Ok(steps)
    } else {
        Err(errors)
    }
}

type Chars<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// Characters up to the next space (or `=`).
fn word(chars: &mut Chars, stop_at_equals: bool) -> String {
    let mut out = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || (stop_at_equals && c == '=') {
            break;
        }
        out.push(c);
        chars.next();
    }
    out
}

fn parse_line(media: Media, line: &str, r: &Registry) -> Result<Step, String> {
    let mut chars = line.chars().peekable();
    let id = word(&mut chars, false);
    let kind = r
        .get(media, &id)
        .ok_or_else(|| tf("pipe.error.unknown_step", &[("kind", &id)]))?;
    let mut params = Params::new();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            break;
        }
        let name = word(&mut chars, true);
        if chars.next_if_eq(&'=').is_none() {
            return Err(tf("pipe.error.expected_pair", &[("token", &name)]));
        }
        let value = if chars.next_if_eq(&'"').is_some() {
            Value::String(quoted(&mut chars)?)
        } else {
            let raw = word(&mut chars, false);
            if raw.contains('"') {
                return Err(t("pipe.error.quote"));
            }
            scalar(&raw)
        };
        if !kind.params.iter().any(|p| p.name == name) {
            return Err(tf(
                "pipe.error.unknown_param",
                &[("name", &name), ("step", &t(kind.label))],
            ));
        }
        params.insert(name, value);
    }
    Ok(Step {
        kind: id,
        params,
        ..Step::default()
    })
}

/// The rest of a quoted string, after its opening quote.
fn quoted(chars: &mut Chars) -> Result<String, String> {
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Ok(out),
            '\\' => match chars.next() {
                Some(escaped @ ('"' | '\\')) => out.push(escaped),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => break,
            },
            c => out.push(c),
        }
    }
    Err(t("pipe.error.quote"))
}

/// A bare word: a number, a boolean, or text.
fn scalar(raw: &str) -> Value {
    match raw {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => serde_json::from_str::<serde_json::Number>(raw)
            .map_or_else(|_| Value::String(raw.into()), Value::Number),
    }
}

/// One line per step, params in the kind's order and then any others in key
/// order. Strings are quoted only when they need to be.
pub fn print(steps: &[Step], media: Media, r: &Registry) -> String {
    let mut out = String::new();
    for step in steps {
        out.push_str(&step.kind);
        let known: Vec<&str> = r
            .get(media, &step.kind)
            .map(|k| k.params.iter().map(|p| p.name).collect())
            .unwrap_or_default();
        let mut names: Vec<&String> = step.params.keys().collect();
        names.sort_by_key(|n| {
            (
                known
                    .iter()
                    .position(|k| k == &n.as_str())
                    .unwrap_or(usize::MAX),
                n.as_str().to_owned(),
            )
        });
        for name in names {
            out.push(' ');
            out.push_str(name);
            out.push('=');
            out.push_str(&show(&step.params[name]));
        }
        out.push('\n');
    }
    out
}

fn show(value: &Value) -> String {
    match value {
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => {
            let needs_quotes = s.is_empty()
                || s.contains([' ', '"', '\\', '=', '#'])
                || s.contains(char::is_whitespace)
                || !matches!(scalar(s), Value::String(_));
            if needs_quotes { quote(s) } else { s.clone() }
        }
        other => quote(&other.to_string()),
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline_steps::builtin;
    use serde_json::json;

    #[test]
    fn text_round_trips_through_steps() {
        let text = "downscale by=long_edge value=1920\nconvert to=webp\ncopy to=\"~/My Pictures/%f.%e\"\nif_name_contains text=invoice\nfork\n";
        let steps = parse(Media::Image, text, &builtin()).unwrap();
        assert_eq!(steps[0].params["value"], json!(1920));
        assert_eq!(steps[2].params["to"], json!("~/My Pictures/%f.%e"));
        assert_eq!(print(&steps, Media::Image, &builtin()), text);
    }

    #[test]
    fn comments_blank_lines_and_booleans() {
        let steps = parse(
            Media::Image,
            "# web\n\ncrop aspect=1:1 smart=false\n",
            &builtin(),
        )
        .unwrap();
        assert_eq!((steps.len(), &steps[0].params["smart"]), (1, &json!(false)));
    }

    #[test]
    fn errors_name_their_line() {
        let errors = parse(
            Media::Image,
            "convert to=webp\nteleport\ncrop sharpness=3\ncopy to=\"~/a\n",
            &builtin(),
        )
        .unwrap_err();
        assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [2, 3, 4]);
        assert!(errors[0].message.contains("teleport") && errors[1].message.contains("sharpness"));
    }

    #[test]
    fn a_string_that_looks_like_a_number_is_quoted() {
        let steps = vec![Step {
            kind: "if_name_contains".into(),
            params: json!({ "text": "2026" }).as_object().unwrap().clone(),
            ..Default::default()
        }];
        assert_eq!(
            print(&steps, Media::Image, &builtin()),
            "if_name_contains text=\"2026\"\n"
        );
        let back = parse(Media::Image, "if_name_contains text=\"2026\"", &builtin()).unwrap();
        assert_eq!(back[0].params["text"], json!("2026"));
    }

    #[test]
    fn a_setting_without_a_value_and_escapes_are_handled() {
        let errors = parse(Media::Image, "convert webp", &builtin()).unwrap_err();
        assert_eq!(errors[0].line, 1);
        let steps = parse(
            Media::Image,
            r#"if_name_contains text="say \"hi\" \\ now""#,
            &builtin(),
        )
        .unwrap();
        assert_eq!(steps[0].params["text"], json!(r#"say "hi" \ now"#));
        assert_eq!(
            print(&steps, Media::Image, &builtin()),
            "if_name_contains text=\"say \\\"hi\\\" \\\\ now\"\n"
        );
    }
}
