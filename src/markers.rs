use egui::Color32;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const MARKER_COLOR_PRESETS: [(&str, [u8; 3]); 6] = [
    ("Rojo", [225, 68, 68]),
    ("Azul", [46, 134, 222]),
    ("Verde", [46, 178, 96]),
    ("Naranja", [240, 140, 38]),
    ("Morado", [148, 82, 208]),
    ("Turquesa", [22, 168, 168]),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapMarker {
    pub id: u64,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub color: [u8; 3],
    #[serde(default)]
    pub notes: String,
}

impl MapMarker {
    pub fn color32(&self) -> Color32 {
        Color32::from_rgb(self.color[0], self.color[1], self.color[2])
    }
}

pub struct MarkerStore {
    pub markers: Vec<MapMarker>,
    next_id: u64,
}

impl MarkerStore {
    pub fn load(path: &Path) -> Self {
        let markers: Vec<MapMarker> = match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        let next_id = markers.iter().map(|m| m.id).max().unwrap_or(0) + 1;
        Self { markers, next_id }
    }

    pub fn save(&self, path: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(&self.markers) {
            let _ = fs::write(path, json);
        }
    }

    pub fn add_marker(
        &mut self,
        path: &Path,
        name: String,
        lat: f64,
        lon: f64,
        color: [u8; 3],
        notes: String,
    ) {
        let id = self.next_id;
        self.next_id += 1;
        self.markers.push(MapMarker {
            id,
            name,
            lat,
            lon,
            color,
            notes,
        });
        self.save(path);
    }

    pub fn remove_marker(&mut self, path: &Path, id: u64) {
        self.markers.retain(|m| m.id != id);
        self.save(path);
    }
}