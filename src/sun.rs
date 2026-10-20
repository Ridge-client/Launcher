//! Chronolocation: when a picture was taken, from where the sun stood. Shadows give the sun's
//! direction (a shadow points away from it) and its height (an object `h` tall casts a shadow
//! `h / tan(elevation)` long on level ground); the sun's position for a place is known for any
//! moment, so the moments that fit both are the candidates.
//!
//! The sun's position uses NOAA's solar calculator equations (Meeus), good to about 0.01° between
//! 1800 and 2100, far finer than anything measured on a picture.

/// Seconds in a day.
pub const DAY: i64 = 86_400;

/// Where the sun stands, in degrees: azimuth clockwise from true north, elevation above the
/// horizon (with atmospheric refraction, as the eye and the camera see it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SunPos {
    pub azimuth: f64,
    pub elevation: f64,
}

/// The sun's declination (degrees) and the equation of time (minutes) at a moment: what changes
/// from day to day, rather than with the hour.
fn orbit(unix: f64) -> (f64, f64) {
    let t = (unix / DAY as f64 + 2_440_587.5 - 2_451_545.0) / 36_525.0;
    let l0 = (280.46646 + t * (36_000.769_83 + t * 0.000_303_2)).rem_euclid(360.0);
    let m = 357.52911 + t * (35_999.050_29 - 0.000_153_7 * t);
    let e = 0.016_708_634 - t * (0.000_042_037 + 0.000_000_126_7 * t);
    let mr = m.to_radians();
    let c = mr.sin() * (1.914_602 - t * (0.004_817 + 0.000_014 * t))
        + (2.0 * mr).sin() * (0.019_993 - 0.000_101 * t)
        + (3.0 * mr).sin() * 0.000_289;
    let omega = (125.04 - 1934.136 * t).to_radians();
    let lambda = (l0 + c - 0.005_69 - 0.004_78 * omega.sin()).to_radians();
    let eps0 =
        23.0 + (26.0 + (21.448 - t * (46.815 + t * (0.000_59 - t * 0.001_813))) / 60.0) / 60.0;
    let eps = (eps0 + 0.002_56 * omega.cos()).to_radians();
    let declination = (eps.sin() * lambda.sin()).asin().to_degrees();
    let y = (eps / 2.0).tan().powi(2);
    let l0r = l0.to_radians();
    let eot = y * (2.0 * l0r).sin() - 2.0 * e * mr.sin()
        + 4.0 * e * y * mr.sin() * (2.0 * l0r).cos()
        - 0.5 * y * y * (4.0 * l0r).sin()
        - 1.25 * e * e * (2.0 * mr).sin();
    (declination, 4.0 * eot.to_degrees())
}

/// The sun at `unix` seconds (UTC) seen from `lat`, `lon` (degrees, east positive), for a day's
/// `declination` and equation of time `eot`.
fn position_with(unix: f64, lat: f64, lon: f64, (declination, eot): (f64, f64)) -> SunPos {
    let minutes = unix.rem_euclid(DAY as f64) / 60.0;
    let solar_time = minutes + eot + 4.0 * lon;
    let hour_angle = (solar_time / 4.0 - 180.0).to_radians();
    let (lat, decl) = (lat.to_radians(), declination.to_radians());
    let cos_zenith =
        (lat.sin() * decl.sin() + lat.cos() * decl.cos() * hour_angle.cos()).clamp(-1.0, 1.0);
    let elevation = 90.0 - cos_zenith.acos().to_degrees();
    let azimuth = (hour_angle.sin())
        .atan2(hour_angle.cos() * lat.sin() - decl.tan() * lat.cos())
        .to_degrees()
        + 180.0;
    SunPos {
        azimuth: azimuth.rem_euclid(360.0),
        elevation: elevation + refraction(elevation),
    }
}

/// The sun at `unix` seconds (UTC) seen from `lat`, `lon`.
pub fn position(unix: f64, lat: f64, lon: f64) -> SunPos {
    position_with(unix, lat, lon, orbit(unix))
}

