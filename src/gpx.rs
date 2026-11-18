use crate::map_math::haversine_km;
use egui::Color32;
use std::fs;
use std::path::Path;

const GPX_COLORS: [[u8; 3]; 5] = [
    [232, 56, 132],
    [22, 168, 224],
    [238, 118, 32],
    [138, 68, 220],
    [32, 186, 112],
];

#[derive(Debug, Clone)]
pub struct GpxRoute {
    pub id: u64,
    pub portable_key: String,
    pub name: String,
    pub segments: Vec<Vec<(f64, f64)>>,
    pub waypoints: Vec<(f64, f64, String)>,
    pub bounds: (f64, f64, f64, f64), // min_lon, min_lat, max_lon, max_lat
    pub distance_km: f64,
    pub color: [u8; 3],
    pub visible: bool,
}

impl GpxRoute {
    pub fn color32(&self) -> Color32 {
        Color32::from_rgb(self.color[0], self.color[1], self.color[2])
    }
}

pub fn parse_gpx_file(
    path: &Path,
    portable_key: String,
    id: u64,
    color_idx: usize,
) -> Result<GpxRoute, String> {
    let xml = fs::read_to_string(path)
        .map_err(|e| format!("No se pudo leer '{}': {e}", path.display()))?;
    let doc = roxmltree::Document::parse(&xml)
        .map_err(|e| format!("Archivo GPX inválido: {e}"))?;

    let mut route_name: Option<String> = None;
    let mut segments: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut route_points: Vec<(f64, f64)> = Vec::new();
    let mut waypoints: Vec<(f64, f64, String)> = Vec::new();

    for node in doc.descendants() {
        match node.tag_name().name() {
            "trk" | "metadata" => {
                if route_name.is_none() {
                    if let Some(name_node) = node.children().find(|c| c.has_tag_name("name")) {
                        if let Some(t) = name_node.text() {
                            let trimmed = t.trim();
                            if !trimmed.is_empty() {
                                route_name = Some(trimmed.to_string());
                            }
                        }
                    }
                }
            }
            "trkseg" => {
                let mut seg = Vec::new();
                for pt in node.children().filter(|c| c.has_tag_name("trkpt")) {
                    if let (Some(lat_s), Some(lon_s)) = (pt.attribute("lat"), pt.attribute("lon"))
                    {
                        if let (Ok(lat), Ok(lon)) = (lat_s.parse::<f64>(), lon_s.parse::<f64>()) {
                            seg.push((lat, lon));
                        }
                    }
                }
                if !seg.is_empty() {
                    segments.push(seg);
                }
            }
            "rtept" => {
                if let (Some(lat_s), Some(lon_s)) = (node.attribute("lat"), node.attribute("lon"))
                {
                    if let (Ok(lat), Ok(lon)) = (lat_s.parse::<f64>(), lon_s.parse::<f64>()) {
                        route_points.push((lat, lon));
                    }
                }
            }
            "wpt" => {
                if let (Some(lat_s), Some(lon_s)) = (node.attribute("lat"), node.attribute("lon"))
                {
                    if let (Ok(lat), Ok(lon)) = (lat_s.parse::<f64>(), lon_s.parse::<f64>()) {
                        let wname = node
                            .children()
                            .find(|c| c.has_tag_name("name"))
                            .and_then(|c| c.text())
                            .map(|s| s.trim().to_string())
                            .unwrap_or_default();
                        waypoints.push((lat, lon, wname));
                    }
                }
            }
            _ => {}
        }
    }

    if !route_points.is_empty() {
        segments.push(route_points);
    }

    if segments.is_empty() && waypoints.is_empty() {
        return Err("El archivo GPX no contiene puntos ni rutas válidas".to_string());
    }

    let mut min_lon = f64::INFINITY;
    let mut min_lat = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut distance_km = 0.0_f64;

    for seg in &segments {
        for &(lat, lon) in seg {
            min_lon = min_lon.min(lon);
            max_lon = max_lon.max(lon);
            min_lat = min_lat.min(lat);
            max_lat = max_lat.max(lat);
        }
        for pair in seg.windows(2) {
            distance_km += haversine_km(pair[0].0, pair[0].1, pair[1].0, pair[1].1);
        }
    }

    for &(lat, lon, _) in &waypoints {
        min_lon = min_lon.min(lon);
        max_lon = max_lon.max(lon);
        min_lat = min_lat.min(lat);
        max_lat = max_lat.max(lat);
    }

    let fallback_name = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Ruta GPX".to_string());

    Ok(GpxRoute {
        id,
        portable_key,
        name: route_name.unwrap_or(fallback_name),
        segments,
        waypoints,
        bounds: (min_lon, min_lat, max_lon, max_lat),
        distance_km,
        color: GPX_COLORS[color_idx % GPX_COLORS.len()],
        visible: true,
    })
}