//! Overpass: asking OpenStreetMap's Overpass API for places (police stations, masts, cameras…)
//! and reading what comes back into points and lines to show on a map.

use std::collections::{BTreeMap, HashMap};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};

pub const ENDPOINT: &str = "https://overpass-api.de/api/interpreter";

/// A latitude and longitude.
pub type Coord = (f64, f64);

/// One element of the answer that has something to say: a node, a way or a relation.
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub kind: &'static str,
    pub id: i64,
    pub tags: BTreeMap<String, String>,
    /// Where it is, for a node (or for a way or relation answered with just its centre).
    pub point: Option<Coord>,
    /// Its outline or path, for a way or relation.
    pub lines: Vec<Vec<Coord>>,
}

impl Feature {
    /// What to call it in a list: its name, else its main tag.
    pub fn title(&self) -> String {
        for key in ["name", "name:en", "ref", "operator", "brand"] {
            if let Some(v) = self.tags.get(key) {
                return v.clone();
            }
        }
        self.tags
            .iter()
            .find(|(k, _)| !k.starts_with("source"))
            .map_or_else(
                || format!("{} {}", self.kind, self.id),
                |(k, v)| format!("{k}={v}"),
            )
    }

    /// A coordinate to centre the map on.
    pub fn anchor(&self) -> Option<Coord> {
        self.point
            .or_else(|| self.lines.iter().flatten().next().copied())
    }

    /// Every coordinate it covers.
    pub fn coords(&self) -> impl Iterator<Item = Coord> + '_ {
        self.point
            .into_iter()
            .chain(self.lines.iter().flatten().copied())
    }
}

/// The query to send: `{{bbox}}` and `{{center}}` filled in from the map's view
/// ((south, west, north, east) and (latitude, longitude)), and JSON asked for.
pub fn prepare(query: &str, bbox: (f64, f64, f64, f64), center: Coord) -> String {
    let mut q = query
        .replace(
            "{{bbox}}",
            &format!("{:.6},{:.6},{:.6},{:.6}", bbox.0, bbox.1, bbox.2, bbox.3),
        )
        .replace("{{center}}", &format!("{:.6},{:.6}", center.0, center.1));
    if q.contains("[out:xml]") {
        q = q.replace("[out:xml]", "[out:json]");
    } else if !q.contains("[out:") {
        q = format!("[out:json][timeout:60];\n{q}");
    }
    q
}

fn kind_of(e: &Value) -> &str {
    e.get("type").and_then(Value::as_str).unwrap_or("")
}

fn coord_of(v: &Value) -> Option<Coord> {
    Some((v.get("lat")?.as_f64()?, v.get("lon")?.as_f64()?))
}

fn geometry_of(v: &Value) -> Option<Vec<Coord>> {
    let line: Vec<Coord> = v
        .get("geometry")?
        .as_array()?
        .iter()
        .filter_map(coord_of)
        .collect();
    (!line.is_empty()).then_some(line)
}

