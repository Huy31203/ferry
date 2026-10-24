//! Loudness targets and the two `loudnorm` passes: measure, then apply the
//! measurement in linear mode (pure gain, dynamics untouched while the peak
//! allows it).

use std::{collections::HashMap, ffi::OsString, path::Path};

use super::Loudness;
use crate::i18n::tf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    /// Integrated loudness, LUFS.
    pub i: f32,
    /// True peak, dBTP.
    pub tp: f32,
    /// Loudness range, LU.
    pub lra: f32,
    /// A mono file played on two speakers sounds 3 dB louder than measured.
    pub dual_mono: bool,
}

/// Pass 1 is mapped to this share of the progress bar, pass 2 to the rest.
pub const MEASURE_SHARE: f32 = 0.3;

/// `None` for Off. LRA 20 keeps music in linear mode; Podcast's 11 levels
/// speech gently. Broadcast (EBU R 128) measures mono as mono.
pub fn spec(target: Loudness, channels: u16) -> Option<Spec> {
    let (i, lra) = match target {
        Loudness::Off => return None,
        Loudness::Streaming => (-14.0, 20.0),
        Loudness::AppleMusic => (-16.0, 20.0),
        Loudness::Podcast => (-16.0, 11.0),
        Loudness::Broadcast => (-23.0, 20.0),
    };
    Some(Spec {
        i,
        tp: -1.0,
        lra,
        dual_mono: channels == 1 && target != Loudness::Broadcast,
    })
}

