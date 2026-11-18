use crate::config::{AppConfig, PortablePaths};
use crate::gpx::{parse_gpx_file, GpxRoute};
use crate::map_math::{haversine_km, MapCamera};
use crate::markers::{MarkerStore, MARKER_COLOR_PRESETS};
use crate::pmtiles_reader::PmTilesArchive;
use crate::renderer::{format_distance_km, render_map, MapRenderLayer};
use crate::style::MapPalette;
use crate::tile_manager::TileManager;
use egui::{
    Align2, Button, Color32, Context, Frame, Margin, PointerButton, Pos2, RichText, Rounding,
    ScrollArea, Sense, Stroke, Vec2,
};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarTab {
    Maps,
    Markers,
    Routes,
}

struct LoadedMap {
    id: usize,
    portable_key: String,
    display_name: String,
    archive: Arc<PmTilesArchive>,
    enabled: bool,
}

struct ContextMenuState {
    screen_pos: Pos2,
    lat: f64,
    lon: f64,
}

struct NewMarkerDialog {
    open: bool,
    name: String,
    notes: String,
    lat: f64,
    lon: f64,
    color_idx: usize,
}

struct HelpModalState {
    open: bool,
    west: String,
    south: String,
    east: String,
    north: String,
    build_date: String,
    output_name: String,
    copied_until: f64,
    copied_path_until: f64,
}

pub struct NomadMapsApp {
    paths: PortablePaths,
    config: AppConfig,
    camera: MapCamera,
    tile_manager: TileManager,
    marker_store: MarkerStore,
    maps: Vec<LoadedMap>,
    routes: Vec<GpxRoute>,
    next_map_id: usize,
    next_route_id: u64,
    sidebar_open: bool,
    sidebar_tab: SidebarTab,
    last_map_rect: egui::Rect,
    cursor_lat_lon: Option<(f64, f64)>,
    measure_mode: bool,
    measure_points: Vec<(f64, f64)>,
    context_menu: Option<ContextMenuState>,
    new_marker: NewMarkerDialog,
    help_modal: HelpModalState,
    status_banner: Option<(String, f64)>,
    config_dirty_since: Option<f64>,
}