/// How much the air lifts the sun at true elevation `e` (degrees), after NOAA.
fn refraction(e: f64) -> f64 {
    if e > 85.0 {
        return 0.0;
    }
    let te = e.to_radians().tan();
    let arcsec = if e > 5.0 {
        58.1 / te - 0.07 / te.powi(3) + 0.000_086 / te.powi(5)
    } else if e > -0.575 {
        1735.0 + e * (-518.2 + e * (103.4 + e * (-12.79 + e * 0.711)))
    } else {
        -20.774 / te
    };
    arcsec / 3600.0
}

/// The sun's elevation (degrees) for an object `height` tall casting a shadow `shadow` long.
pub fn elevation_from_shadow(height: f64, shadow: f64) -> f64 {
    height.atan2(shadow).to_degrees()
}

/// `b - a` wrapped to -180..180 degrees.
pub fn angle_diff(a: f64, b: f64) -> f64 {
    (b - a + 180.0).rem_euclid(360.0) - 180.0
}

/// The 16-point compass name of a bearing.
pub fn compass(bearing: f64) -> &'static str {
    const NAMES: [&str; 16] = [
        "N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW",
        "NW", "NNW",
    ];
    NAMES[((bearing.rem_euclid(360.0) / 22.5).round() as usize) % 16]
}

// ---- calendar ----

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a day number from `days_from_civil`.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (
        if m <= 2 {
            yoe + era * 400 + 1
        } else {
            yoe + era * 400
        },
        m,
        d,
    )
}

/// 0 for Sunday … 6 for Saturday.
fn weekday(days: i64) -> i64 {
    (days + 4).rem_euclid(7)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    let next = if m == 12 {
        days_from_civil(y + 1, 1, 1)
    } else {
        days_from_civil(y, m + 1, 1)
    };
    (next - days_from_civil(y, m, 1)) as u32
}

/// A date typed as 2026-04-15, 15/04/2026 or 15.04.2026, as a day number.
pub fn parse_date(text: &str) -> Option<i64> {
    let parts: Vec<i64> = text
        .trim()
        .split(['-', '/', '.'])
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;
    let [a, b, c] = parts[..] else {
        return None;
    };
    // year first (ISO) or last (day/month/year, as written in Europe)
    let (y, m, d) = if text.trim().find(['-', '/', '.']) == Some(4) {
        (a, b, c)
    } else {
        (c, b, a)
    };
    if !(1800..=2200).contains(&y) || !(1..=12).contains(&m) {
        return None;
    }
    let (m, d) = (m as u32, u32::try_from(d).ok()?);
    (1..=days_in_month(y, m))
        .contains(&d)
        .then(|| days_from_civil(y, m, d))
}

/// 2026-04-15
pub fn format_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// 15 Apr 2026
pub fn format_date_long(days: i64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let (y, m, d) = civil_from_days(days);
    format!("{d} {} {y}", MONTHS[m as usize - 1])
}

/// 14:05 for seconds since midnight.
pub fn format_clock(secs: i64) -> String {
    let secs = secs.rem_euclid(DAY);
    format!("{:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

/// "+2", "-5", "+5:30"
pub fn format_offset(secs: i64) -> String {
    let sign = if secs < 0 { '-' } else { '+' };
    let (h, m) = (secs.abs() / 3600, secs.abs() % 3600 / 60);
    if m == 0 {
        format!("{sign}{h}")
    } else {
        format!("{sign}{h}:{m:02}")
    }
}

// ---- local time ----

/// When the clocks go forward an hour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SummerTime {
    /// Never (or set the offset by hand).
    None,
    /// The EU and the UK: last Sunday of March to last Sunday of October, at 01:00 UTC.
    #[default]
    Europe,
    /// The US and Canada: second Sunday of March to first Sunday of November, at 02:00 local.
    NorthAmerica,
}

impl SummerTime {
    pub const ALL: [SummerTime; 3] = [Self::None, Self::Europe, Self::NorthAmerica];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No summer time",
            Self::Europe => "Europe (last Sun Mar – last Sun Oct)",
            Self::NorthAmerica => "North America (2nd Sun Mar – 1st Sun Nov)",
        }
    }
}

/// The `n`th (from 1) or, for `n` = 0, the last Sunday of a month.
fn sunday(y: i64, m: u32, n: i64) -> i64 {
    if n == 0 {
        let last = days_from_civil(y, m, days_in_month(y, m));
        last - weekday(last)
    } else {
        let first = days_from_civil(y, m, 1);
        first + (7 - weekday(first)) % 7 + 7 * (n - 1)
    }
}

