use egui::{Pos2, Rect, Vec2};

pub const TILE_SIZE_PX: f64 = 512.0;
pub const MAX_MERCATOR_LAT: f64 = 85.05112878;

pub fn lon_to_nx(lon: f64) -> f64 {
    ((lon + 180.0) / 360.0).clamp(0.0, 1.0)
}

pub fn lat_to_ny(lat: f64) -> f64 {
    let clamped = lat.clamp(-MAX_MERCATOR_LAT, MAX_MERCATOR_LAT);
    let lat_rad = clamped.to_radians();
    let ny = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) * 0.5;
    ny.clamp(0.0, 1.0)
}

pub fn nx_to_lon(nx: f64) -> f64 {
    nx * 360.0 - 180.0
}

pub fn ny_to_lat(ny: f64) -> f64 {
    let clamped = ny.clamp(0.0, 1.0);
    let n = std::f64::consts::PI * (1.0 - 2.0 * clamped);
    n.sinh().atan().to_degrees()
}

pub fn tile_bounds_lat_lon(z: u8, x: u32, y: u32) -> (f64, f64, f64, f64) {
    let n = (1u64 << (z as u32)) as f64;
    let min_lon = (x as f64 / n) * 360.0 - 180.0;
    let max_lon = ((x + 1) as f64 / n) * 360.0 - 180.0;
    let max_lat = ny_to_lat(y as f64 / n);
    let min_lat = ny_to_lat((y + 1) as f64 / n);
    (min_lon, min_lat, max_lon, max_lat)
}

pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0088;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat * 0.5).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon * 0.5).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).max(0.0).sqrt());
    EARTH_RADIUS_KM * c
}

#[derive(Debug, Clone)]
pub struct MapCamera {
    pub center_nx: f64,
    pub center_ny: f64,
    pub zoom: f64,
}

impl MapCamera {
    pub fn from_lat_lon(lat: f64, lon: f64, zoom: f64) -> Self {
        Self {
            center_nx: lon_to_nx(lon),
            center_ny: lat_to_ny(lat),
            zoom: zoom.clamp(0.5, 20.5),
        }
    }

    pub fn center_lat_lon(&self) -> (f64, f64) {
        (ny_to_lat(self.center_ny), nx_to_lon(self.center_nx))
    }

    pub fn world_px(&self) -> f64 {
        TILE_SIZE_PX * 2.0_f64.powf(self.zoom)
    }

    pub fn world_to_screen(&self, nx: f64, ny: f64, rect: Rect) -> Pos2 {
        let wpx = self.world_px();
        let cx = rect.center().x as f64;
        let cy = rect.center().y as f64;
        Pos2::new(
            (cx + (nx - self.center_nx) * wpx) as f32,
            (cy + (ny - self.center_ny) * wpx) as f32,
        )
    }

    pub fn lat_lon_to_screen(&self, lat: f64, lon: f64, rect: Rect) -> Pos2 {
        self.world_to_screen(lon_to_nx(lon), lat_to_ny(lat), rect)
    }

    pub fn screen_to_world(&self, pos: Pos2, rect: Rect) -> (f64, f64) {
        let wpx = self.world_px();
        let nx = self.center_nx + (pos.x as f64 - rect.center().x as f64) / wpx;
        let ny = self.center_ny + (pos.y as f64 - rect.center().y as f64) / wpx;
        (nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0))
    }

    pub fn screen_to_lat_lon(&self, pos: Pos2, rect: Rect) -> (f64, f64) {
        let (nx, ny) = self.screen_to_world(pos, rect);
        (ny_to_lat(ny), nx_to_lon(nx))
    }

    pub fn pan_by_pixels(&mut self, delta: Vec2) {
        let wpx = self.world_px();
        self.center_nx = (self.center_nx - delta.x as f64 / wpx).clamp(0.0, 1.0);
        self.center_ny = (self.center_ny - delta.y as f64 / wpx).clamp(0.0, 1.0);
    }

    pub fn zoom_at_screen_pos(&mut self, delta_zoom: f64, cursor: Pos2, rect: Rect) {
        let wpx_before = self.world_px();
        let cursor_nx = self.center_nx + (cursor.x as f64 - rect.center().x as f64) / wpx_before;
        let cursor_ny = self.center_ny + (cursor.y as f64 - rect.center().y as f64) / wpx_before;

        self.zoom = (self.zoom + delta_zoom).clamp(0.5, 20.5);
        let wpx_after = self.world_px();

        self.center_nx =
            (cursor_nx - (cursor.x as f64 - rect.center().x as f64) / wpx_after).clamp(0.0, 1.0);
        self.center_ny =
            (cursor_ny - (cursor.y as f64 - rect.center().y as f64) / wpx_after).clamp(0.0, 1.0);
    }

    pub fn fit_bounds(
        &mut self,
        min_lon: f64,
        min_lat: f64,
        max_lon: f64,
        max_lat: f64,
        viewport_size: Vec2,
    ) {
        let nx0 = lon_to_nx(min_lon.min(max_lon));
        let nx1 = lon_to_nx(min_lon.max(max_lon));
        let ny0 = lat_to_ny(max_lat.max(min_lat));
        let ny1 = lat_to_ny(max_lat.min(min_lat));

        self.center_nx = ((nx0 + nx1) * 0.5).clamp(0.0, 1.0);
        self.center_ny = ((ny0 + ny1) * 0.5).clamp(0.0, 1.0);

        let dx = (nx1 - nx0).max(1e-6);
        let dy = (ny1 - ny0).max(1e-6);

        let avail_w = (viewport_size.x as f64 * 0.85).max(100.0);
        let avail_h = (viewport_size.y as f64 * 0.85).max(100.0);

        let zoom_x = (avail_w / (dx * TILE_SIZE_PX)).log2();
        let zoom_y = (avail_h / (dy * TILE_SIZE_PX)).log2();
        self.zoom = zoom_x.min(zoom_y).clamp(1.0, 17.5);
    }

    pub fn visible_bounds_lat_lon(&self, rect: Rect) -> (f64, f64, f64, f64) {
        let (north, west) = self.screen_to_lat_lon(rect.left_top(), rect);
        let (south, east) = self.screen_to_lat_lon(rect.right_bottom(), rect);
        (west, south, east, north)
    }

    pub fn visible_tiles(&self, rect: Rect, target_z: u8) -> Vec<(u8, u32, u32)> {
        let n = 1u32 << (target_z as u32);
        let wpx = self.world_px();

        let left_nx = self.center_nx + (rect.left() as f64 - rect.center().x as f64) / wpx;
        let right_nx = self.center_nx + (rect.right() as f64 - rect.center().x as f64) / wpx;
        let top_ny = self.center_ny + (rect.top() as f64 - rect.center().y as f64) / wpx;
        let bottom_ny = self.center_ny + (rect.bottom() as f64 - rect.center().y as f64) / wpx;

        let min_x = (left_nx * n as f64).floor().max(0.0).min((n - 1) as f64) as u32;
        let max_x = (right_nx * n as f64).floor().max(0.0).min((n - 1) as f64) as u32;
        let min_y = (top_ny * n as f64).floor().max(0.0).min((n - 1) as f64) as u32;
        let max_y = (bottom_ny * n as f64).floor().max(0.0).min((n - 1) as f64) as u32;

        let mut tiles = Vec::new();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                tiles.push((target_z, x, y));
            }
        }
        tiles
    }
}