impl NomadMapsApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let paths = PortablePaths::detect();
        let config = AppConfig::load(&paths.config_file);

        apply_theme_visuals(&cc.egui_ctx, config.dark_mode);

        let camera = MapCamera::from_lat_lon(config.center_lat, config.center_lon, config.zoom);
        let tile_manager = TileManager::new(cc.egui_ctx.clone());
        let marker_store = MarkerStore::load(&paths.markers_file);

        let mut app = Self {
            paths,
            config,
            camera,
            tile_manager,
            marker_store,
            maps: Vec::new(),
            routes: Vec::new(),
            next_map_id: 1,
            next_route_id: 1,
            sidebar_open: true,
            sidebar_tab: SidebarTab::Maps,
            last_map_rect: egui::Rect::from_min_size(Pos2::ZERO, Vec2::new(980.0, 700.0)),
            cursor_lat_lon: None,
            measure_mode: false,
            measure_points: Vec::new(),
            context_menu: None,
            new_marker: NewMarkerDialog {
                open: false,
                name: String::new(),
                notes: String::new(),
                lat: 0.0,
                lon: 0.0,
                color_idx: 0,
            },
            help_modal: HelpModalState {
                open: false,
                west: "-3.8500".to_string(),
                south: "40.3000".to_string(),
                east: "-3.5500".to_string(),
                north: "40.5500".to_string(),
                build_date: "20250201".to_string(),
                output_name: "mi_ciudad.pmtiles".to_string(),
                copied_until: 0.0,
                copied_path_until: 0.0,
            },
            status_banner: None,
            config_dirty_since: None,
        };

        app.scan_maps();
        app.reload_saved_gpx_routes();

        if !app.config.first_run_initialized {
            app.config.first_run_initialized = true;
            if let Some(first) = app.maps.first_mut() {
                first.enabled = true;
                let h = first.archive.header.clone();
                if h.has_valid_bounds() {
                    app.camera.fit_bounds(
                        h.min_lon,
                        h.min_lat,
                        h.max_lon,
                        h.max_lat,
                        app.last_map_rect.size(),
                    );
                } else {
                    app.camera = MapCamera::from_lat_lon(
                        h.center_lat,
                        h.center_lon,
                        (h.center_zoom as f64).max(4.0),
                    );
                }
            }
            app.save_config();
        }

        app
    }

    fn save_config(&mut self) {
        let (lat, lon) = self.camera.center_lat_lon();
        self.config.center_lat = lat;
        self.config.center_lon = lon;
        self.config.zoom = self.camera.zoom;
        self.config.enabled_maps = self
            .maps
            .iter()
            .filter(|m| m.enabled)
            .map(|m| m.portable_key.clone())
            .collect();
        self.config.gpx_paths = self
            .routes
            .iter()
            .map(|r| r.portable_key.clone())
            .collect();
        self.config.save(&self.paths.config_file);
        self.config_dirty_since = None;
    }

    fn mark_dirty(&mut self, time: f64) {
        if self.config_dirty_since.is_none() {
            self.config_dirty_since = Some(time);
        }
    }

    fn scan_maps(&mut self) {
        let previously_enabled: HashSet<String> = if self.maps.is_empty() {
            self.config.enabled_maps.iter().cloned().collect()
        } else {
            self.maps
                .iter()
                .filter(|m| m.enabled)
                .map(|m| m.portable_key.clone())
                .collect()
        };

        let mut candidate_paths: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.paths.maps_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.eq_ignore_ascii_case("pmtiles"))
                        .unwrap_or(false)
                {
                    candidate_paths.push(p);
                }
            }
        }
        candidate_paths.sort();

        for stored in &self.config.extra_map_paths {
            let p = self.paths.resolve_key(stored);
            if p.is_file() && !candidate_paths.contains(&p) {
                candidate_paths.push(p);
            }
        }

        let mut existing_by_key: HashMapLookup = self
            .maps
            .drain(..)
            .map(|m| (m.portable_key.clone(), m))
            .collect();

        let mut new_maps = Vec::new();
        for path in candidate_paths {
            let key = self.paths.make_portable_key(&path);
            if let Some(mut old) = existing_by_key.remove(&key) {
                old.enabled = previously_enabled.contains(&key);
                new_maps.push(old);
            } else {
                match PmTilesArchive::open(&path) {
                    Ok(archive) => {
                        let id = self.next_map_id;
                        self.next_map_id += 1;
                        let display_name = archive
                            .metadata
                            .name
                            .clone()
                            .filter(|n| !n.trim().is_empty())
                            .unwrap_or_else(|| {
                                path.file_name()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_else(|| key.clone())
                            });
                        let enabled = previously_enabled.contains(&key);
                        new_maps.push(LoadedMap {
                            id,
                            portable_key: key,
                            display_name,
                            archive: Arc::new(archive),
                            enabled,
                        });
                    }
                    Err(err) => {
                        self.status_banner = Some((format!("Error abriendo mapa: {err}"), 6.0));
                    }
                }
            }
        }

        self.maps = new_maps;
    }

    fn add_pmtiles_via_dialog(&mut self, time: f64) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Mapas vectoriales PMTiles", &["pmtiles"])
            .pick_file()
        {
            let key = self.paths.make_portable_key(&path);
            if let Some(existing) = self.maps.iter_mut().find(|m| m.portable_key == key) {
                existing.enabled = true;
                let h = existing.archive.header.clone();
                self.center_on_header(&h);
                self.mark_dirty(time);
                return;
            }

            match PmTilesArchive::open(&path) {
                Ok(archive) => {
                    let id = self.next_map_id;
                    self.next_map_id += 1;
                    let display_name = archive
                        .metadata
                        .name
                        .clone()
                        .filter(|n| !n.trim().is_empty())
                        .unwrap_or_else(|| {
                            path.file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| key.clone())
                        });
                    let h = archive.header.clone();
                    if !path.starts_with(&self.paths.maps_dir)
                        && !self.config.extra_map_paths.contains(&key)
                    {
                        self.config.extra_map_paths.push(key.clone());
                    }
                    self.maps.push(LoadedMap {
                        id,
                        portable_key: key,
                        display_name,
                        archive: Arc::new(archive),
                        enabled: true,
                    });
                    self.center_on_header(&h);
                    self.save_config();
                    self.status_banner = Some(("Mapa añadido correctamente".to_string(), time + 4.0));
                }
                Err(err) => {
                    self.status_banner = Some((err, time + 6.0));
                }
            }
        }
    }

    fn reload_saved_gpx_routes(&mut self) {
        let saved = self.config.gpx_paths.clone();
        for key in saved {
            let p = self.paths.resolve_key(&key);
            if p.is_file() {
                let id = self.next_route_id;
                if let Ok(route) = parse_gpx_file(&p, key, id, (id as usize).saturating_sub(1)) {
                    self.next_route_id += 1;
                    self.routes.push(route);
                }
            }
        }
    }

    fn add_gpx_via_dialog(&mut self, time: f64) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Rutas GPX", &["gpx"])
            .pick_file()
        {
            let key = self.paths.make_portable_key(&path);
            let id = self.next_route_id;
            match parse_gpx_file(&path, key, id, (id as usize).saturating_sub(1)) {
                Ok(route) => {
                    self.next_route_id += 1;
                    let bounds = route.bounds;
                    self.routes.push(route);
                    if bounds.2 > bounds.0 && bounds.3 > bounds.1 {
                        self.camera.fit_bounds(
                            bounds.0,
                            bounds.1,
                            bounds.2,
                            bounds.3,
                            self.last_map_rect.size(),
                        );
                    } else {
                        self.camera = MapCamera::from_lat_lon(bounds.1, bounds.0, 13.0);
                    }
                    self.sidebar_tab = SidebarTab::Routes;
                    self.sidebar_open = true;
                    self.save_config();
                    self.status_banner = Some(("Ruta GPX importada".to_string(), time + 4.0));
                }
                Err(err) => {
                    self.status_banner = Some((err, time + 6.0));
                }
            }
        }
    }

    fn center_on_header(&mut self, h: &crate::pmtiles_reader::PmtilesHeader) {
        if h.has_valid_bounds() {
            self.camera.fit_bounds(
                h.min_lon,
                h.min_lat,
                h.max_lon,
                h.max_lat,
                self.last_map_rect.size(),
            );
        } else {
            self.camera = MapCamera::from_lat_lon(
                h.center_lat,
                h.center_lon,
                (h.center_zoom as f64).max(5.0),
            );
        }
    }

    fn center_on_active_map(&mut self) {
        let target = self
            .maps
            .iter()
            .find(|m| m.enabled)
            .or_else(|| self.maps.first())
            .map(|m| m.archive.header.clone());

        if let Some(h) = target {
            self.center_on_header(&h);
        }
    }

    fn measured_total_km(&self) -> f64 {
        let mut total = 0.0_f64;
        for pair in self.measure_points.windows(2) {
            total += haversine_km(pair[0].0, pair[0].1, pair[1].0, pair[1].1);
        }
        total
    }
}

type HashMapLookup = std::collections::HashMap<String, LoadedMap>;

