//! NMEA 0183: GPS fixes from a receiver's own log (GGA, RMC) and AIS position reports relayed
//! from other ships' transponders (AIVDM). One line is one sentence; GGA only carries a time of
//! day, so it borrows the most recent date RMC gave earlier in the same file.
//!
//! AIS position reports (message types 1–3, Class A, and 18–19, Class B) are always a single
//! NMEA sentence — long messages split across several only carry static or voyage data, which
//! has no position to plot — so multi-part AIVDM sentences are read for their message type and
//! otherwise skipped.

use std::collections::BTreeMap;

use super::{Fix, Track};
use crate::sun::{days_from_civil, DAY};

const KNOTS_TO_MPS: f64 = 0.514_444;

pub(super) fn parse(text: &str) -> Result<Vec<Track>, String> {
    let mut gps = Track {
        name: "GPS".into(),
        points: Vec::new(),
    };
    let mut last_date: Option<(i64, u32, u32)> = None;
    let mut ships: BTreeMap<u32, Vec<Fix>> = BTreeMap::new();

    for line in text.lines() {
        let Some(sentence) = line
            .trim()
            .strip_prefix('$')
            .or_else(|| line.trim().strip_prefix('!'))
        else {
            continue;
        };
        let body = sentence.split('*').next().unwrap_or(sentence);
        let fields: Vec<&str> = body.split(',').collect();
        let Some(kind) = fields.first() else { continue };
        let kind = kind.get(2..).unwrap_or("");
        match kind {
            "GGA" => {
                if let Some(fix) = parse_gga(&fields, last_date) {
                    gps.points.push(fix);
                }
            }
            "RMC" => {
                if let Some((fix, date)) = parse_rmc(&fields) {
                    last_date = Some(date);
                    gps.points.push(fix);
                }
            }
            "VDM" | "VDO" => {
                if let Some((mmsi, fix)) = parse_aivdm(&fields) {
                    ships.entry(mmsi).or_default().push(fix);
                }
            }
            _ => {}
        }
    }

    let mut tracks: Vec<Track> = Vec::new();
    if !gps.points.is_empty() {
        tracks.push(gps);
    }
    for (mmsi, points) in ships {
        tracks.push(Track {
            name: format!("AIS {mmsi}"),
            points,
        });
    }
    if tracks.is_empty() {
        return Err("no GPS or AIS fixes found in this NMEA file".into());
    }
    Ok(tracks)
}

/// $--GGA,time,lat,N/S,lon,E/W,fix,sats,hdop,alt,M,sep,M,age,station
fn parse_gga(fields: &[&str], date: Option<(i64, u32, u32)>) -> Option<Fix> {
    let lat = coord(fields.get(2)?, fields.get(3)?)?;
    let lon = coord(fields.get(4)?, fields.get(5)?)?;
    let time = fields
        .get(1)
        .and_then(|t| time_of_day(t))
        .zip(date)
        .map(|(secs, (y, m, d))| days_from_civil(y, m, d) * DAY + secs);
    let alt = fields.get(9).and_then(|a| a.parse().ok());
    Some(Fix {
        lat,
        lon,
        time,
        alt,
        speed: None,
    })
}

/// $--RMC,time,status,lat,N/S,lon,E/W,speed,track,date,magvar,E/W[,mode]
fn parse_rmc(fields: &[&str]) -> Option<(Fix, (i64, u32, u32))> {
    let lat = coord(fields.get(3)?, fields.get(4)?)?;
    let lon = coord(fields.get(5)?, fields.get(6)?)?;
    let date = ddmmyy(fields.get(9)?)?;
    let time = fields
        .get(1)
        .and_then(|t| time_of_day(t))
        .map(|secs| days_from_civil(date.0, date.1, date.2) * DAY + secs);
    let speed = fields
        .get(7)
        .and_then(|s| s.parse::<f64>().ok())
        .map(|knots| knots * KNOTS_TO_MPS);
    Some((
        Fix {
            lat,
            lon,
            time,
            alt: None,
            speed,
        },
        date,
    ))
}

/// "4807.038" (ddmm.mmmm) or "01131.000" (dddmm.mmmm), whichever width the degrees are: dividing
/// by 100 and flooring always splits off the two-digit minutes, however many digits of degrees
/// come before them.
fn coord(field: &str, hemisphere: &str) -> Option<f64> {
    if field.is_empty() {
        return None;
    }
    let raw: f64 = field.parse().ok()?;
    let deg = (raw / 100.0).floor();
    let min = raw - deg * 100.0;
    let value = deg + min / 60.0;
    match hemisphere {
        "S" | "W" => Some(-value),
        "N" | "E" => Some(value),
        _ => None,
    }
}

/// "123519" or "123519.00": hours, minutes, seconds of the UTC day.
fn time_of_day(field: &str) -> Option<i64> {
    if field.len() < 6 {
        return None;
    }
    let h: i64 = field.get(0..2)?.parse().ok()?;
    let m: i64 = field.get(2..4)?.parse().ok()?;
    let s: f64 = field.get(4..)?.parse().ok()?;
    Some(h * 3600 + m * 60 + s as i64)
}