/// The local clock's offset from UTC (seconds) at `unix`, for standard offset `base` (seconds).
pub fn utc_offset(unix: i64, base: i64, summer: SummerTime) -> i64 {
    let (y, ..) = civil_from_days(unix.div_euclid(DAY));
    let (start, end) = match summer {
        SummerTime::None => return base,
        SummerTime::Europe => (sunday(y, 3, 0) * DAY + 3600, sunday(y, 10, 0) * DAY + 3600),
        SummerTime::NorthAmerica => (
            sunday(y, 3, 2) * DAY + 2 * 3600 - base,
            sunday(y, 11, 1) * DAY + 2 * 3600 - base - 3600,
        ),
    };
    if (start..end).contains(&unix) {
        base + 3600
    } else {
        base
    }
}

// ---- solving ----

/// What a picture shows about the sun, and how far off each measurement may be.
#[derive(Debug, Clone, PartialEq)]
pub struct Clues {
    pub lat: f64,
    pub lon: f64,
    /// The sun's azimuth (the shadows' compass bearing + 180°), if the shadows' direction is known.
    pub azimuth: Option<f64>,
    /// The sun's elevation, if the shadows' length is known.
    pub elevation: Option<f64>,
    /// ± degrees of each.
    pub azimuth_tolerance: f64,
    pub elevation_tolerance: f64,
    /// The local clock: standard offset from UTC (seconds) and summer time.
    pub utc_offset: i64,
    pub summer: SummerTime,
    /// The local dates to search, as day numbers, first to last.
    pub days: (i64, i64),
}

/// A moment that fits the clues.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Match {
    /// Unix seconds (UTC).
    pub unix: i64,
    /// The local clock's offset from UTC then (seconds).
    pub offset: i64,
    pub sun: SunPos,
    /// How far from the clues, in tolerances: 0 is exact, 1 at the edge of what is accepted.
    pub miss: f64,
}

impl Match {
    pub fn local_day(&self) -> i64 {
        (self.unix + self.offset).div_euclid(DAY)
    }

    /// Seconds since local midnight.
    pub fn local_clock(&self) -> i64 {
        (self.unix + self.offset).rem_euclid(DAY)
    }
}

/// Consecutive days on which the sun fits at about the same time.
#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub first_day: i64,
    pub last_day: i64,
    /// Earliest and latest local clock time (seconds since midnight) among its days.
    pub earliest: i64,
    pub latest: i64,
    /// The closest fit.
    pub best: Match,
}

/// The longest search, in days.
pub const MAX_DAYS: i64 = 3 * 366;
/// Step of the first pass over each day, in seconds.
const STEP: i64 = 120;

impl Clues {
    /// How far the sun at `sun` is from the clues, in tolerances (see `Match::miss`).
    fn miss(&self, sun: SunPos) -> f64 {
        let mut sum = 0.0;
        if let Some(az) = self.azimuth {
            sum += (angle_diff(az, sun.azimuth) / self.azimuth_tolerance.max(0.01)).powi(2);
        }
        if let Some(el) = self.elevation {
            sum += ((sun.elevation - el) / self.elevation_tolerance.max(0.01)).powi(2);
        }
        sum.sqrt()
    }

    /// How far the sun at `unix` is from the clues; never a fit while the sun is down.
    fn miss_at(&self, unix: f64) -> (f64, SunPos) {
        let sun = position(unix, self.lat, self.lon);
        let miss = if sun.elevation <= 0.0 {
            f64::INFINITY
        } else {
            self.miss(sun)
        };
        (miss, sun)
    }

