//! The BaseStation ("SBS") text format ADS-B feeders such as dump1090 log and serve: one plane
//! per line, comma-separated, identified by its ICAO hex address. Only `MSG,3` (an airborne
//! position report) carries a latitude and longitude; every other message type is skipped.

use std::collections::BTreeMap;

use super::{Fix, Track};
use crate::sun::days_from_civil;

pub(super) fn parse(text: &str) -> Result<Vec<Track>, String> {
    let mut planes: BTreeMap<String, Vec<Fix>> = BTreeMap::new();
    for line in text.lines() {
        if let Some(fix) = parse_line(line) {
            let icao = line.split(',').nth(4).unwrap_or("").to_string();
            planes.entry(icao).or_default().push(fix);
        }
    }
    if planes.is_empty() {
        return Err("no airborne position reports (MSG,3) found in this SBS log".into());
    }
    Ok(planes
        .into_iter()
        .map(|(icao, points)| Track {
            name: format!("Plane {icao}"),
            points,
        })
        .collect())
}

/// Field numbers follow the published BaseStation layout, 1-based: 2 transmission type, 5 ICAO
/// hex, 7/8 the date and time the message was generated, 15 latitude, 16 longitude.
fn parse_line(line: &str) -> Option<Fix> {
    let fields: Vec<&str> = line.trim_end().split(',').collect();
    if fields.first() != Some(&"MSG") || fields.get(1) != Some(&"3") {
        return None;
    }
    let lat: f64 = fields.get(14)?.parse().ok()?;
    let lon: f64 = fields.get(15)?.parse().ok()?;
    let time = fields
        .get(6)
        .zip(fields.get(7))
        .and_then(|(d, t)| parse_date_time(d, t));
    Some(Fix { lat, lon, time })
}

/// "2017/01/01" and "00:00:00.000".
fn parse_date_time(date: &str, time: &str) -> Option<i64> {
    let mut d = date.split('/');
    let y: i64 = d.next()?.parse().ok()?;
    let m: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    let mut t = time.split(':');
    let h: i64 = t.next()?.parse().ok()?;
    let min: i64 = t.next()?.parse().ok()?;
    let s: f64 = t.next()?.parse().ok()?;
    Some(days_from_civil(y, m, day) * crate::sun::DAY + h * 3600 + min * 60 + s as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_airborne_position_and_skips_other_messages() {
        let text = "MSG,1,1,1,4CA2C6,1,2017/01/01,00:00:00.000,2017/01/01,00:00:00.000,RYR42NG,,,,,,,,,,,0\n\
                     MSG,3,1,1,4CA2C6,1,2017/01/01,00:00:01.000,2017/01/01,00:00:01.000,,38000,,,51.47000,-0.45000,,,,,,0\n";
        let tracks = parse(text).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].name, "Plane 4CA2C6");
        assert_eq!(tracks[0].points.len(), 1);
        assert_eq!(tracks[0].points[0].lat, 51.47);
        assert_eq!(tracks[0].points[0].lon, -0.45);
        assert!(tracks[0].points[0].time.is_some());
    }

    #[test]
    fn empty_log_is_an_error() {
        assert!(parse("STA,1,1,1,4CA2C6,1,,,,,,,,,,,,,,,,\n").is_err());
    }
}
