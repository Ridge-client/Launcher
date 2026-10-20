//! KML (Google Earth) and KMZ (a KML file zipped, optionally with images it references). A
//! Placemark's Point, LineString or Polygon becomes a track; so does a `gx:Track`, the
//! extension Google Earth itself records a timed path with. Namespace prefixes (`gx:`, `kml:`)
//! are ignored throughout: roxmltree already strips them from the names matched below.

use std::io::{Cursor, Read};

use roxmltree::Node;

use super::{Fix, Track};

pub(super) fn parse(text: &str) -> Result<Vec<Track>, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    if doc.root_element().tag_name().name() != "kml" {
        return Err("not a KML file: its root element isn't <kml>".into());
    }

    let mut tracks = Vec::new();
    for placemark in doc.descendants().filter(|n| n.has_tag_name("Placemark")) {
        let name = child_text(placemark, "name").unwrap_or_else(|| "Placemark".into());
        let mut found = placemark_tracks(placemark, &name);
        tracks.append(&mut found);
    }
    if tracks.is_empty() {
        return Err("this KML file has no Placemark with a position".into());
    }
    Ok(tracks)
}

pub(super) fn parse_kmz(bytes: &[u8]) -> Result<Vec<Track>, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let index = (0..archive.len())
        .find(|&i| {
            archive
                .by_index(i)
                .is_ok_and(|f| f.name().eq_ignore_ascii_case("doc.kml"))
        })
        .or_else(|| {
            (0..archive.len()).find(|&i| {
                archive
                    .by_index(i)
                    .is_ok_and(|f| f.name().to_ascii_lowercase().ends_with(".kml"))
            })
        })
        .ok_or("this KMZ file has no .kml file inside")?;
    let mut text = String::new();
    archive
        .by_index(index)
        .map_err(|e| e.to_string())?
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    parse(&text)
}

/// Every track a Placemark's geometry gives: usually one, but `MultiGeometry` can hold several.
fn placemark_tracks(placemark: Node, name: &str) -> Vec<Track> {
    let mut tracks = Vec::new();
    collect_geometry(placemark, name, &mut tracks);
    tracks
}

fn collect_geometry(node: Node, name: &str, out: &mut Vec<Track>) {
    for child in node.children().filter(|n| n.is_element()) {
        match child.tag_name().name() {
            "Point" => push(out, name, coordinates_points(child)),
            "LineString" => push(out, name, coordinates_points(child)),
            "LinearRing" => push(out, name, coordinates_points(child)),
            "Polygon" => {
                for ring in child.descendants().filter(|n| n.has_tag_name("LinearRing")) {
                    push(out, name, coordinates_points(ring));
                }
            }
            "MultiGeometry" => collect_geometry(child, name, out),
            "Track" => push(out, name, gx_track_points(child)),
            _ => {}
        }
    }
}

fn push(out: &mut Vec<Track>, name: &str, points: Vec<Fix>) {
    if !points.is_empty() {
        let name = if out.iter().any(|t: &Track| t.name == name) {
            format!("{name} ({})", out.len() + 1)
        } else {
            name.to_string()
        };
        out.push(Track { name, points });
    }
}

/// A `<coordinates>` child: "lon,lat[,alt]" tuples separated by whitespace.
fn coordinates_points(node: Node) -> Vec<Fix> {
    let Some(text) = child_text(node, "coordinates") else {
        return Vec::new();
    };
    text.split_whitespace().filter_map(tuple).collect()
}

fn tuple(tuple: &str) -> Option<Fix> {
    let mut parts = tuple.split(',');
    let lon: f64 = parts.next()?.parse().ok()?;
    let lat: f64 = parts.next()?.parse().ok()?;
    Some(Fix {
        lat,
        lon,
        time: None,
    })
}

/// A `gx:Track`: alternating `<when>` and `<gx:coord>` children, "lon lat alt" space-separated.
fn gx_track_points(node: Node) -> Vec<Fix> {
    let whens: Vec<i64> = children(node, "when")
        .filter_map(|n| n.text())
        .filter_map(super::parse_iso8601)
        .collect();
    children(node, "coord")
        .filter_map(|n| n.text())
        .filter_map(|t| {
            let mut parts = t.split_whitespace();
            let lon: f64 = parts.next()?.parse().ok()?;
            let lat: f64 = parts.next()?.parse().ok()?;
            Some((lat, lon))
        })
        .enumerate()
        .map(|(i, (lat, lon))| Fix {
            lat,
            lon,
            time: whens.get(i).copied(),
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_linestring_placemark() {
        let text = r#"<?xml version="1.0"?>
<kml xmlns="http://www.earth.google.com/kml/2.2">
  <Document>
    <Placemark>
      <name>Flight</name>
      <LineString>
        <coordinates>2.55,49.01,0 -73.78,40.64,0</coordinates>
      </LineString>
    </Placemark>
  </Document>
</kml>"#;
        let tracks = parse(text).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].name, "Flight");
        assert_eq!(tracks[0].points.len(), 2);
        assert_eq!(tracks[0].points[0].lon, 2.55);
        assert_eq!(tracks[0].points[0].lat, 49.01);
    }

    #[test]
    fn reads_a_gx_track_with_times() {
        let text = r#"<?xml version="1.0"?>
<kml xmlns:gx="http://www.google.com/kml/ext/2.2">
  <Placemark>
    <name>Drive</name>
    <gx:Track>
      <when>2024-05-01T08:00:00Z</when>
      <gx:coord>2.0 48.0 0</gx:coord>
      <when>2024-05-01T08:01:00Z</when>
      <gx:coord>2.1 48.1 0</gx:coord>
    </gx:Track>
  </Placemark>
</kml>"#;
        let tracks = parse(text).unwrap();
        assert_eq!(tracks[0].points.len(), 2);
        assert_eq!(
            tracks[0].points[1].time.unwrap() - tracks[0].points[0].time.unwrap(),
            60
        );
    }

    #[test]
    fn rejects_a_non_kml_file() {
        assert!(parse("<gpx></gpx>").is_err());
    }
}
