//! GPX: the GPS Exchange format most GPS receivers, phones and route planners export. Tracks
//! (`trk`/`trkseg`/`trkpt`) are the usual recorded path; routes (`rte`/`rtept`) and standalone
//! waypoints (`wpt`) are read too, each becoming their own track so they still show on the map.

use roxmltree::Node;

use super::{Fix, Track};

pub(super) fn parse(text: &str) -> Result<Vec<Track>, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    if root.tag_name().name() != "gpx" {
        return Err("not a GPX file: its root element isn't <gpx>".into());
    }

    let mut tracks = Vec::new();
    for trk in children(root, "trk") {
        let name = child_text(trk, "name").unwrap_or_else(|| "Track".into());
        for (i, seg) in children(trk, "trkseg").enumerate() {
            let points: Vec<Fix> = children(seg, "trkpt").filter_map(point).collect();
            if !points.is_empty() {
                let name = if i == 0 {
                    name.clone()
                } else {
                    format!("{name} ({})", i + 1)
                };
                tracks.push(Track { name, points });
            }
        }
    }
    for rte in children(root, "rte") {
        let name = child_text(rte, "name").unwrap_or_else(|| "Route".into());
        let points: Vec<Fix> = children(rte, "rtept").filter_map(point).collect();
        if !points.is_empty() {
            tracks.push(Track { name, points });
        }
    }
    let waypoints: Vec<Fix> = children(root, "wpt").filter_map(point).collect();
    if !waypoints.is_empty() {
        tracks.push(Track {
            name: "Waypoints".into(),
            points: waypoints,
        });
    }

    if tracks.is_empty() {
        return Err("this GPX file has no tracks, routes or waypoints with a position".into());
    }
    Ok(tracks)
}

fn children<'a, 'i>(node: Node<'a, 'i>, name: &'a str) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children()
        .filter(move |n| n.is_element() && n.tag_name().name() == name)
}

fn child_text(node: Node, name: &str) -> Option<String> {
    children(node, name)
        .next()
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A `trkpt`, `rtept` or `wpt`: its `lat`/`lon` attributes and, when it has them, its `time`
/// and `ele` (elevation, metres). GPX has no speed of its own; `fill_missing_speeds` works one
/// out from consecutive timed fixes once the whole track is read.
fn point(node: Node) -> Option<Fix> {
    let lat: f64 = node.attribute("lat")?.parse().ok()?;
    let lon: f64 = node.attribute("lon")?.parse().ok()?;
    let time = child_text(node, "time").and_then(|t| super::parse_iso8601(&t));
    let alt = child_text(node, "ele").and_then(|e| e.parse().ok());
    Some(Fix {
        lat,
        lon,
        time,
        alt,
        speed: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<gpx version="1.1">
  <trk>
    <name>Morning walk</name>
    <trkseg>
      <trkpt lat="48.8584" lon="2.2945"><ele>35.2</ele><time>2024-05-01T08:00:00Z</time></trkpt>
      <trkpt lat="48.8600" lon="2.2950"><time>2024-05-01T08:05:30Z</time></trkpt>
    </trkseg>
  </trk>
  <wpt lat="48.0" lon="2.0"><name>Start</name></wpt>
</gpx>"#;

    #[test]
    fn reads_a_track_and_a_waypoint() {
        let tracks = parse(SAMPLE).unwrap();
        let walk = tracks.iter().find(|t| t.name == "Morning walk").unwrap();
        assert_eq!(walk.points.len(), 2);
        assert_eq!(walk.points[0].lat, 48.8584);
        assert_eq!(walk.points[0].alt, Some(35.2));
        assert!(walk.points[0].time.is_some());
        let later = walk.points[1].time.unwrap() - walk.points[0].time.unwrap();
        assert_eq!(later, 5 * 60 + 30);
        assert!(tracks.iter().any(|t| t.name == "Waypoints"));
    }

    #[test]
    fn rejects_a_non_gpx_file() {
        assert!(parse("<kml></kml>").is_err());
    }
}