impl eframe::App for NomadMapsApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let now = ctx.input(|i| i.time);

        if self.tile_manager.poll_results() > 0 {
            ctx.request_repaint();
        }

        if let Some(dirty_t) = self.config_dirty_since {
            if now - dirty_t >= 0.8 {
                self.save_config();
            }
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            self.save_config();
        }

        if let Some((_, until)) = &self.status_banner {
            if now > *until {
                self.status_banner = None;
            }
        }

        let dark_mode = self.config.dark_mode;
        let accent = Color32::from_rgb(38, 152, 88);
        let measure_accent = Color32::from_rgb(0, 152, 232);

        // Top Toolbar
        egui::TopBottomPanel::top("top_toolbar")
            .frame(
                Frame::none()
                    .fill(if dark_mode {
                        Color32::from_rgb(24, 28, 34)
                    } else {
                        Color32::from_rgb(252, 252, 250)
                    })
                    .stroke(Stroke::new(
                        1.0_f32,
                        if dark_mode {
                            Color32::from_rgb(46, 54, 64)
                        } else {
                            Color32::from_rgb(220, 218, 210)
                        },
                    ))
                    .inner_margin(Margin::symmetric(12.0, 8.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(if self.sidebar_open {
                            "Ocultar panel"
                        } else {
                            "Mostrar panel"
                        })
                        .clicked()
                    {
                        self.sidebar_open = !self.sidebar_open;
                    }

                    ui.separator();
                    ui.label(
                        RichText::new("NomadMaps")
                            .strong()
                            .size(16.0)
                            .color(accent),
                    );
                    ui.label(
                        RichText::new("Portable")
                            .small()
                            .color(if dark_mode {
                                Color32::from_rgb(155, 165, 178)
                            } else {
                                Color32::from_rgb(110, 116, 124)
                            }),
                    );

                    ui.separator();

                    if ui.button("+ Añadir mapa (.pmtiles)").clicked() {
                        self.add_pmtiles_via_dialog(now);
                    }

                    if ui.button("+ Importar ruta (.gpx)").clicked() {
                        self.add_gpx_via_dialog(now);
                    }

                    let measure_btn = if self.measure_mode {
                        Button::new(
                            RichText::new("Midiendo distancia (Activo)")
                                .color(Color32::WHITE)
                                .strong(),
                        )
                        .fill(measure_accent)
                    } else {
                        Button::new("Medir distancia")
                    };
                    if ui.add(measure_btn).clicked() {
                        self.measure_mode = !self.measure_mode;
                        self.context_menu = None;
                    }

                    let help_btn = Button::new(
                        RichText::new("? Descargar Mapas / Ayuda")
                            .color(Color32::WHITE)
                            .strong(),
                    )
                    .fill(accent);
                    if ui.add(help_btn).clicked() {
                        self.help_modal.open = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let theme_label = if self.config.dark_mode {
                            "Modo Claro"
                        } else {
                            "Modo Oscuro"
                        };
                        if ui.button(theme_label).clicked() {
                            self.config.dark_mode = !self.config.dark_mode;
                            apply_theme_visuals(ctx, self.config.dark_mode);
                            self.save_config();
                        }
                    });
                });
            });

        // Bottom Status Bar
        egui::TopBottomPanel::bottom("bottom_status_bar")
            .frame(
                Frame::none()
                    .fill(if dark_mode {
                        Color32::from_rgb(24, 28, 34)
                    } else {
                        Color32::from_rgb(248, 247, 243)
                    })
                    .stroke(Stroke::new(
                        1.0_f32,
                        if dark_mode {
                            Color32::from_rgb(46, 54, 64)
                        } else {
                            Color32::from_rgb(220, 218, 210)
                        },
                    ))
                    .inner_margin(Margin::symmetric(12.0, 5.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (lat, lon) = self
                        .cursor_lat_lon
                        .unwrap_or_else(|| self.camera.center_lat_lon());
                    ui.label(
                        RichText::new(format!("Lat: {lat:.5}  |  Lon: {lon:.5}"))
                            .monospace()
                            .small(),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Zoom: {:.1}", self.camera.zoom))
                            .monospace()
                            .small(),
                    );
                    ui.separator();
                    let cache_mb = self.tile_manager.cache_bytes() as f64 / (1024.0 * 1024.0);
                    ui.label(
                        RichText::new(format!("Cache RAM: {cache_mb:.1} / 150 MB"))
                            .monospace()
                            .small(),
                    );

                    let pending = self.tile_manager.in_flight_count();
                    if pending > 0 {
                        ui.separator();
                        ui.spinner();
                        ui.label(RichText::new(format!("Cargando {pending} tiles...")).small());
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("Desarrollado por Jorge Cuevas Ramos")
                                .small()
                                .weak(),
                        );
                        if let Some((msg, _)) = &self.status_banner {
                            ui.separator();
                            ui.label(RichText::new(msg).small().color(accent));
                        }
                    });
                });
            });

        // Collapsible Left Sidebar
        if self.sidebar_open {
            egui::SidePanel::left("sidebar_panel")
                .resizable(true)
                .default_width(295.0)
                .min_width(240.0)
                .max_width(440.0)
                .frame(
                    Frame::none()
                        .fill(if dark_mode {
                            Color32::from_rgb(26, 31, 38)
                        } else {
                            Color32::from_rgb(252, 251, 248)
                        })
                        .stroke(Stroke::new(
                            1.0_f32,
                            if dark_mode {
                                Color32::from_rgb(46, 54, 64)
                            } else {
                                Color32::from_rgb(220, 218, 210)
                            },
                        ))
                        .inner_margin(Margin::same(10.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.sidebar_tab, SidebarTab::Maps, "Mapas");
                        ui.selectable_value(
                            &mut self.sidebar_tab,
                            SidebarTab::Markers,
                            format!("Marcadores ({})", self.marker_store.markers.len()),
                        );
                        ui.selectable_value(
                            &mut self.sidebar_tab,
                            SidebarTab::Routes,
                            format!("Rutas ({})", self.routes.len()),
                        );
                    });
                    ui.separator();

                    match self.sidebar_tab {
                        SidebarTab::Maps => self.ui_sidebar_maps(ui, now),
                        SidebarTab::Markers => self.ui_sidebar_markers(ui),
                        SidebarTab::Routes => self.ui_sidebar_routes(ui, now),
                    }
                });
        }

        // Main Map Canvas
        egui::CentralPanel::default()
            .frame(Frame::none())
            .show(ctx, |ui| {
                let avail = ui.available_size();
                let (response, painter) = ui.allocate_painter(avail, Sense::click_and_drag());
                let map_rect = response.rect;
                self.last_map_rect = map_rect;

                if !ctx.wants_keyboard_input() {
                    let pan_speed = 18.0;
                    let mut kb_pan = Vec2::ZERO;
                    ctx.input(|i| {
                        if i.key_down(egui::Key::ArrowLeft) {
                            kb_pan.x += pan_speed;
                        }
                        if i.key_down(egui::Key::ArrowRight) {
                            kb_pan.x -= pan_speed;
                        }
                        if i.key_down(egui::Key::ArrowUp) {
                            kb_pan.y += pan_speed;
                        }
                        if i.key_down(egui::Key::ArrowDown) {
                            kb_pan.y -= pan_speed;
                        }
                        if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                            self.camera
                                .zoom_at_screen_pos(0.75, map_rect.center(), map_rect);
                            self.mark_dirty(now);
                        }
                        if i.key_pressed(egui::Key::Minus) {
                            self.camera
                                .zoom_at_screen_pos(-0.75, map_rect.center(), map_rect);
                            self.mark_dirty(now);
                        }
                        if self.measure_mode && i.key_pressed(egui::Key::Backspace) {
                            self.measure_points.pop();
                        }
                        if i.key_pressed(egui::Key::Escape) {
                            if self.context_menu.is_some() {
                                self.context_menu = None;
                            } else if self.measure_mode {
                                self.measure_mode = false;
                                self.measure_points.clear();
                            }
                        }
                    });
                    if kb_pan != Vec2::ZERO {
                        self.camera.pan_by_pixels(kb_pan);
                        self.mark_dirty(now);
                        ctx.request_repaint();
                    }
                }

                if response.dragged_by(PointerButton::Primary) {
                    let delta = response.drag_delta();
                    if delta != Vec2::ZERO {
                        self.camera.pan_by_pixels(delta);
                        self.context_menu = None;
                        self.mark_dirty(now);
                    }
                }

                if !self.measure_mode && response.double_clicked_by(PointerButton::Primary) {
                    if let Some(pos) = response.interact_pointer_pos() {
                        self.camera.zoom_at_screen_pos(1.0, pos, map_rect);
                        self.context_menu = None;
                        self.mark_dirty(now);
                    }
                }

                if response.clicked_by(PointerButton::Primary) {
                    self.context_menu = None;
                    if self.measure_mode {
                        if let Some(pos) = response.interact_pointer_pos() {
                            let (lat, lon) = self.camera.screen_to_lat_lon(pos, map_rect);
                            self.measure_points.push((lat, lon));
                        }
                    }
                }

                if response.secondary_clicked() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let (lat, lon) = self.camera.screen_to_lat_lon(pos, map_rect);
                        self.context_menu = Some(ContextMenuState {
                            screen_pos: pos,
                            lat,
                            lon,
                        });
                    }
                }

                if let Some(hover_pos) = response.hover_pos() {
                    self.cursor_lat_lon = Some(self.camera.screen_to_lat_lon(hover_pos, map_rect));
                    let scroll_y = ctx.input(|i| i.smooth_scroll_delta.y);
                    if scroll_y.abs() > 0.01 {
                        let delta_zoom = (scroll_y as f64 * 0.005).clamp(-1.5, 1.5);
                        self.camera
                            .zoom_at_screen_pos(delta_zoom, hover_pos, map_rect);
                        self.context_menu = None;
                        self.mark_dirty(now);
                    }
                } else {
                    self.cursor_lat_lon = None;
                }

                let palette = if self.config.dark_mode {
                    MapPalette::dark()
                } else {
                    MapPalette::light()
                };

                let active_layers: Vec<MapRenderLayer<'_>> = self
                    .maps
                    .iter()
                    .filter(|m| m.enabled)
                    .map(|m| MapRenderLayer {
                        id: m.id,
                        display_name: m.display_name.as_str(),
                        archive: &m.archive,
                    })
                    .collect();

                let measure_cursor = if self.measure_mode {
                    self.cursor_lat_lon
                } else {
                    None
                };

                render_map(
                    &painter,
                    map_rect,
                    &self.camera,
                    &palette,
                    &active_layers,
                    &mut self.tile_manager,
                    &self.marker_store.markers,
                    &self.routes,
                    &self.measure_points,
                    measure_cursor,
                    self.config.show_buildings,
                    self.config.show_labels,
                );

                if active_layers.is_empty() && !self.measure_mode {
                    self.ui_empty_map_overlay(ctx, map_rect, now);
                }

                if self.measure_mode || !self.measure_points.is_empty() {
                    self.ui_measure_hud(ctx, map_rect);
                }

                self.ui_floating_controls(ctx, map_rect, now);
                self.ui_context_menu(ctx);
            });

        self.ui_new_marker_dialog(ctx);
        self.ui_help_modal(ctx, now);
    }
}

