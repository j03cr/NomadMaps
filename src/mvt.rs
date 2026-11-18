use crate::style::{
    classify_area, classify_road, AreaKind, BoundaryKind, LabelKind, RoadKind,
};
use prost::Message;
use std::collections::HashSet;

#[derive(Clone, PartialEq, Message)]
pub struct ProtoTile {
    #[prost(message, repeated, tag = "3")]
    pub layers: Vec<ProtoLayer>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoLayer {
    #[prost(uint32, optional, tag = "15")]
    pub version: Option<u32>,
    #[prost(string, optional, tag = "1")]
    pub name: Option<String>,
    #[prost(message, repeated, tag = "2")]
    pub features: Vec<ProtoFeature>,
    #[prost(string, repeated, tag = "3")]
    pub keys: Vec<String>,
    #[prost(message, repeated, tag = "4")]
    pub values: Vec<ProtoValue>,
    #[prost(uint32, optional, tag = "5")]
    pub extent: Option<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoFeature {
    #[prost(uint64, optional, tag = "1")]
    pub id: Option<u64>,
    #[prost(uint32, repeated, tag = "2")]
    pub tags: Vec<u32>,
    #[prost(int32, optional, tag = "3")]
    pub geom_type: Option<i32>,
    #[prost(uint32, repeated, tag = "4")]
    pub geometry: Vec<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ProtoValue {
    #[prost(string, optional, tag = "1")]
    pub string_value: Option<String>,
    #[prost(float, optional, tag = "2")]
    pub float_value: Option<f32>,
    #[prost(double, optional, tag = "3")]
    pub double_value: Option<f64>,
    #[prost(int64, optional, tag = "4")]
    pub int_value: Option<i64>,
    #[prost(uint64, optional, tag = "5")]
    pub uint_value: Option<u64>,
    #[prost(sint64, optional, tag = "6")]
    pub sint_value: Option<i64>,
    #[prost(bool, optional, tag = "7")]
    pub bool_value: Option<bool>,
}

impl ProtoValue {
    fn as_str(&self) -> Option<&str> {
        self.string_value.as_deref()
    }

    fn as_f64(&self) -> Option<f64> {
        if let Some(v) = self.double_value {
            Some(v)
        } else if let Some(v) = self.float_value {
            Some(v as f64)
        } else if let Some(v) = self.int_value {
            Some(v as f64)
        } else if let Some(v) = self.uint_value {
            Some(v as f64)
        } else {
            self.sint_value.map(|v| v as f64)
        }
    }

    fn as_bool(&self) -> Option<bool> {
        self.bool_value
    }
}

#[derive(Debug, Clone)]
pub struct DecodedPolygon {
    pub kind: AreaKind,
    pub vertices: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct DecodedLine {
    pub kind: RoadKind,
    pub is_bridge: bool,
    pub is_tunnel: bool,
    pub points: Vec<[f32; 2]>,
}

#[derive(Debug, Clone)]
pub struct DecodedBoundary {
    pub kind: BoundaryKind,
    pub points: Vec<[f32; 2]>,
}

#[derive(Debug, Clone)]
pub struct DecodedLabel {
    pub kind: LabelKind,
    pub text: String,
    pub pos: [f32; 2],
    pub priority: i32,
}

#[derive(Debug, Clone)]
pub struct DecodedTile {
    pub polygons: Vec<DecodedPolygon>,
    pub lines: Vec<DecodedLine>,
    pub boundaries: Vec<DecodedBoundary>,
    pub labels: Vec<DecodedLabel>,
    pub byte_size: usize,
}

impl DecodedTile {
    fn compute_byte_size(&mut self) {
        let mut size = std::mem::size_of::<Self>();
        for p in &self.polygons {
            size += std::mem::size_of::<DecodedPolygon>()
                + p.vertices.len() * std::mem::size_of::<[f32; 2]>()
                + p.indices.len() * std::mem::size_of::<u32>();
        }
        for l in &self.lines {
            size += std::mem::size_of::<DecodedLine>()
                + l.points.len() * std::mem::size_of::<[f32; 2]>();
        }
        for b in &self.boundaries {
            size += std::mem::size_of::<DecodedBoundary>()
                + b.points.len() * std::mem::size_of::<[f32; 2]>();
        }
        for lb in &self.labels {
            size += std::mem::size_of::<DecodedLabel>() + lb.text.len();
        }
        self.byte_size = size;
    }
}

struct FeatureTags<'a> {
    kind: &'a str,
    kind_detail: &'a str,
    class: &'a str,
    name: &'a str,
    ref_code: &'a str,
    brunnel: &'a str,
    is_bridge: bool,
    is_tunnel: bool,
    admin_level: f64,
    population: f64,
}

impl<'a> FeatureTags<'a> {
    fn parse(layer: &'a ProtoLayer, tags: &[u32]) -> Self {
        let mut out = Self {
            kind: "",
            kind_detail: "",
            class: "",
            name: "",
            ref_code: "",
            brunnel: "",
            is_bridge: false,
            is_tunnel: false,
            admin_level: 0.0,
            population: 0.0,
        };

        let mut i = 0;
        while i + 1 < tags.len() {
            let k_idx = tags[i] as usize;
            let v_idx = tags[i + 1] as usize;
            i += 2;
            if k_idx >= layer.keys.len() || v_idx >= layer.values.len() {
                continue;
            }
            let key = layer.keys[k_idx].as_str();
            let val = &layer.values[v_idx];
            match key {
                "kind" => {
                    if let Some(s) = val.as_str() {
                        out.kind = s;
                    }
                }
                "kind_detail" => {
                    if let Some(s) = val.as_str() {
                        out.kind_detail = s;
                    } else if let Some(n) = val.as_f64() {
                        if n as i32 == 2 {
                            out.kind_detail = "2";
                        }
                    }
                }
                "class" => {
                    if let Some(s) = val.as_str() {
                        out.class = s;
                    }
                }
                "name" | "name:es" => {
                    if out.name.is_empty() || key == "name:es" {
                        if let Some(s) = val.as_str() {
                            if !s.is_empty() {
                                out.name = s;
                            }
                        }
                    }
                }
                "ref" => {
                    if let Some(s) = val.as_str() {
                        out.ref_code = s;
                    }
                }
                "brunnel" => {
                    if let Some(s) = val.as_str() {
                        out.brunnel = s;
                    }
                }
                "is_bridge" => {
                    out.is_bridge = val.as_bool().unwrap_or(false)
                        || val.as_f64().map(|v| v > 0.0).unwrap_or(false);
                }
                "is_tunnel" => {
                    out.is_tunnel = val.as_bool().unwrap_or(false)
                        || val.as_f64().map(|v| v > 0.0).unwrap_or(false);
                }
                "admin_level" => {
                    if let Some(n) = val.as_f64() {
                        out.admin_level = n;
                    }
                }
                "population" => {
                    if let Some(n) = val.as_f64() {
                        out.population = n;
                    }
                }
                _ => {}
            }
        }

        if out.brunnel == "bridge" {
            out.is_bridge = true;
        } else if out.brunnel == "tunnel" {
            out.is_tunnel = true;
        }
        out
    }
}

pub fn decode_mvt_tile(raw_bytes: &[u8], tile_z: u8) -> Result<DecodedTile, String> {
    let proto_tile =
        ProtoTile::decode(raw_bytes).map_err(|e| format!("Error decodificando MVT: {e}"))?;

    let mut polygons = Vec::new();
    let mut lines = Vec::new();
    let mut boundaries = Vec::new();
    let mut labels = Vec::new();
    let mut seen_street_names: HashSet<String> = HashSet::new();

    for layer in &proto_tile.layers {
        let _ = layer.version;
        let layer_name = layer.name.as_deref().unwrap_or("");
        let extent = layer.extent.unwrap_or(4096).max(1) as f32;

        for feature in &layer.features {
            let _ = feature.id;
            let geom_type = feature.geom_type.unwrap_or(0);
            let tags = FeatureTags::parse(layer, &feature.tags);

            match geom_type {
                // POINT
                1 => {
                    let pts = decode_points(&feature.geometry, extent);
                    if !tags.name.is_empty() {
                        if let Some(pt) = pts.first().copied() {
                            if pt[0] >= -0.05 && pt[0] <= 1.05 && pt[1] >= -0.05 && pt[1] <= 1.05 {
                                if let Some((lkind, prio)) =
                                    classify_point_label(layer_name, &tags, tile_z)
                                {
                                    labels.push(DecodedLabel {
                                        kind: lkind,
                                        text: tags.name.to_string(),
                                        pos: pt,
                                        priority: prio,
                                    });
                                }
                            }
                        }
                    }
                }
                // LINESTRING
                2 => {
                    let paths = decode_lines(&feature.geometry, extent);
                    if paths.is_empty() {
                        continue;
                    }

                    if layer_name == "boundaries" || layer_name == "boundary" {
                        let bkind = if tags.kind == "country"
                            || tags.kind_detail == "2"
                            || (tags.admin_level > 0.0 && tags.admin_level <= 2.5)
                        {
                            BoundaryKind::Country
                        } else {
                            BoundaryKind::Region
                        };
                        for path in paths {
                            if path.len() >= 2 {
                                boundaries.push(DecodedBoundary {
                                    kind: bkind,
                                    points: path,
                                });
                            }
                        }
                        continue;
                    }

                    if let Some(rkind) =
                        classify_road(layer_name, tags.kind, tags.kind_detail, tags.class)
                    {
                        let label_text = if !tags.name.is_empty() {
                            Some(tags.name)
                        } else if !tags.ref_code.is_empty() && rkind >= RoadKind::Primary {
                            Some(tags.ref_code)
                        } else {
                            None
                        };

                        let mut added_label = false;
                        for path in paths {
                            if path.len() < 2 {
                                continue;
                            }
                            if !added_label && rkind != RoadKind::Waterway && rkind != RoadKind::Rail
                            {
                                if let Some(text) = label_text {
                                    if tile_z >= min_zoom_for_road_label(rkind)
                                        && !seen_street_names.contains(text)
                                    {
                                        if let Some(mid) = polyline_midpoint(&path) {
                                            if mid[0] >= 0.04
                                                && mid[0] <= 0.96
                                                && mid[1] >= 0.04
                                                && mid[1] <= 0.96
                                            {
                                                seen_street_names.insert(text.to_string());
                                                let prio = match rkind {
                                                    RoadKind::Motorway => 55,
                                                    RoadKind::Trunk => 50,
                                                    RoadKind::Primary => 45,
                                                    RoadKind::Secondary => 40,
                                                    RoadKind::Tertiary => 35,
                                                    _ => 28,
                                                };
                                                labels.push(DecodedLabel {
                                                    kind: LabelKind::Street,
                                                    text: text.to_string(),
                                                    pos: mid,
                                                    priority: prio,
                                                });
                                                added_label = true;
                                            }
                                        }
                                    }
                                }
                            }

                            if layer_name != "transportation_name" {
                                lines.push(DecodedLine {
                                    kind: rkind,
                                    is_bridge: tags.is_bridge,
                                    is_tunnel: tags.is_tunnel,
                                    points: path,
                                });
                            }
                        }
                    }
                }
                // POLYGON
                3 => {
                    if let Some(akind) = classify_area(layer_name, tags.kind, tags.class) {
                        let rings = decode_lines(&feature.geometry, extent);
                        if rings.is_empty() {
                            continue;
                        }
                        let triangulated = triangulate_mvt_rings(&rings);
                        for (vertices, indices) in triangulated {
                            if !vertices.is_empty() && indices.len() >= 3 {
                                polygons.push(DecodedPolygon {
                                    kind: akind,
                                    vertices,
                                    indices,
                                });
                            }
                        }

                        if !tags.name.is_empty()
                            && (akind == AreaKind::Park
                                || akind == AreaKind::Forest
                                || akind == AreaKind::OceanOrLake)
                            && tile_z >= 10
                        {
                            if let Some(center) = ring_centroid(&rings[0]) {
                                if center[0] >= 0.05
                                    && center[0] <= 0.95
                                    && center[1] >= 0.05
                                    && center[1] <= 0.95
                                {
                                    let lkind = if akind == AreaKind::OceanOrLake {
                                        LabelKind::Water
                                    } else {
                                        LabelKind::Park
                                    };
                                    labels.push(DecodedLabel {
                                        kind: lkind,
                                        text: tags.name.to_string(),
                                        pos: center,
                                        priority: 22,
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    polygons.sort_by_key(|p| p.kind);
    lines.sort_by_key(|l| (l.is_bridge, l.kind));
    labels.sort_by(|a, b| b.priority.cmp(&a.priority));

    let mut tile = DecodedTile {
        polygons,
        lines,
        boundaries,
        labels,
        byte_size: 0,
    };
    tile.compute_byte_size();
    Ok(tile)
}

fn min_zoom_for_road_label(kind: RoadKind) -> u8 {
    match kind {
        RoadKind::Motorway | RoadKind::Trunk => 10,
        RoadKind::Primary => 11,
        RoadKind::Secondary => 12,
        RoadKind::Tertiary | RoadKind::Residential => 13,
        _ => 14,
    }
}

fn classify_point_label(
    layer_name: &str,
    tags: &FeatureTags<'_>,
    tile_z: u8,
) -> Option<(LabelKind, i32)> {
    let t = if !tags.kind_detail.is_empty() {
        tags.kind_detail
    } else if !tags.kind.is_empty() {
        tags.kind
    } else {
        tags.class
    };

    if layer_name == "places" || layer_name == "place" {
        return match t {
            "country" | "continent" => Some((LabelKind::Country, 120)),
            "city" | "state" | "region" => Some((LabelKind::City, 100)),
            "locality" => {
                if tags.population >= 100_000.0 {
                    Some((LabelKind::City, 98))
                } else if tags.population >= 10_000.0 {
                    Some((LabelKind::Town, 85))
                } else if tile_z >= 8 {
                    Some((LabelKind::Town, 78))
                } else {
                    None
                }
            }
            "town" => Some((LabelKind::Town, 85)),
            "village" | "hamlet" => {
                if tile_z >= 9 {
                    Some((LabelKind::Village, 70))
                } else {
                    None
                }
            }
            "suburb" | "neighbourhood" | "macrohood" | "quarter" => {
                if tile_z >= 11 {
                    Some((LabelKind::Suburb, 65))
                } else {
                    None
                }
            }
            _ => {
                if tile_z >= 9 {
                    Some((LabelKind::Town, 75))
                } else {
                    None
                }
            }
        };
    }

    if (layer_name == "pois" || layer_name == "poi") && tile_z >= 14 {
        return Some((LabelKind::Poi, 18));
    }

    None
}

#[inline]
fn zigzag_decode(n: u32) -> i32 {
    ((n >> 1) as i32) ^ -((n & 1) as i32)
}

fn decode_points(geom: &[u32], extent: f32) -> Vec<[f32; 2]> {
    let mut pts = Vec::new();
    let mut i = 0;
    let mut cx: i32 = 0;
    let mut cy: i32 = 0;

    while i < geom.len() {
        let cmd_int = geom[i];
        i += 1;
        let cmd_id = cmd_int & 0x7;
        let count = cmd_int >> 3;
        if cmd_id == 1 {
            for _ in 0..count {
                if i + 1 >= geom.len() {
                    return pts;
                }
                cx += zigzag_decode(geom[i]);
                cy += zigzag_decode(geom[i + 1]);
                i += 2;
                pts.push([cx as f32 / extent, cy as f32 / extent]);
            }
        } else {
            break;
        }
    }
    pts
}

fn decode_lines(geom: &[u32], extent: f32) -> Vec<Vec<[f32; 2]>> {
    let mut lines: Vec<Vec<[f32; 2]>> = Vec::new();
    let mut current: Vec<[f32; 2]> = Vec::new();
    let mut i = 0;
    let mut cx: i32 = 0;
    let mut cy: i32 = 0;

    while i < geom.len() {
        let cmd_int = geom[i];
        i += 1;
        let cmd_id = cmd_int & 0x7;
        let count = cmd_int >> 3;
        match cmd_id {
            1 => {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                for _ in 0..count {
                    if i + 1 >= geom.len() {
                        break;
                    }
                    cx += zigzag_decode(geom[i]);
                    cy += zigzag_decode(geom[i + 1]);
                    i += 2;
                    current.push([cx as f32 / extent, cy as f32 / extent]);
                }
            }
            2 => {
                for _ in 0..count {
                    if i + 1 >= geom.len() {
                        break;
                    }
                    cx += zigzag_decode(geom[i]);
                    cy += zigzag_decode(geom[i + 1]);
                    i += 2;
                    current.push([cx as f32 / extent, cy as f32 / extent]);
                }
            }
            7 => {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
            }
            _ => break,
        }
    }

    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn polyline_midpoint(pts: &[[f32; 2]]) -> Option<[f32; 2]> {
    if pts.len() < 2 {
        return None;
    }
    let mut total_len = 0.0_f32;
    for w in pts.windows(2) {
        let dx = w[1][0] - w[0][0];
        let dy = w[1][1] - w[0][1];
        total_len += (dx * dx + dy * dy).sqrt();
    }
    if total_len < 0.06 {
        return None;
    }
    let half = total_len * 0.5;
    let mut acc = 0.0_f32;
    for w in pts.windows(2) {
        let dx = w[1][0] - w[0][0];
        let dy = w[1][1] - w[0][1];
        let seg = (dx * dx + dy * dy).sqrt();
        if acc + seg >= half && seg > 1e-6 {
            let t = (half - acc) / seg;
            return Some([w[0][0] + dx * t, w[0][1] + dy * t]);
        }
        acc += seg;
    }
    Some(pts[pts.len() / 2])
}

fn ring_centroid(pts: &[[f32; 2]]) -> Option<[f32; 2]> {
    if pts.len() < 3 {
        return None;
    }
    let mut sx = 0.0_f32;
    let mut sy = 0.0_f32;
    for p in pts {
        sx += p[0];
        sy += p[1];
    }
    let n = pts.len() as f32;
    Some([sx / n, sy / n])
}

fn simplify_ring(ring: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut out: Vec<[f32; 2]> = Vec::with_capacity(ring.len());
    for &p in ring {
        if let Some(&last) = out.last() {
            if (p[0] - last[0]).abs() < 1e-5 && (p[1] - last[1]).abs() < 1e-5 {
                continue;
            }
        }
        out.push(p);
    }
    if out.len() >= 2 {
        let first = out[0];
        let last = *out.last().unwrap();
        if (first[0] - last[0]).abs() < 1e-5 && (first[1] - last[1]).abs() < 1e-5 {
            out.pop();
        }
    }
    out
}

fn signed_area(pts: &[[f32; 2]]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0_f64;
    let n = pts.len();
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        area += (a[0] as f64) * (b[1] as f64) - (b[0] as f64) * (a[1] as f64);
    }
    area * 0.5
}

fn triangulate_mvt_rings(raw_rings: &[Vec<[f32; 2]>]) -> Vec<(Vec<[f32; 2]>, Vec<u32>)> {
    let mut groups: Vec<(Vec<[f32; 2]>, Vec<Vec<[f32; 2]>>)> = Vec::new();

    for raw in raw_rings {
        let mut ring = simplify_ring(raw);
        if ring.len() < 3 {
            continue;
        }
        let area = signed_area(&ring);
        if area.abs() < 1e-11 {
            continue;
        }
        if area > 0.0 {
            groups.push((ring, Vec::new()));
        } else if let Some((_, holes)) = groups.last_mut() {
            holes.push(ring);
        } else {
            ring.reverse();
            groups.push((ring, Vec::new()));
        }
    }

    let mut result = Vec::with_capacity(groups.len());
    for (mut outer, mut holes) in groups {
        if holes.len() > 256 {
            holes.truncate(256);
        }
        holes.sort_by(|a, b| {
            let max_a = a.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
            let max_b = b.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
            max_b.total_cmp(&max_a)
        });
        for hole in holes {
            if outer.len() + hole.len() > 16_000 {
                break;
            }
            merge_hole(&mut outer, &hole);
        }

        let indices = ear_clip_simple(&outer);
        if indices.len() >= 3 {
            result.push((outer, indices));
        }
    }
    result
}

#[inline]
fn cross2d(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f64 {
    let abx = (b[0] - a[0]) as f64;
    let aby = (b[1] - a[1]) as f64;
    let acx = (c[0] - a[0]) as f64;
    let acy = (c[1] - a[1]) as f64;
    abx * acy - aby * acx
}

fn point_in_triangle(a: [f32; 2], b: [f32; 2], c: [f32; 2], p: [f32; 2]) -> bool {
    let orient = cross2d(a, b, c);
    if orient >= 0.0 {
        cross2d(a, b, p) >= -1e-10 && cross2d(b, c, p) >= -1e-10 && cross2d(c, a, p) >= -1e-10
    } else {
        cross2d(a, b, p) <= 1e-10 && cross2d(b, c, p) <= 1e-10 && cross2d(c, a, p) <= 1e-10
    }
}

fn merge_hole(outer: &mut Vec<[f32; 2]>, hole: &[[f32; 2]]) {
    if outer.len() < 3 || hole.len() < 3 {
        return;
    }

    let mut m_idx = 0;
    for (idx, pt) in hole.iter().enumerate() {
        if pt[0] > hole[m_idx][0] || (pt[0] == hole[m_idx][0] && pt[1] < hole[m_idx][1]) {
            m_idx = idx;
        }
    }
    let m = hole[m_idx];

    let mut best_ix = f32::INFINITY;
    let mut best_edge: Option<(usize, usize)> = None;
    let n = outer.len();

    for i in 0..n {
        let j = (i + 1) % n;
        let a = outer[i];
        let b = outer[j];
        if (a[1] <= m[1] && b[1] > m[1]) || (b[1] <= m[1] && a[1] > m[1]) {
            let denom = b[1] - a[1];
            if denom.abs() > 1e-8 {
                let t = (m[1] - a[1]) / denom;
                let ix = a[0] + t * (b[0] - a[0]);
                if ix >= m[0] - 1e-6 && ix < best_ix {
                    best_ix = ix;
                    best_edge = Some((i, j));
                }
            }
        }
    }

    let Some((ei, ej)) = best_edge else {
        return;
    };

    let mut p_idx = if outer[ei][0] >= outer[ej][0] {
        ei
    } else {
        ej
    };
    let p = outer[p_idx];
    let ray_pt = [best_ix, m[1]];

    let mut best_score = f64::INFINITY;
    let mut best_dist = f64::INFINITY;
    for k in 0..n {
        if k == p_idx {
            continue;
        }
        let prev = outer[(k + n - 1) % n];
        let curr = outer[k];
        let next = outer[(k + 1) % n];
        if cross2d(prev, curr, next) < 0.0 && point_in_triangle(m, ray_pt, p, curr) {
            let dx = (curr[0] - m[0]) as f64;
            let dy = (curr[1] - m[1]).abs() as f64;
            if dx >= -1e-7 {
                let angle_proxy = dy / dx.max(1e-7);
                let dist = dx * dx + dy * dy;
                if angle_proxy < best_score - 1e-9
                    || ((angle_proxy - best_score).abs() <= 1e-9 && dist < best_dist)
                {
                    best_score = angle_proxy;
                    best_dist = dist;
                    p_idx = k;
                }
            }
        }
    }

    let mut merged = Vec::with_capacity(outer.len() + hole.len() + 2);
    merged.extend_from_slice(&outer[..=p_idx]);
    merged.extend_from_slice(&hole[m_idx..]);
    merged.extend_from_slice(&hole[..=m_idx]);
    merged.extend_from_slice(&outer[p_idx..]);
    *outer = merged;
}

fn ear_clip_simple(pts: &[[f32; 2]]) -> Vec<u32> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    if n == 3 {
        return vec![0, 1, 2];
    }
    if n == 4 {
        let c0 = cross2d(pts[0], pts[1], pts[2]);
        let c2 = cross2d(pts[2], pts[3], pts[0]);
        if c0 > 0.0 && c2 > 0.0 {
            return vec![0, 1, 2, 2, 3, 0];
        }
    }

    let mut active: Vec<usize> = (0..n).collect();
    let mut indices = Vec::with_capacity((n - 2) * 3);

    let mut i = 0usize;
    let mut no_progress_steps = 0usize;
    let max_total_steps = n * 4 + 32;
    let mut total_steps = 0usize;

    while active.len() > 3 && total_steps < max_total_steps {
        total_steps += 1;
        let m = active.len();
        if i >= m {
            i = 0;
        }
        let prev_pos = (i + m - 1) % m;
        let next_pos = (i + 1) % m;

        let p_idx = active[prev_pos];
        let c_idx = active[i];
        let n_idx = active[next_pos];

        let a = pts[p_idx];
        let b = pts[c_idx];
        let c = pts[n_idx];

        let cross = cross2d(a, b, c);
        if cross > 1e-11 {
            let min_x = a[0].min(b[0]).min(c[0]);
            let max_x = a[0].max(b[0]).max(c[0]);
            let min_y = a[1].min(b[1]).min(c[1]);
            let max_y = a[1].max(b[1]).max(c[1]);

            let mut contains_other = false;
            for (pos, &v_idx) in active.iter().enumerate() {
                if pos == prev_pos || pos == i || pos == next_pos {
                    continue;
                }
                let q = pts[v_idx];
                if q[0] < min_x || q[0] > max_x || q[1] < min_y || q[1] > max_y {
                    continue;
                }
                if (q[0] - a[0]).abs() < 1e-7 && (q[1] - a[1]).abs() < 1e-7
                    || (q[0] - b[0]).abs() < 1e-7 && (q[1] - b[1]).abs() < 1e-7
                    || (q[0] - c[0]).abs() < 1e-7 && (q[1] - c[1]).abs() < 1e-7
                {
                    continue;
                }
                let qp = active[(pos + m - 1) % m];
                let qn = active[(pos + 1) % m];
                if cross2d(pts[qp], q, pts[qn]) <= 1e-9 && point_in_triangle(a, b, c, q) {
                    contains_other = true;
                    break;
                }
            }

            if !contains_other {
                indices.push(p_idx as u32);
                indices.push(c_idx as u32);
                indices.push(n_idx as u32);
                active.remove(i);
                no_progress_steps = 0;
                continue;
            }
        }

        i += 1;
        no_progress_steps += 1;
        if no_progress_steps > m * 2 {
            let mut best_pos = None;
            let mut min_abs_cross = f64::INFINITY;
            for pos in 0..m {
                let pa = pts[active[(pos + m - 1) % m]];
                let pb = pts[active[pos]];
                let pc = pts[active[(pos + 1) % m]];
                let ac = cross2d(pa, pb, pc).abs();
                if ac < min_abs_cross {
                    min_abs_cross = ac;
                    best_pos = Some(pos);
                }
            }
            if let Some(pos) = best_pos {
                if min_abs_cross < 1e-7 {
                    active.remove(pos);
                    no_progress_steps = 0;
                    continue;
                }
            }
            break;
        }
    }

    if active.len() == 3 {
        indices.push(active[0] as u32);
        indices.push(active[1] as u32);
        indices.push(active[2] as u32);
    }

    indices
}