/// Reads an Overpass JSON answer into its features.
pub fn parse(text: &str) -> Result<Vec<Feature>> {
    let root: Value = serde_json::from_str(text)
        .map_err(|_| anyhow!("Overpass didn't answer with JSON (is the query `[out:json]`?)"))?;
    let elements = root
        .get("elements")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("Overpass answered without any elements"))?;
    if elements.is_empty() {
        if let Some(remark) = root.get("remark").and_then(Value::as_str) {
            bail!("{remark}");
        }
    }

    let id_of = |e: &Value| e.get("id").and_then(Value::as_i64).unwrap_or(0);
    let nodes: HashMap<i64, Coord> = elements
        .iter()
        .filter(|e| kind_of(e) == "node")
        .filter_map(|e| Some((id_of(e), coord_of(e)?)))
        .collect();
    let ways: HashMap<i64, Vec<Coord>> = elements
        .iter()
        .filter(|e| kind_of(e) == "way")
        .filter_map(|e| {
            let line = geometry_of(e).or_else(|| {
                let line: Vec<Coord> = e
                    .get("nodes")?
                    .as_array()?
                    .iter()
                    .filter_map(|n| nodes.get(&n.as_i64()?).copied())
                    .collect();
                (!line.is_empty()).then_some(line)
            })?;
            Some((id_of(e), line))
        })
        .collect();

    let tags_of = |e: &Value| -> BTreeMap<String, String> {
        e.get("tags")
            .and_then(Value::as_object)
            .map(|t| {
                t.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut features = Vec::new();
    for e in elements {
        let tags = tags_of(e);
        if tags.is_empty() {
            continue;
        }
        let kind = match kind_of(e) {
            "node" => "node",
            "way" => "way",
            "relation" => "relation",
            _ => continue,
        };
        let id = id_of(e);
        let (mut point, mut lines) = (None, Vec::new());
        match kind {
            "node" => point = coord_of(e),
            "way" => {
                if let Some(line) = ways.get(&id) {
                    lines.push(line.clone());
                }
            }
            _ => {
                for member in e
                    .get("members")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let mid = member.get("ref").and_then(Value::as_i64).unwrap_or(0);
                    match member.get("type").and_then(Value::as_str) {
                        Some("way") => {
                            if let Some(line) =
                                geometry_of(member).or_else(|| ways.get(&mid).cloned())
                            {
                                lines.push(line);
                            }
                        }
                        Some("node") if point.is_none() && lines.is_empty() => {
                            point = nodes.get(&mid).copied();
                        }
                        _ => {}
                    }
                }
                if !lines.is_empty() {
                    point = None;
                }
            }
        }
        // `out center` gives a way or relation a middle instead of an outline
        if point.is_none() && lines.is_empty() {
            point = e.get("center").and_then(coord_of);
        }
        if point.is_some() || !lines.is_empty() {
            features.push(Feature {
                kind,
                id,
                tags,
                point,
                lines,
            });
        }
    }
    // a query that returns bare nodes (`out skel`): show them rather than nothing
    if features.is_empty() {
        features = nodes
            .iter()
            .map(|(id, c)| Feature {
                kind: "node",
                id: *id,
                tags: BTreeMap::new(),
                point: Some(*c),
                lines: Vec::new(),
            })
            .collect();
        features.sort_by_key(|f| f.id);
    }
    Ok(features)
}

/// The features as a GeoJSON FeatureCollection (coordinates are longitude, latitude).
pub fn geojson(features: &[Feature]) -> Value {
    let pos = |c: &Coord| json!([c.1, c.0]);
    let features: Vec<Value> = features
        .iter()
        .map(|f| {
            let geometry = match (&f.point, f.lines.as_slice()) {
                (Some(p), []) => json!({"type": "Point", "coordinates": pos(p)}),
                (_, [line]) => {
                    json!({"type": "LineString", "coordinates": line.iter().map(pos).collect::<Vec<_>>()})
                }
                (_, lines) => json!({
                    "type": "MultiLineString",
                    "coordinates": lines.iter().map(|l| l.iter().map(pos).collect::<Vec<_>>()).collect::<Vec<_>>(),
                }),
            };
            let mut properties = serde_json::Map::new();
            properties.insert("@id".into(), json!(format!("{}/{}", f.kind, f.id)));
            for (k, v) in &f.tags {
                properties.insert(k.clone(), json!(v));
            }
            json!({"type": "Feature", "geometry": geometry, "properties": properties})
        })
        .collect();
    json!({"type": "FeatureCollection", "features": features})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_query_gets_the_view_and_json() {
        let q = prepare(
            "node[amenity=police]({{bbox}});out;",
            (1.0, 2.0, 3.0, 4.0),
            (2.0, 3.0),
        );
        assert!(q.starts_with("[out:json]"));
        assert!(q.contains("(1.000000,2.000000,3.000000,4.000000)"));
        let q = prepare(
            "[out:json];node(around:5,{{center}});out;",
            (0.0, 0.0, 0.0, 0.0),
            (2.5, 3.5),
        );
        assert!(q.contains("around:5,2.500000,3.500000"));
    }

    #[test]
    fn ways_are_built_from_their_nodes() {
        let text = r#"{"elements":[
            {"type":"node","id":1,"lat":1.0,"lon":2.0,"tags":{"amenity":"police","name":"Post"}},
            {"type":"node","id":2,"lat":1.0,"lon":3.0},
            {"type":"node","id":3,"lat":2.0,"lon":3.0},
            {"type":"way","id":10,"nodes":[2,3],"tags":{"landuse":"military"}},
            {"type":"relation","id":20,"tags":{"type":"multipolygon"},
             "members":[{"type":"way","ref":10,"role":"outer"}]}
        ]}"#;
        let f = parse(text).unwrap();
        assert_eq!(f.len(), 3);
        assert_eq!(f[0].title(), "Post");
        assert_eq!(f[0].point, Some((1.0, 2.0)));
        assert_eq!(f[1].lines, vec![vec![(1.0, 3.0), (2.0, 3.0)]]);
        assert_eq!(f[2].lines.len(), 1);
        let g = geojson(&f);
        assert_eq!(g["features"].as_array().unwrap().len(), 3);
        assert_eq!(
            g["features"][0]["geometry"]["coordinates"],
            json!([2.0, 1.0])
        );
    }

    #[test]
    fn a_failed_run_says_why() {
        let e = parse(r#"{"elements":[],"remark":"runtime error: Query timed out"}"#).unwrap_err();
        assert!(e.to_string().contains("timed out"));
        assert!(parse("<html>").is_err());
    }

    #[test]
    fn centres_stand_in_for_outlines() {
        let text = r#"{"elements":[{"type":"way","id":5,"center":{"lat":4.0,"lon":5.0},"tags":{"man_made":"tower"}}]}"#;
        let f = parse(text).unwrap();
        assert_eq!(f[0].anchor(), Some((4.0, 5.0)));
    }
}