impl NomadMapsApp {
    fn ui_sidebar_maps(&mut self, ui: &mut egui::Ui, now: f64) {
        ui.horizontal(|ui| {
            if ui.button("Recargar ./mapas/").clicked() {
                self.scan_maps();
                self.status_banner = Some(("Carpeta ./mapas/ actualizada".to_string(), now + 3.0));
            }
            if ui.button("Añadir archivo...").clicked() {
                self.add_pmtiles_via_dialog(now);
            }
        });

        ui.label(
            RichText::new(format!("Carpeta: {}", self.paths.maps_dir.display()))
                .small()
                .weak(),
        );

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut self.config.show_buildings, "Edificios")
                .changed()
            {
                self.mark_dirty(now);
            }
            if ui
                .checkbox(&mut self.config.show_labels, "Etiquetas")
                .changed()
            {
                self.mark_dirty(now);
            }
        });

        ui.separator();

        if self.maps.is_empty() {
            ui.add_space(12.0);
            ui.label(RichText::new("No se encontraron mapas .pmtiles locales.").strong());
            ui.label(
                "Coloca archivos .pmtiles en la carpeta ./mapas/ junto al ejecutable, o pulsa \"Descargar Mapas / Ayuda\" en la barra superior.",
            );
            return;
        }

        let mut center_target: Option<crate::pmtiles_reader::PmtilesHeader> = None;
        let mut changed = false;

        ui.push_id("maps_scroll_area", |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                for map in &mut self.maps {
                    let dark = self.config.dark_mode;
                    Frame::none()
                        .fill(if dark {
                            Color32::from_rgb(33, 39, 48)
                        } else {
                            Color32::from_rgb(245, 243, 238)
                        })
                        .rounding(Rounding::same(6.0))
                        .stroke(Stroke::new(
                            1.0_f32,
                            if dark {
                                Color32::from_rgb(54, 64, 76)
                            } else {
                                Color32::from_rgb(220, 216, 206)
                            },
                        ))
                        .inner_margin(Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut map.enabled, "").changed() {
                                    changed = true;
                                }
                                ui.label(RichText::new(&map.display_name).strong());
                            });

                            let h = &map.archive.header;
                            ui.label(
                                RichText::new(format!(
                                    "{} | {} | Zoom {}-{}",
                                    map.archive.formatted_size(),
                                    h.tile_type.label(),
                                    h.min_zoom,
                                    h.max_zoom
                                ))
                                .small()
                                .weak(),
                            );
                            if h.has_valid_bounds() {
                                ui.label(
                                    RichText::new(format!(
                                        "Limites: [{:.2}, {:.2}] a [{:.2}, {:.2}]",
                                        h.min_lon, h.min_lat, h.max_lon, h.max_lat
                                    ))
                                    .small()
                                    .weak(),
                                );
                            }
                            if let Some(attr) = &map.archive.metadata.attribution {
                                if !attr.trim().is_empty() {
                                    ui.label(RichText::new(attr).small().weak());
                                }
                            }

                            ui.add_space(3.0);
                            ui.horizontal(|ui| {
                                if ui.small_button("Centrar en mapa").clicked() {
                                    map.enabled = true;
                                    changed = true;
                                    center_target = Some(h.clone());
                                }
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        });

        if let Some(h) = center_target {
            self.center_on_header(&h);
        }
        if changed {
            self.save_config();
        }
    }

    fn ui_sidebar_markers(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Haz clic derecho sobre cualquier punto del mapa para guardar un marcador en ./marcadores.json.")
                .small()
                .weak(),
        );
        ui.separator();

        if self.marker_store.markers.is_empty() {
            ui.add_space(12.0);
            ui.label("Aún no tienes marcadores guardados.");
            return;
        }

        let mut jump_to: Option<(f64, f64)> = None;
        let mut delete_id: Option<u64> = None;

        ui.push_id("markers_scroll_area", |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                for m in &self.marker_store.markers {
                    let dark = self.config.dark_mode;
                    Frame::none()
                        .fill(if dark {
                            Color32::from_rgb(33, 39, 48)
                        } else {
                            Color32::from_rgb(245, 243, 238)
                        })
                        .rounding(Rounding::same(6.0))
                        .inner_margin(Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                                ui.painter().circle_filled(rect.center(), 5.5, m.color32());
                                ui.label(RichText::new(&m.name).strong());
                            });

                            ui.label(
                                RichText::new(format!("{:.5}, {:.5}", m.lat, m.lon))
                                    .monospace()
                                    .small()
                                    .weak(),
                            );
                            if !m.notes.trim().is_empty() {
                                ui.label(RichText::new(&m.notes).small());
                            }

                            ui.horizontal(|ui| {
                                if ui.small_button("Ir al punto").clicked() {
                                    jump_to = Some((m.lat, m.lon));
                                }
                                if ui.small_button("Eliminar").clicked() {
                                    delete_id = Some(m.id);
                                }
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        });

        if let Some((lat, lon)) = jump_to {
            self.camera = MapCamera::from_lat_lon(lat, lon, self.camera.zoom.max(15.0));
            self.save_config();
        }
        if let Some(id) = delete_id {
            self.marker_store
                .remove_marker(&self.paths.markers_file, id);
        }
    }

    fn ui_sidebar_routes(&mut self, ui: &mut egui::Ui, now: f64) {
        if ui.button("+ Importar ruta (.gpx)").clicked() {
            self.add_gpx_via_dialog(now);
        }
        ui.separator();

        if self.routes.is_empty() {
            ui.add_space(12.0);
            ui.label("No hay rutas GPX cargadas.");
            return;
        }

        let mut center_bounds: Option<(f64, f64, f64, f64)> = None;
        let mut remove_id: Option<u64> = None;

        ui.push_id("routes_scroll_area", |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                for route in &mut self.routes {
                    let dark = self.config.dark_mode;
                    Frame::none()
                        .fill(if dark {
                            Color32::from_rgb(33, 39, 48)
                        } else {
                            Color32::from_rgb(245, 243, 238)
                        })
                        .rounding(Rounding::same(6.0))
                        .inner_margin(Margin::same(8.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.checkbox(&mut route.visible, "");
                                let (rect, _) =
                                    ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                                ui.painter().circle_filled(rect.center(), 4.5, route.color32());
                                ui.label(RichText::new(&route.name).strong());
                            });

                            ui.label(
                                RichText::new(format!(
                                    "Distancia: {:.2} km | Waypoints: {}",
                                    route.distance_km,
                                    route.waypoints.len()
                                ))
                                .small()
                                .weak(),
                            );

                            ui.horizontal(|ui| {
                                if ui.small_button("Centrar").clicked() {
                                    route.visible = true;
                                    center_bounds = Some(route.bounds);
                                }
                                if ui.small_button("Quitar").clicked() {
                                    remove_id = Some(route.id);
                                }
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        });

        if let Some(b) = center_bounds {
            if b.2 > b.0 && b.3 > b.1 {
                self.camera
                    .fit_bounds(b.0, b.1, b.2, b.3, self.last_map_rect.size());
            } else {
                self.camera = MapCamera::from_lat_lon(b.1, b.0, 14.0);
            }
            self.save_config();
        }
        if let Some(id) = remove_id {
            self.routes.retain(|r| r.id != id);
            self.save_config();
        }
    }

    fn ui_measure_hud(&mut self, ctx: &Context, map_rect: egui::Rect) {
        let dark = self.config.dark_mode;
        let measure_accent = Color32::from_rgb(0, 152, 232);

        let total_km = self.measured_total_km();
        let preview_km = match (self.measure_points.last(), self.cursor_lat_lon) {
            (Some(&(lat, lon)), Some((clat, clon))) if self.measure_mode => {
                Some(haversine_km(lat, lon, clat, clon))
            }
            _ => None,
        };

        let pos = Pos2::new(
            map_rect.left() + 16.0,
            (map_rect.bottom() - 122.0).max(map_rect.top() + 16.0),
        );

        egui::Area::new(egui::Id::new("measure_hud"))
            .fixed_pos(pos)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(if dark {
                        Color32::from_rgba_unmultiplied(26, 32, 40, 240)
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 245)
                    })
                    .rounding(Rounding::same(10.0))
                    .stroke(Stroke::new(1.5_f32, measure_accent))
                    .inner_margin(Margin::same(10.0))
                    .show(ui, |ui| {
                        ui.set_min_width(270.0);
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Medicion de distancias")
                                    .strong()
                                    .color(measure_accent),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "Puntos: {}",
                                            self.measure_points.len()
                                        ))
                                        .small()
                                        .weak(),
                                    );
                                },
                            );
                        });

                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(format!("Total: {}", format_distance_km(total_km)))
                                .strong()
                                .size(17.0),
                        );

                        if let Some(p_km) = preview_km {
                            ui.label(
                                RichText::new(format!(
                                    "Al cursor: +{} (Total: {})",
                                    format_distance_km(p_km),
                                    format_distance_km(total_km + p_km)
                                ))
                                .small()
                                .weak(),
                            );
                        } else if self.measure_mode {
                            ui.label(
                                RichText::new("Haz clic en el mapa para añadir puntos.")
                                    .small()
                                    .weak(),
                            );
                        }

                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui
                                .add_enabled(
                                    !self.measure_points.is_empty(),
                                    Button::new("Deshacer"),
                                )
                                .clicked()
                            {
                                self.measure_points.pop();
                            }
                            if ui
                                .add_enabled(
                                    !self.measure_points.is_empty(),
                                    Button::new("Limpiar"),
                                )
                                .clicked()
                            {
                                self.measure_points.clear();
                            }
                            if ui.button("Cerrar").clicked() {
                                self.measure_mode = false;
                                self.measure_points.clear();
                            }
                        });
                    });
            });
    }

    fn ui_empty_map_overlay(&mut self, ctx: &Context, map_rect: egui::Rect, now: f64) {
        let dark = self.config.dark_mode;
        egui::Area::new(egui::Id::new("empty_map_overlay"))
            .fixed_pos(Pos2::new(
                map_rect.center().x - 210.0,
                map_rect.top() + 28.0,
            ))
            .show(ctx, |ui| {
                Frame::none()
                    .fill(if dark {
                        Color32::from_rgba_unmultiplied(28, 34, 42, 235)
                    } else {
                        Color32::from_rgba_unmultiplied(255, 255, 255, 240)
                    })
                    .rounding(Rounding::same(10.0))
                    .stroke(Stroke::new(
                        1.0_f32,
                        if dark {
                            Color32::from_rgb(66, 76, 90)
                        } else {
                            Color32::from_rgb(206, 212, 218)
                        },
                    ))
                    .inner_margin(Margin::same(14.0))
                    .show(ui, |ui| {
                        ui.set_width(390.0);
                        ui.label(
                            RichText::new("Sin mapas activos en pantalla")
                                .strong()
                                .size(15.0),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            "Añade o activa un mapa vectorial (.pmtiles) desde el panel izquierdo o descarga mapas offline gratuitos paso a paso.",
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if ui.button("+ Añadir mapa (.pmtiles)").clicked() {
                                self.add_pmtiles_via_dialog(now);
                            }
                            if ui
                                .add(
                                    Button::new(
                                        RichText::new("Como descargar mapas")
                                            .color(Color32::WHITE),
                                    )
                                    .fill(Color32::from_rgb(38, 152, 88)),
                                )
                                .clicked()
                            {
                                self.help_modal.open = true;
                            }
                        });
                    });
            });
    }

    fn ui_floating_controls(&mut self, ctx: &Context, map_rect: egui::Rect, now: f64) {
        let dark = self.config.dark_mode;
        let bg = if dark {
            Color32::from_rgba_unmultiplied(30, 36, 44, 235)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 240)
        };
        let border = if dark {
            Color32::from_rgb(64, 74, 88)
        } else {
            Color32::from_rgb(206, 212, 218)
        };

        let anchor_pos = Pos2::new(map_rect.right() - 62.0, map_rect.top() + 18.0);
        egui::Area::new(egui::Id::new("floating_map_controls"))
            .fixed_pos(anchor_pos)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(bg)
                    .rounding(Rounding::same(10.0))
                    .stroke(Stroke::new(1.0_f32, border))
                    .inner_margin(Margin::same(6.0))
                    .show(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            let btn_size = Vec2::new(36.0, 32.0);
                            if ui
                                .add_sized(btn_size, Button::new(RichText::new("+").size(18.0)))
                                .on_hover_text("Acercar")
                                .clicked()
                            {
                                self.camera
                                    .zoom_at_screen_pos(1.0, map_rect.center(), map_rect);
                                self.mark_dirty(now);
                            }
                            if ui
                                .add_sized(btn_size, Button::new(RichText::new("-").size(18.0)))
                                .on_hover_text("Alejar")
                                .clicked()
                            {
                                self.camera
                                    .zoom_at_screen_pos(-1.0, map_rect.center(), map_rect);
                                self.mark_dirty(now);
                            }
                            ui.separator();
                            if ui
                                .add_sized(btn_size, Button::new(RichText::new("◎").size(15.0)))
                                .on_hover_text("Centrar en el mapa activo")
                                .clicked()
                            {
                                self.center_on_active_map();
                                self.save_config();
                            }
                            let theme_icon = if self.config.dark_mode { "L" } else { "D" };
                            let theme_tip = if self.config.dark_mode {
                                "Cambiar a Modo Claro"
                            } else {
                                "Cambiar a Modo Oscuro"
                            };
                            if ui
                                .add_sized(btn_size, Button::new(RichText::new(theme_icon).strong()))
                                .on_hover_text(theme_tip)
                                .clicked()
                            {
                                self.config.dark_mode = !self.config.dark_mode;
                                apply_theme_visuals(ctx, self.config.dark_mode);
                                self.save_config();
                            }
                        });
                    });
            });
    }

    fn ui_context_menu(&mut self, ctx: &Context) {
        let Some(menu) = self.context_menu.as_ref() else {
            return;
        };
        let screen_pos = menu.screen_pos;
        let lat = menu.lat;
        let lon = menu.lon;

        let mut close_menu = false;
        let mut open_marker_dialog = false;
        let mut start_measure = false;

        let dark = self.config.dark_mode;
        egui::Area::new(egui::Id::new("map_context_menu"))
            .fixed_pos(screen_pos)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(if dark {
                        Color32::from_rgb(32, 38, 46)
                    } else {
                        Color32::WHITE
                    })
                    .rounding(Rounding::same(8.0))
                    .stroke(Stroke::new(
                        1.0_f32,
                        if dark {
                            Color32::from_rgb(70, 80, 94)
                        } else {
                            Color32::from_rgb(200, 206, 214)
                        },
                    ))
                    .inner_margin(Margin::same(8.0))
                    .show(ui, |ui| {
                        ui.set_min_width(210.0);
                        ui.label(
                            RichText::new(format!("{lat:.5}, {lon:.5}"))
                                .monospace()
                                .small()
                                .weak(),
                        );
                        ui.separator();
                        if ui.button("Guardar marcador aquí").clicked() {
                            open_marker_dialog = true;
                            close_menu = true;
                        }
                        if ui.button("Medir distancia desde aquí").clicked() {
                            start_measure = true;
                            close_menu = true;
                        }
                        if ui.button("Copiar coordenadas").clicked() {
                            ctx.copy_text(format!("{lat:.6}, {lon:.6}"));
                            close_menu = true;
                        }
                        if ui.button("Cerrar").clicked() {
                            close_menu = true;
                        }
                    });
            });

        if start_measure {
            self.measure_mode = true;
            self.measure_points.push((lat, lon));
        }
        if open_marker_dialog {
            self.new_marker = NewMarkerDialog {
                open: true,
                name: format!("Marcador {}", self.marker_store.markers.len() + 1),
                notes: String::new(),
                lat,
                lon,
                color_idx: 0,
            };
        }
        if close_menu {
            self.context_menu = None;
        }
    }

    fn ui_new_marker_dialog(&mut self, ctx: &Context) {
        if !self.new_marker.open {
            return;
        }
        let mut open = self.new_marker.open;
        let mut save_clicked = false;
        let mut cancel_clicked = false;

        egui::Window::new("Nuevo marcador")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(
                    RichText::new(format!(
                        "Ubicación: {:.6}, {:.6}",
                        self.new_marker.lat, self.new_marker.lon
                    ))
                    .monospace()
                    .small(),
                );
                ui.add_space(6.0);

                ui.label("Nombre:");
                ui.text_edit_singleline(&mut self.new_marker.name);
                ui.add_space(6.0);

                ui.label("Color:");
                ui.horizontal_wrapped(|ui| {
                    for (idx, (label, rgb)) in MARKER_COLOR_PRESETS.iter().enumerate() {
                        let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                        let selected = self.new_marker.color_idx == idx;
                        let btn = Button::new(RichText::new(*label).color(Color32::WHITE).small())
                            .fill(color)
                            .stroke(if selected {
                                Stroke::new(2.0_f32, Color32::BLACK)
                            } else {
                                Stroke::NONE
                            });
                        if ui.add(btn).clicked() {
                            self.new_marker.color_idx = idx;
                        }
                    }
                });
                ui.add_space(6.0);

                ui.label("Notas (opcional):");
                ui.text_edit_singleline(&mut self.new_marker.notes);
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui
                        .add(
                            Button::new(RichText::new("Guardar marcador").color(Color32::WHITE))
                                .fill(Color32::from_rgb(38, 152, 88)),
                        )
                        .clicked()
                    {
                        save_clicked = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        cancel_clicked = true;
                    }
                });
            });

        if save_clicked {
            let clean_name = self.new_marker.name.trim();
            let final_name = if clean_name.is_empty() {
                "Marcador".to_string()
            } else {
                clean_name.to_string()
            };
            let color = MARKER_COLOR_PRESETS[self.new_marker.color_idx].1;
            self.marker_store.add_marker(
                &self.paths.markers_file,
                final_name,
                self.new_marker.lat,
                self.new_marker.lon,
                color,
                self.new_marker.notes.trim().to_string(),
            );
            self.sidebar_tab = SidebarTab::Markers;
            open = false;
        } else if cancel_clicked {
            open = false;
        }

        self.new_marker.open = open;
    }

    fn ui_help_modal(&mut self, ctx: &Context, now: f64) {
        if !self.help_modal.open {
            return;
        }

        let mut open = self.help_modal.open;
        let accent = Color32::from_rgb(38, 152, 88);
        let dark = self.config.dark_mode;

        egui::Window::new("Como descargar mapas offline (.pmtiles)")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_size([700.0, 560.0])
            .show(ctx, |ui| {
                ScrollArea::vertical().show(ui, |ui| {
                    ui.label(
                        RichText::new("NomadMaps Portable — Desarrollado por Jorge Cuevas Ramos")
                            .strong()
                            .color(accent),
                    );
                    ui.add_space(6.0);

                    ui.label(RichText::new("Carpeta portable de mapas detectada:").strong());
                    let maps_path_str = self.paths.maps_dir.display().to_string();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&maps_path_str).monospace().small());
                        if ui.small_button("Copiar ruta").clicked() {
                            ctx.copy_text(maps_path_str.clone());
                            self.help_modal.copied_path_until = now + 2.5;
                        }
                        if now < self.help_modal.copied_path_until {
                            ui.label(RichText::new("Ruta copiada").small().color(accent));
                        }
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    ui.label(
                        RichText::new(
                            "Método 1 (Directo con un clic): Países enteros listos para usar",
                        )
                        .strong()
                        .size(15.0)
                        .color(accent),
                    );
                    ui.add_space(4.0);
                    ui.label("1. Abre tu navegador web y entra en:");
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("https://download.tiles.coalition.space/bundles")
                                .monospace(),
                        );
                        if ui.small_button("Copiar URL").clicked() {
                            ctx.copy_text(
                                "https://download.tiles.coalition.space/bundles".to_string(),
                            );
                        }
                    });
                    ui.label("2. Elige tu país o región y descarga el archivo .pmtiles.");
                    ui.label("3. Copia el archivo .pmtiles dentro de la carpeta ./mapas/ situada junto a NomadMaps.exe (o ábrelo con el botón \"+ Añadir mapa (.pmtiles)\").");
                    ui.label("4. Pulsa \"Recargar ./mapas/\" en el panel izquierdo y actívalo.");

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(6.0);

                    ui.label(
                        RichText::new(
                            "Método 2 (A medida con pmtiles.exe): Recortar solo tu ciudad o zona",
                        )
                        .strong()
                        .size(15.0)
                        .color(accent),
                    );
                    ui.add_space(4.0);
                    ui.label("1. Descarga el ejecutable pmtiles.exe para Windows desde:");
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("https://github.com/protomaps/go-pmtiles/releases")
                                .monospace(),
                        );
                        if ui.small_button("Copiar URL").clicked() {
                            ctx.copy_text(
                                "https://github.com/protomaps/go-pmtiles/releases".to_string(),
                            );
                        }
                    });
                    ui.label("2. Entra en https://bboxfinder.com, dibuja un rectángulo sobre tu zona y anota las 4 coordenadas inferiores (Oeste, Sur, Este, Norte).");
                    ui.label("3. Consulta la fecha del último build disponible en https://maps.protomaps.com/builds/ (formato AAAAMMDD, por ejemplo 20250201).");
                    ui.add_space(8.0);

                    Frame::none()
                        .fill(if dark {
                            Color32::from_rgb(33, 39, 48)
                        } else {
                            Color32::from_rgb(244, 243, 238)
                        })
                        .rounding(Rounding::same(8.0))
                        .stroke(Stroke::new(
                            1.0_f32,
                            if dark {
                                Color32::from_rgb(60, 70, 84)
                            } else {
                                Color32::from_rgb(214, 210, 202)
                            },
                        ))
                        .inner_margin(Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Generador interactivo de comando pmtiles.exe")
                                        .strong(),
                                );
                                if ui.small_button("Usar vista actual del mapa").clicked() {
                                    let (w, s, e, n) =
                                        self.camera.visible_bounds_lat_lon(self.last_map_rect);
                                    self.help_modal.west = format!("{w:.4}");
                                    self.help_modal.south = format!("{s:.4}");
                                    self.help_modal.east = format!("{e:.4}");
                                    self.help_modal.north = format!("{n:.4}");
                                }
                            });

                            ui.add_space(6.0);
                            egui::Grid::new("pmtiles_cmd_grid")
                                .num_columns(4)
                                .spacing([10.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label("Oeste (Min Lon):");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.help_modal.west)
                                            .desired_width(105.0),
                                    );
                                    ui.label("Sur (Min Lat):");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.help_modal.south)
                                            .desired_width(105.0),
                                    );
                                    ui.end_row();

                                    ui.label("Este (Max Lon):");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.help_modal.east)
                                            .desired_width(105.0),
                                    );
                                    ui.label("Norte (Max Lat):");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.help_modal.north)
                                            .desired_width(105.0),
                                    );
                                    ui.end_row();

                                    ui.label("Fecha build:");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.help_modal.build_date)
                                            .desired_width(105.0),
                                    );
                                    ui.label("Archivo salida:");
                                    ui.add(
                                        egui::TextEdit::singleline(
                                            &mut self.help_modal.output_name,
                                        )
                                        .desired_width(140.0),
                                    );
                                    ui.end_row();
                                });

                            let w_v = self.help_modal.west.trim().parse::<f64>();
                            let s_v = self.help_modal.south.trim().parse::<f64>();
                            let e_v = self.help_modal.east.trim().parse::<f64>();
                            let n_v = self.help_modal.north.trim().parse::<f64>();

                            let mut validation_err: Option<&str> = None;
                            if let (Ok(w), Ok(s), Ok(e), Ok(n)) = (&w_v, &s_v, &e_v, &n_v) {
                                if w >= e {
                                    validation_err = Some("Aviso: Oeste debe ser menor que Este.");
                                } else if s >= n {
                                    validation_err = Some("Aviso: Sur debe ser menor que Norte.");
                                }
                            } else {
                                validation_err =
                                    Some("Introduce valores numéricos válidos con punto decimal.");
                            }

                            if let Some(err) = validation_err {
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(err)
                                        .small()
                                        .color(Color32::from_rgb(220, 120, 40)),
                                );
                            }

                            let date_clean = self.help_modal.build_date.trim();
                            let out_clean = {
                                let raw = self.help_modal.output_name.trim();
                                if raw.is_empty() {
                                    "mi_zona.pmtiles".to_string()
                                } else if raw.ends_with(".pmtiles") {
                                    raw.to_string()
                                } else {
                                    format!("{raw}.pmtiles")
                                }
                            };
                            let cmd = format!(
                                "pmtiles.exe extract https://build.protomaps.com/{}.pmtiles {} --bbox={},{},{},{}",
                                if date_clean.is_empty() {
                                    "20250201"
                                } else {
                                    date_clean
                                },
                                out_clean,
                                self.help_modal.west.trim(),
                                self.help_modal.south.trim(),
                                self.help_modal.east.trim(),
                                self.help_modal.north.trim()
                            );

                            ui.add_space(6.0);
                            ui.label(RichText::new("Comando generado:").small().weak());
                            let mut cmd_view = cmd.as_str();
                            ui.add(
                                egui::TextEdit::multiline(&mut cmd_view)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY),
                            );

                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if ui
                                    .add(
                                        Button::new(
                                            RichText::new("Copiar comando")
                                                .color(Color32::WHITE)
                                                .strong(),
                                        )
                                        .fill(accent),
                                    )
                                    .clicked()
                                {
                                    ctx.copy_text(cmd);
                                    self.help_modal.copied_until = now + 2.5;
                                }
                                if now < self.help_modal.copied_until {
                                    ui.label(
                                        RichText::new("Comando copiado al portapapeles")
                                            .color(accent)
                                            .strong(),
                                    );
                                }
                            });
                        });
                });
            });

        self.help_modal.open = open;
    }
}

fn apply_theme_visuals(ctx: &Context, dark_mode: bool) {
    if dark_mode {
        ctx.set_visuals(egui::Visuals::dark());
    } else {
        ctx.set_visuals(egui::Visuals::light());
    }
}