    /// Every moment that fits, in time order: one per fitting stretch of each day (usually one or
    /// two a day), at its closest fit.
    pub fn solve(&self) -> Vec<Match> {
        let mut found = Vec::new();
        if self.azimuth.is_none() && self.elevation.is_none() {
            return found;
        }
        let (first, last) = (self.days.0, self.days.1.min(self.days.0 + MAX_DAYS - 1));
        for day in first..=last {
            // local midnight, with the offset of that day's noon
            let noon = day * DAY + DAY / 2;
            let offset = utc_offset(noon - self.utc_offset, self.utc_offset, self.summer);
            let start = day * DAY - offset;
            // first pass: the day's orbit is enough to find where the fits are
            let orbit = orbit((start + DAY / 2) as f64);
            let misses: Vec<f64> = (0..=DAY / STEP)
                .map(|i| {
                    let sun = position_with((start + i * STEP) as f64, self.lat, self.lon, orbit);
                    if sun.elevation <= 0.0 {
                        f64::INFINITY
                    } else {
                        self.miss(sun)
                    }
                })
                .collect();
            for i in 0..misses.len() {
                let here = misses[i];
                let before = if i > 0 { misses[i - 1] } else { f64::INFINITY };
                let after = misses.get(i + 1).copied().unwrap_or(f64::INFINITY);
                // a local minimum near enough to be worth refining (the first pass is rough)
                if !(here.is_finite() && here <= before && here < after && here < 2.0) {
                    continue;
                }
                let t = start + i as i64 * STEP;
                let m = self.refine((t - STEP) as f64, (t + STEP) as f64);
                let in_day = (start..start + DAY).contains(&m.unix);
                if m.miss <= 1.0 && in_day && found.last().is_none_or(|p: &Match| p.unix != m.unix)
                {
                    found.push(m);
                }
            }
        }
        found
    }

    /// The closest fit between `a` and `b` (seconds), to the second.
    fn refine(&self, mut a: f64, mut b: f64) -> Match {
        while b - a > 1.0 {
            let (m1, m2) = (a + (b - a) / 3.0, b - (b - a) / 3.0);
            if self.miss_at(m1).0 <= self.miss_at(m2).0 {
                b = m2;
            } else {
                a = m1;
            }
        }
        let unix = ((a + b) / 2.0).round();
        let (miss, sun) = self.miss_at(unix);
        let unix = unix as i64;
        Match {
            unix,
            offset: utc_offset(unix, self.utc_offset, self.summer),
            sun,
            miss,
        }
    }
}

/// Fits grouped into windows: a fit joins the window it continues from the day before at about
/// the same time (within an hour), so a spring and an autumn date, or a morning and an
/// afternoon, stay apart.
pub fn windows(matches: &[Match]) -> Vec<Window> {
    let mut out: Vec<Window> = Vec::new();
    // per window, the clock time of its last day's fit
    let mut last_clock: Vec<i64> = Vec::new();
    for m in matches {
        let (day, clock) = (m.local_day(), m.local_clock());
        let joined = out.iter().zip(&last_clock).position(|(w, c)| {
            (w.last_day == day - 1 || w.last_day == day) && (clock - c).abs() <= 3600
        });
        match joined {
            Some(i) => {
                let w = &mut out[i];
                w.last_day = day;
                w.earliest = w.earliest.min(clock);
                w.latest = w.latest.max(clock);
                if m.miss < w.best.miss {
                    w.best = *m;
                }
                last_clock[i] = clock;
            }
            None => {
                out.push(Window {
                    first_day: day,
                    last_day: day,
                    earliest: clock,
                    latest: clock,
                    best: *m,
                });
                last_clock.push(clock);
            }
        }
    }
    out
}

