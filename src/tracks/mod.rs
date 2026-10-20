//! Reading track files for the Map tab: NMEA 0183 (GPS fixes and AIS position reports), GPX,
//! KML/KMZ and the BaseStation (SBS) text format dump1090-style ADS-B feeders log to. Each
//! returns the same shape regardless of where the points came from, so the Map tab can draw a
//! plane's, boat's or car's path the same way whichever file it was read from.

mod gpx;
mod kml;
mod nmea;
mod sbs;

use std::path::Path;

use crate::sun::{days_from_civil, DAY};

/// One fix: where, and when the file says so.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    pub lat: f64,
    pub lon: f64,
    /// Unix seconds, when the format and the file both give a time.
    pub time: Option<i64>,
}

/// One path through a file: a GPX track, an AIS ship, an ADS-B aircraft, a KML placemark. A
/// single fix (no path to draw, just a point) is a track of one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Track {
    pub name: String,
    pub points: Vec<Fix>,
}

/// Reads a track file, guessing its format from its extension and, failing that, from its
/// content, so a renamed or extension-less file still opens.
pub fn load(path: &Path) -> Result<Vec<Track>, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("gpx") => gpx::parse(&read_text(path)?),
        Some("kml") => kml::parse(&read_text(path)?),
        Some("kmz") => kml::parse_kmz(&std::fs::read(path).map_err(|e| e.to_string())?),
        Some("nmea" | "log" | "txt" | "ais") => sniff_text(&read_text(path)?),
        _ => {
            if let Ok(bytes) = std::fs::read(path) {
                if bytes.starts_with(b"PK\x03\x04") {
                    return kml::parse_kmz(&bytes);
                }
                if let Ok(text) = String::from_utf8(bytes) {
                    return sniff_text(&text);
                }
            }
            Err("unrecognised file: expected .nmea, .gpx, .kml, .kmz or an ADS-B SBS log".into())
        }
    }
}

fn read_text(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

/// "2024-05-01T08:00:00Z" or "...+02:00", with optional fractional seconds, as GPX and KML
/// both give a point's time.
fn parse_iso8601(text: &str) -> Option<i64> {
    let text = text.trim();
    let (date, rest) = text.split_once('T')?;
    let mut d = date.split('-');
    let y: i64 = d.next()?.parse().ok()?;
    let m: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;

    let (time, offset) = match rest.strip_suffix('Z') {
        Some(time) => (time, 0),
        None => split_offset(rest),
    };
    let mut t = time.split(':');
    let h: i64 = t.next()?.parse().ok()?;
    let min: i64 = t.next()?.parse().ok()?;
    let s: f64 = t.next()?.parse().ok()?;
    let secs = h * 3600 + min * 60 + s as i64;
    Some(days_from_civil(y, m, day) * DAY + secs - offset)
}

/// A time ending in "+02:00" or "-05:30": that offset in seconds, and the time before it. No
/// suffix at all (a bare local time with no zone) is taken as UTC.
fn split_offset(time: &str) -> (&str, i64) {
    if time.len() < 6 {
        return (time, 0);
    }
    let (head, tail) = time.split_at(time.len() - 6);
    let bytes = tail.as_bytes();
    if matches!(bytes[0], b'+' | b'-') && bytes[3] == b':' {
        let sign = if bytes[0] == b'-' { -1 } else { 1 };
        let oh: i64 = tail.get(1..3).and_then(|s| s.parse().ok()).unwrap_or(0);
        let om: i64 = tail.get(4..6).and_then(|s| s.parse().ok()).unwrap_or(0);
        return (head, sign * (oh * 3600 + om * 60));
    }
    (time, 0)
}

/// A file without a format-specific extension: GPX and KML are XML and say which they are in
/// their root element; everything else is line-based, told apart by its first non-blank line.
fn sniff_text(text: &str) -> Result<Vec<Track>, String> {
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let trimmed = first.trim_start();
    if trimmed.starts_with("<?xml") || trimmed.starts_with('<') {
        if text.contains("<gpx") {
            return gpx::parse(text);
        }
        if text.contains("<kml") {
            return kml::parse(text);
        }
        return Err("this XML file is neither GPX nor KML".into());
    }
    if trimmed.starts_with('$') || trimmed.starts_with('!') {
        return nmea::parse(text);
    }
    if trimmed.starts_with("MSG,") || trimmed.starts_with("STA,") || trimmed.starts_with("ID,") {
        return sbs::parse(text);
    }
    Err("unrecognised track file: expected NMEA 0183, GPX, KML or an ADS-B SBS log".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_with_z_and_with_an_offset_agree() {
        assert_eq!(parse_iso8601("2024-05-01T08:00:00Z"), Some(1_714_550_400));
        assert_eq!(
            parse_iso8601("2024-05-01T10:00:00+02:00"),
            parse_iso8601("2024-05-01T08:00:00Z")
        );
    }
}