/// "−14 LUFS", with a true minus sign.
pub fn label(spec: &Spec) -> String {
    tf(
        "audio.detail.lufs",
        &[("n", &spec.i.to_string().replace('-', "−"))],
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    pub i: f64,
    pub tp: f64,
    pub lra: f64,
    pub thresh: f64,
    pub offset: f64,
}

impl Measured {
    /// Digital silence and clips under 0.4 s report `-inf`.
    pub fn is_measurable(&self) -> bool {
        self.i.is_finite() && self.i > -70.0 && self.thresh.is_finite() && self.tp.is_finite()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoudnessError {
    NoReport,
}

fn target(spec: &Spec) -> String {
    format!("I={:.1}:TP={:.1}:LRA={:.1}", spec.i, spec.tp, spec.lra)
}

/// Pass 1: decode the first audio stream into the filter, write nothing.
pub fn measure_args(input: &Path, spec: &Spec) -> Vec<OsString> {
    let dual = if spec.dual_mono {
        ":dual_mono=true"
    } else {
        ""
    };
    let filter = format!("loudnorm={}{dual}:print_format=json", target(spec));
    let mut args = vec![OsString::from("-i"), input.as_os_str().to_owned()];
    args.extend(["-map", "0:a:0", "-af"].map(OsString::from));
    args.push(filter.into());
    args.extend(["-f", "null", "-"].map(OsString::from));
    args
}

/// The `{…}` block `loudnorm` prints after its last log line; every value is a string.
fn report(log: &str) -> Option<HashMap<String, String>> {
    let after = &log[log.rfind("[Parsed_loudnorm_")?..];
    let start = after.find('{')?;
    let end = start + after[start..].find('}')?;
    serde_json::from_str(&after[start..=end]).ok()
}

fn number(text: &str) -> Option<f64> {
    match text {
        "-inf" => Some(f64::NEG_INFINITY),
        "inf" => Some(f64::INFINITY),
        _ => text.parse().ok(),
    }
}

pub fn parse_measurement(log: &str) -> Result<Measured, LoudnessError> {
    let report = report(log).ok_or(LoudnessError::NoReport)?;
    let get = |key: &str| {
        report
            .get(key)
            .and_then(|v| number(v))
            .ok_or(LoudnessError::NoReport)
    };
    Ok(Measured {
        i: get("input_i")?,
        tp: get("input_tp")?,
        lra: get("input_lra")?,
        thresh: get("input_thresh")?,
        offset: get("target_offset")?,
    })
}

/// Pass 2's `-af` value. `measured_LRA` is never exactly 0, which `loudnorm`
/// takes for "not measured".
pub fn filter(spec: &Spec, m: &Measured) -> String {
    let dual = if spec.dual_mono {
        ":dual_mono=true"
    } else {
        ""
    };
    format!(
        "loudnorm={}:measured_I={:.2}:measured_TP={:.2}:measured_LRA={:.2}:measured_thresh={:.2}:offset={:.2}:linear=true{dual}:print_format=json",
        target(spec),
        m.i,
        m.tp,
        m.lra.max(0.01),
        m.thresh,
        m.offset,
    )
}

/// Pass 2 had to fall back to the limiter (`dynamic`) to hold the true peak.
pub fn was_limited(log: &str) -> bool {
    report(log).is_some_and(|r| r.get("normalization_type").is_some_and(|t| t == "dynamic"))
}

#[cfg(test)]
pub(crate) mod samples {
    pub const MEASURED: &str = "size=N/A time=00:00:30.00 bitrate=N/A speed= 412x
[Parsed_loudnorm_0 @ 0x600000f0c000] 
{
\t\"input_i\" : \"-27.61\",
\t\"input_tp\" : \"-4.47\",
\t\"input_lra\" : \"18.06\",
\t\"input_thresh\" : \"-39.20\",
\t\"output_i\" : \"-16.58\",
\t\"output_tp\" : \"-1.50\",
\t\"output_lra\" : \"14.78\",
\t\"output_thresh\" : \"-27.71\",
\t\"normalization_type\" : \"dynamic\",
\t\"target_offset\" : \"0.58\"
}
";
    pub const SILENT: &str = "[Parsed_loudnorm_0 @ 0x600000f0c000] 
{
\t\"input_i\" : \"-inf\",
\t\"input_tp\" : \"-inf\",
\t\"input_lra\" : \"0.00\",
\t\"input_thresh\" : \"-inf\",
\t\"output_i\" : \"-inf\",
\t\"output_tp\" : \"-inf\",
\t\"output_lra\" : \"0.00\",
\t\"output_thresh\" : \"-inf\",
\t\"normalization_type\" : \"dynamic\",
\t\"target_offset\" : \"inf\"
}
";
    pub const PASS2_LINEAR: &str = "[Parsed_loudnorm_0 @ 0x600000f0c000] 
{
\t\"input_i\" : \"-27.61\",
\t\"input_tp\" : \"-4.47\",
\t\"input_lra\" : \"18.06\",
\t\"input_thresh\" : \"-39.20\",
\t\"output_i\" : \"-14.02\",
\t\"output_tp\" : \"-1.00\",
\t\"output_lra\" : \"17.90\",
\t\"output_thresh\" : \"-24.60\",
\t\"normalization_type\" : \"linear\",
\t\"target_offset\" : \"0.02\"
}
";
    pub const PASS2_DYNAMIC: &str = "[Parsed_loudnorm_0 @ 0x600000f0c000] 
{
\t\"input_i\" : \"-27.61\",
\t\"input_tp\" : \"-4.47\",
\t\"input_lra\" : \"18.06\",
\t\"input_thresh\" : \"-39.20\",
\t\"output_i\" : \"-14.30\",
\t\"output_tp\" : \"-1.00\",
\t\"output_lra\" : \"12.10\",
\t\"output_thresh\" : \"-24.90\",
\t\"normalization_type\" : \"dynamic\",
\t\"target_offset\" : \"0.30\"
}
";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_target_matches_its_standard() {
        let s = |t| spec(t, 2).unwrap();
        assert_eq!(
            s(Loudness::Streaming),
            Spec {
                i: -14.0,
                tp: -1.0,
                lra: 20.0,
                dual_mono: false
            }
        );
        assert_eq!(
            s(Loudness::AppleMusic),
            Spec {
                i: -16.0,
                tp: -1.0,
                lra: 20.0,
                dual_mono: false
            }
        );
        assert_eq!(
            s(Loudness::Podcast),
            Spec {
                i: -16.0,
                tp: -1.0,
                lra: 11.0,
                dual_mono: false
            }
        );
        assert_eq!(
            s(Loudness::Broadcast),
            Spec {
                i: -23.0,
                tp: -1.0,
                lra: 20.0,
                dual_mono: false
            }
        );
        assert_eq!(spec(Loudness::Off, 2), None);
        assert!(
            spec(Loudness::Podcast, 1).unwrap().dual_mono
                && !spec(Loudness::Broadcast, 1).unwrap().dual_mono
        );
        assert_eq!(label(&s(Loudness::Streaming)), "−14 LUFS");
    }

    #[test]
    fn pass_one_measures_the_first_audio_stream() {
        let a: Vec<String> =
            measure_args(Path::new("/b/x.wav"), &spec(Loudness::Podcast, 1).unwrap())
                .iter()
                .map(|s| s.to_string_lossy().into())
                .collect();
        assert_eq!(
            a,
            [
                "-i",
                "/b/x.wav",
                "-map",
                "0:a:0",
                "-af",
                "loudnorm=I=-16.0:TP=-1.0:LRA=11.0:dual_mono=true:print_format=json",
                "-f",
                "null",
                "-"
            ]
        );
    }

    #[test]
    fn the_json_after_the_last_loudnorm_line_is_read() {
        let m = parse_measurement(samples::MEASURED).unwrap();
        assert_eq!(
            (m.i, m.tp, m.lra, m.thresh, m.offset),
            (-27.61, -4.47, 18.06, -39.20, 0.58)
        );
        assert!(m.is_measurable());
        assert_eq!(
            parse_measurement("no json here"),
            Err(LoudnessError::NoReport)
        );
    }

    #[test]
    fn silence_is_not_measurable() {
        let m = parse_measurement(samples::SILENT).unwrap();
        assert!(m.i.is_infinite() && !m.is_measurable());
    }

    #[test]
    fn pass_two_feeds_the_measurement_back_in_linear_mode() {
        let m = parse_measurement(samples::MEASURED).unwrap();
        assert_eq!(
            filter(&spec(Loudness::Streaming, 2).unwrap(), &m),
            "loudnorm=I=-14.0:TP=-1.0:LRA=20.0:measured_I=-27.61:measured_TP=-4.47:measured_LRA=18.06:measured_thresh=-39.20:offset=0.58:linear=true:print_format=json"
        );
        let steady = Measured { lra: 0.0, ..m };
        assert!(
            filter(&spec(Loudness::Streaming, 2).unwrap(), &steady).contains("measured_LRA=0.01")
        );
    }

    #[test]
    fn a_dynamic_second_pass_was_limited() {
        assert!(
            was_limited(samples::PASS2_DYNAMIC)
                && !was_limited(samples::PASS2_LINEAR)
                && !was_limited("")
        );
    }
}