/// For a search over one whole year standing for any year: a window running into the last day
/// searched joins the one at the same time of day from the first day, so a fit across New Year
/// (around the December solstice) is one window. Its last day then counts on into the next year.
pub fn wrap_year(mut windows: Vec<Window>, (first, last): (i64, i64)) -> Vec<Window> {
    let near = |a: &Window, b: &Window| (a.best.local_clock() - b.best.local_clock()).abs() <= 3600;
    let mut i = 0;
    while i < windows.len() {
        let joins = windows[i].last_day == last;
        let next = joins
            .then(|| {
                windows.iter().position(|w| {
                    w.first_day == first && w.last_day != last && near(w, &windows[i])
                })
            })
            .flatten();
        if let Some(j) = next {
            let start = windows.remove(j);
            let i = if j < i { i - 1 } else { i };
            let w = &mut windows[i];
            w.last_day = start.last_day + (last - first + 1);
            w.earliest = w.earliest.min(start.earliest);
            w.latest = w.latest.max(start.latest);
            if start.best.miss < w.best.miss {
                w.best = start.best;
            }
        }
        i += 1;
    }
    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARSEILLE: (f64, f64) = (43.3026, 5.3806);

    fn unix(y: i64, m: u32, d: u32, h: i64, min: i64) -> i64 {
        days_from_civil(y, m, d) * DAY + h * 3600 + min * 60
    }

    #[test]
    fn calendar_round_trips() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        for z in [-800_000, -1, 0, 59, 11_016, 20_000, 30_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(weekday(days_from_civil(2026, 10, 20)), 2); // a Tuesday
        assert_eq!(parse_date("2026-04-15"), Some(days_from_civil(2026, 4, 15)));
        assert_eq!(parse_date("15/04/2026"), Some(days_from_civil(2026, 4, 15)));
        assert_eq!(parse_date("2026-02-30"), None);
        assert_eq!(parse_date("tomorrow"), None);
        assert_eq!(format_date(days_from_civil(2026, 4, 5)), "2026-04-05");
        assert_eq!(format_date_long(days_from_civil(2026, 4, 5)), "5 Apr 2026");
    }

    #[test]
    fn summer_time_switches_on_the_right_sundays() {
        let paris = 3600;
        // 2026: 29 March and 25 October, at 01:00 UTC
        assert_eq!(
            utc_offset(unix(2026, 3, 29, 0, 59), paris, SummerTime::Europe),
            3600
        );
        assert_eq!(
            utc_offset(unix(2026, 3, 29, 1, 0), paris, SummerTime::Europe),
            7200
        );
        assert_eq!(
            utc_offset(unix(2026, 10, 25, 0, 59), paris, SummerTime::Europe),
            7200
        );
        assert_eq!(
            utc_offset(unix(2026, 10, 25, 1, 0), paris, SummerTime::Europe),
            3600
        );
        // New York 2026: 8 March 07:00 UTC and 1 November 06:00 UTC
        let ny = -5 * 3600;
        let us = SummerTime::NorthAmerica;
        assert_eq!(utc_offset(unix(2026, 3, 8, 6, 59), ny, us), ny);
        assert_eq!(utc_offset(unix(2026, 3, 8, 7, 0), ny, us), ny + 3600);
        assert_eq!(utc_offset(unix(2026, 11, 1, 5, 59), ny, us), ny + 3600);
        assert_eq!(utc_offset(unix(2026, 11, 1, 6, 0), ny, us), ny);
        assert_eq!(
            utc_offset(unix(2026, 7, 1, 12, 0), ny, SummerTime::None),
            ny
        );
    }

    #[test]
    fn sun_matches_known_positions() {
        // Marseille 2026-06-21 12:00 UTC, by hand from declination 23.44° and hour angle 4.9°
        let s = position(unix(2026, 6, 21, 12, 0) as f64, MARSEILLE.0, MARSEILLE.1);
        assert!((s.azimuth - 193.2).abs() < 0.5, "{s:?}");
        assert!((s.elevation - 69.7).abs() < 0.3, "{s:?}");
        // the equator at the March equinox's solar noon: the sun is overhead, almost
        let t = unix(2026, 3, 20, 12, 7) as f64;
        assert!(position(t, 0.0, 0.0).elevation > 89.0);
        // at the June solstice's solar noon, 90° - latitude + 23.44°
        let noon = unix(2026, 6, 21, 12, 2) as f64 - 4.0 * 60.0 * MARSEILLE.1;
        let s = position(noon, MARSEILLE.0, MARSEILLE.1);
        assert!(
            (s.elevation - (90.0 - MARSEILLE.0 + 23.44)).abs() < 0.2,
            "{s:?}"
        );
        assert!(angle_diff(s.azimuth, 180.0).abs() < 1.0, "{s:?}");
        // morning sun in the east, evening in the west
        let morning = position(unix(2026, 6, 21, 5, 0) as f64, MARSEILLE.0, MARSEILLE.1);
        assert!((50.0..100.0).contains(&morning.azimuth), "{morning:?}");
        let evening = position(unix(2026, 6, 21, 18, 0) as f64, MARSEILLE.0, MARSEILLE.1);
        assert!((260.0..310.0).contains(&evening.azimuth), "{evening:?}");
    }

    #[test]
    fn shadows_give_elevation_and_direction() {
        assert!((elevation_from_shadow(1.0, 1.0) - 45.0).abs() < 1e-9);
        assert!((elevation_from_shadow(1.7, 2.944) - 30.0).abs() < 0.01);
        assert!((angle_diff(350.0, 10.0) - 20.0).abs() < 1e-9);
        assert!((angle_diff(10.0, 350.0) + 20.0).abs() < 1e-9);
        assert_eq!(compass(0.0), "N");
        assert_eq!(compass(214.0), "SW");
        assert_eq!(compass(355.0), "N");
    }

    fn clues_for(sun: SunPos, days: (i64, i64)) -> Clues {
        Clues {
            lat: MARSEILLE.0,
            lon: MARSEILLE.1,
            azimuth: Some(sun.azimuth),
            elevation: Some(sun.elevation),
            azimuth_tolerance: 2.0,
            elevation_tolerance: 1.0,
            utc_offset: 3600,
            summer: SummerTime::Europe,
            days,
        }
    }

    #[test]
    fn finds_the_time_on_a_known_day() {
        let taken = unix(2026, 5, 2, 9, 37); // 11:37 in Marseille (summer time)
        let sun = position(taken as f64, MARSEILLE.0, MARSEILLE.1);
        let day = days_from_civil(2026, 5, 2);
        let found = clues_for(sun, (day, day)).solve();
        assert_eq!(found.len(), 1, "{found:?}");
        assert!((found[0].unix - taken).abs() <= 30, "{found:?}");
        assert_eq!(found[0].offset, 7200);
        assert_eq!(format_clock(found[0].local_clock()), "11:37");
        // the direction alone is enough on a known day
        let mut by_direction = clues_for(sun, (day, day));
        by_direction.elevation = None;
        let found = by_direction.solve();
        assert_eq!(found.len(), 1, "{found:?}");
        assert!((found[0].unix - taken).abs() <= 60, "{found:?}");
    }

    #[test]
    fn finds_the_date_and_its_mirror_over_a_year() {
        let taken = unix(2026, 4, 18, 12, 10);
        let sun = position(taken as f64, MARSEILLE.0, MARSEILLE.1);
        let year = (days_from_civil(2026, 1, 1), days_from_civil(2026, 12, 31));
        let found = clues_for(sun, year).solve();
        let windows = windows(&found);
        // the sun stands the same way in April and in late August
        assert_eq!(windows.len(), 2, "{windows:#?}");
        let april = &windows[0];
        assert!(
            (april.first_day..=april.last_day).contains(&days_from_civil(2026, 4, 18)),
            "{april:#?}"
        );
        let august = &windows[1];
        let (_, m, _) = civil_from_days(august.best.local_day());
        assert!(m == 8 || m == 9, "{august:#?}");
        // and nowhere else: no fit in winter
        assert!(found.iter().all(|m| {
            let (_, month, _) = civil_from_days(m.local_day());
            (3..=9).contains(&month)
        }));
    }

    #[test]
    fn a_fit_across_new_year_is_one_window() {
        let taken = unix(2026, 12, 21, 11, 40);
        let sun = position(taken as f64, MARSEILLE.0, MARSEILLE.1);
        let year = (days_from_civil(2026, 1, 1), days_from_civil(2026, 12, 31));
        let split = windows(&clues_for(sun, year).solve());
        assert_eq!(split.len(), 2, "{split:#?}");
        let joined = wrap_year(split, year);
        assert_eq!(joined.len(), 1, "{joined:#?}");
        let w = &joined[0];
        assert!(w.first_day < days_from_civil(2026, 12, 21), "{w:#?}");
        assert!(w.last_day > days_from_civil(2026, 12, 31), "{w:#?}");
    }

    #[test]
    fn nothing_fits_an_impossible_sun() {
        // a sun 80° high never happens at 43° N
        let clues = Clues {
            elevation: Some(80.0),
            azimuth: None,
            ..clues_for(
                SunPos {
                    azimuth: 0.0,
                    elevation: 0.0,
                },
                (days_from_civil(2026, 1, 1), days_from_civil(2026, 12, 31)),
            )
        };
        assert!(clues.solve().is_empty());
    }
}