/// "230394": day, month, two-digit year (GPS predates the century, so under 80 is 2000s).
fn ddmmyy(field: &str) -> Option<(i64, u32, u32)> {
    if field.len() != 6 {
        return None;
    }
    let d: u32 = field.get(0..2)?.parse().ok()?;
    let m: u32 = field.get(2..4)?.parse().ok()?;
    let yy: i64 = field.get(4..6)?.parse().ok()?;
    let y = if yy < 80 { 2000 + yy } else { 1900 + yy };
    Some((y, m, d))
}

/// !--VDM,fragments,fragment,seq,channel,payload,fill*checksum — only single-sentence messages
/// are decoded (see the module note); their payload is 6-bit "ASCII armored" per AIS character.
fn parse_aivdm(fields: &[&str]) -> Option<(u32, Fix)> {
    if fields.get(1).copied() != Some("1") {
        return None;
    }
    let payload = fields.get(5)?;
    let bytes: Vec<u32> = payload.bytes().map(armor).collect::<Option<_>>()?;
    decode_position(&bytes)
}

/// One armored character to its 6-bit value (ITU-R M.1371 Table 47).
fn armor(c: u8) -> Option<u32> {
    let v = c.checked_sub(48)? as u32;
    let v = if v > 40 { v.checked_sub(8)? } else { v };
    (v < 64).then_some(v)
}

/// `len` bits of the payload's bitstream starting at bit `start` (6 bits per armored
/// character, most significant bit first), as an unsigned integer.
fn bits(payload: &[u32], start: usize, len: usize) -> u64 {
    let mut value = 0u64;
    for i in 0..len {
        let bit = start + i;
        let b = payload.get(bit / 6).map_or(0, |c| (c >> (5 - bit % 6)) & 1);
        value = (value << 1) | u64::from(b);
    }
    value
}

/// A field of `width` bits as two's-complement signed.
fn signed(value: u64, width: u32) -> i64 {
    let shift = 64 - width;
    ((value << shift) as i64) >> shift
}

/// A position report's MMSI and fix: message types 1–3 (Class A) and 18–19 (Class B), the only
/// ones carrying a latitude and longitude. Speed over ground sits a little before the position
/// in both layouts (ITU-R M.1371): 10 bits, tenths of a knot, 1023 meaning "not available".
fn decode_position(payload: &[u32]) -> Option<(u32, Fix)> {
    let msg_type = bits(payload, 0, 6);
    let (sog_start, lon_start) = match msg_type {
        1..=3 => (50, 61),
        18 | 19 => (46, 57),
        _ => return None,
    };
    if payload.len() * 6 < lon_start + 28 + 27 {
        return None;
    }
    let mmsi = bits(payload, 8, 30) as u32;
    let lon = signed(bits(payload, lon_start, 28), 28) as f64 / 600_000.0;
    let lat = signed(bits(payload, lon_start + 28, 27), 27) as f64 / 600_000.0;
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let sog = bits(payload, sog_start, 10);
    let speed = (sog != 1023).then_some(sog as f64 * 0.1 * KNOTS_TO_MPS);
    Some((
        mmsi,
        Fix {
            lat,
            lon,
            time: None,
            alt: None,
            speed,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gga_and_rmc_from_the_nmea_spec_example() {
        let text = "$GPRMC,123519,A,4807.038,N,01131.000,E,022.4,084.4,230394,003.1,W*6A\n\
                     $GPGGA,123520,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,*47\n";
        let tracks = parse(text).unwrap();
        let gps = tracks.iter().find(|t| t.name == "GPS").unwrap();
        assert_eq!(gps.points.len(), 2);
        let rmc = gps.points[0];
        assert!((rmc.lat - 48.1173).abs() < 1e-3);
        assert!((rmc.lon - 11.5167).abs() < 1e-3);
        // 1994-03-23 12:35:19 UTC
        assert_eq!(rmc.time, Some(764_426_119));
        // GGA borrows RMC's date, one second later
        assert_eq!(gps.points[1].time, Some(764_426_120));
        // RMC's speed (22.4 knots) and GGA's altitude (545.4 m)
        assert!((rmc.speed.unwrap() - 22.4 * KNOTS_TO_MPS).abs() < 1e-6);
        assert_eq!(gps.points[1].alt, Some(545.4));
    }

    #[test]
    fn south_and_west_are_negative() {
        assert!((coord("4807.038", "S").unwrap() + 48.1173).abs() < 1e-4);
        assert!((coord("01131.000", "W").unwrap() + 11.5167).abs() < 1e-4);
    }

    #[test]
    fn a_class_a_position_report_decodes() {
        // A widely used AIVDM worked example (MMSI 227006760, Brest Vessel Traffic Service).
        let line = "!AIVDM,1,1,,B,15M67FC000G?ufbE`FepT@3n00Sa,0*5C\n";
        let tracks = parse(line).unwrap();
        assert_eq!(tracks.len(), 1);
        assert!(tracks[0].name.starts_with("AIS "));
        let fix = tracks[0].points[0];
        assert!((-90.0..=90.0).contains(&fix.lat));
        assert!((-180.0..=180.0).contains(&fix.lon));
    }
}
