use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PortablePaths {
    pub exe_dir: PathBuf,
    pub maps_dir: PathBuf,
    pub config_file: PathBuf,
    pub markers_file: PathBuf,
}

impl PortablePaths {
    pub fn detect() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|parent| parent.to_path_buf()))
            .unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            });

        let maps_dir = exe_dir.join("mapas");
        let _ = fs::create_dir_all(&maps_dir);

        let config_file = exe_dir.join("config.json");
        let markers_file = exe_dir.join("marcadores.json");

        Self {
            exe_dir,
            maps_dir,
            config_file,
            markers_file,
        }
    }

    pub fn make_portable_key(&self, path: &Path) -> String {
        if let Ok(rel) = path.strip_prefix(&self.exe_dir) {
            rel.to_string_lossy().replace('\\', "/")
        } else {
            path.to_string_lossy().to_string()
        }
    }

    pub fn resolve_key(&self, stored: &str) -> PathBuf {
        let candidate = PathBuf::from(stored);
        if candidate.is_absolute() {
            candidate
        } else {
            let normalized = stored.replace('/', std::path::MAIN_SEPARATOR_STR);
            self.exe_dir.join(normalized)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub dark_mode: bool,
    pub center_lat: f64,
    pub center_lon: f64,
    pub zoom: f64,
    pub enabled_maps: Vec<String>,
    pub extra_map_paths: Vec<String>,
    pub gpx_paths: Vec<String>,
    pub show_buildings: bool,
    pub show_labels: bool,
    pub first_run_initialized: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            dark_mode: false,
            center_lat: 25.0,
            center_lon: 0.0,
            zoom: 2.0,
            enabled_maps: Vec::new(),
            extra_map_paths: Vec::new(),
            gpx_paths: Vec::new(),
            show_buildings: true,
            show_labels: true,
            first_run_initialized: false,
        }
    }
}

impl AppConfig {
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }
}