use crate::gpx::GpxRoute;
use crate::map_math::{haversine_km, tile_bounds_lat_lon, MapCamera};
use crate::markers::MapMarker;
use crate::mvt::DecodedTile;
use crate::pmtiles_reader::PmTilesArchive;
use crate::style::{road_widths, AreaKind, BoundaryKind, LabelKind, MapPalette, RoadKind};
use crate::tile_manager::{TileLookup, TileManager};
use egui::{Align2, Color32, FontId, Mesh, Painter, Pos2, Rect, Rounding, Shape, Stroke, Vec2};
use std::sync::Arc;

pub struct MapRenderLayer<'a> {
    pub id: usize,
    pub display_name: &'a str,
    pub archive: &'a Arc<PmTilesArchive>,
}

struct RenderTileItem {
    tile_rect: Rect,
    draw_origin: Pos2,
    draw_size: f32,
    tile: Arc<DecodedTile>,
}

pub fn format_distance_km(km: f64) -> String {
    if km < 1.0 {
        format!("{:.0} m", km * 1000.0)
    } else if km < 100.0 {
        format!("{km:.2} km")
    } else {
        format!("{km:.1} km")
    }
}

pub fn render_map(
    painter: &Painter,
    map_rect: Rect,
    camera: &MapCamera,
    palette: &MapPalette,
    maps: &[MapRenderLayer<'_>],
    tile_manager: &mut TileManager,
    markers: &[MapMarker],
    routes: &[GpxRoute],
    measure_points: &[(f64, f64)],
    measure_cursor: Option<(f64, f64)>,
    show_buildings: bool,
    show_labels: bool,
) {
    let map_painter = painter.with_clip_rect(map_rect);
    map_painter.rect_filled(map_rect, Rounding::ZERO, palette.land_bg);

    draw_subtle_grid(&map_painter, map_rect, camera, palette);

    let world_px = camera.world_px();
    let mut render_tiles: Vec<RenderTileItem> = Vec::new();
    let mut out_of_zoom_boxes: Vec<(Rect, String)> = Vec::new();

    for map_layer in maps {
        let header = &map_layer.archive.header;
        if (camera.zoom + 2.0) < header.min_zoom as f64 && header.has_valid_bounds() {
            let tl = camera.lat_lon_to_screen(header.max_lat, header.min_lon, map_rect);
            let br = camera.lat_lon_to_screen(header.min_lat, header.max_lon, map_rect);
            let bbox_rect = Rect::from_two_pos(tl, br);
            if bbox_rect.intersects(map_rect) {
                out_of_zoom_boxes.push((
                    bbox_rect,
                    format!(
                        "{} (Zoom {}-{})",
                        map_layer.display_name, header.min_zoom, header.max_zoom
                    ),
                ));
            }
            continue;
        }

        let target_z = (camera.zoom.floor() as i32)
            .clamp(header.min_zoom as i32, header.max_zoom as i32) as u8;
        let visible_coords = camera.visible_tiles(map_rect, target_z);

        for (z, x, y) in visible_coords {
            let (t_min_lon, t_min_lat, t_max_lon, t_max_lat) = tile_bounds_lat_lon(z, x, y);
            if !map_layer
                .archive
                .covers_bounds(t_min_lon, t_min_lat, t_max_lon, t_max_lat)
            {
                continue;
            }

            let tile_rect = tile_screen_rect(z, x, y, camera, world_px, map_rect);
            match tile_manager.get_or_parent(map_layer.id, z, x, y) {
                TileLookup::Ready(Some(tile)) => {
                    render_tiles.push(RenderTileItem {
                        tile_rect,
                        draw_origin: tile_rect.min,
                        draw_size: tile_rect.width(),
                        tile,
                    });
                }
                TileLookup::Ready(None) => {}
                TileLookup::Parent {
                    z: pz,
                    x: px,
                    y: py,
                    tile,
                } => {
                    tile_manager.request_tile(map_layer.id, map_layer.archive, z, x, y);
                    let parent_rect = tile_screen_rect(pz, px, py, camera, world_px, map_rect);
                    render_tiles.push(RenderTileItem {
                        tile_rect,
                        draw_origin: parent_rect.min,
                        draw_size: parent_rect.width(),
                        tile,
                    });
                }
                TileLookup::Missing => {
                    tile_manager.request_tile(map_layer.id, map_layer.archive, z, x, y);
                }
            }
        }
    }

    // Pass 1: Area polygons (land, parks, forests, landuse, water) clipped strictly per tile
    for item in &render_tiles {
        let clip = item.tile_rect.intersect(map_rect);
        if !clip.is_positive() {
            continue;
        }
        let tile_painter = map_painter.with_clip_rect(clip);
        let mut mesh = Mesh::default();
        for poly in &item.tile.polygons {
            if poly.kind == AreaKind::Building {
                continue;
            }
            let color = palette.area_color(poly.kind);
            append_polygon_to_mesh(&mut mesh, poly, item.draw_origin, item.draw_size, color);
        }
        if !mesh.vertices.is_empty() && !mesh.indices.is_empty() {
            tile_painter.add(Shape::Mesh(mesh));
        }
    }

    // Pass 2: Waterways (streams, rivers, canals)
    for item in &render_tiles {
        let (_, water_w) = road_widths(RoadKind::Waterway, camera.zoom);
        for line in &item.tile.lines {
            if line.kind == RoadKind::Waterway {
                draw_polyline(
                    &map_painter,
                    &line.points,
                    item.draw_origin,
                    item.draw_size,
                    Stroke::new(water_w, palette.water),
                );
            }
        }
    }

    // Pass 3: Buildings (zoom >= 12.5)
    if show_buildings && camera.zoom >= 12.5 {
        for item in &render_tiles {
            let clip = item.tile_rect.intersect(map_rect);
            if !clip.is_positive() {
                continue;
            }
            let tile_painter = map_painter.with_clip_rect(clip);
            let mut mesh = Mesh::default();
            for poly in &item.tile.polygons {
                if poly.kind == AreaKind::Building {
                    append_polygon_to_mesh(
                        &mut mesh,
                        poly,
                        item.draw_origin,
                        item.draw_size,
                        palette.building_fill,
                    );
                }
            }
            if !mesh.vertices.is_empty() && !mesh.indices.is_empty() {
                tile_painter.add(Shape::Mesh(mesh));
            }
        }
    }

    // Pass 4: Administrative Boundaries
    for item in &render_tiles {
        for boundary in &item.tile.boundaries {
            let (width, color) = match boundary.kind {
                BoundaryKind::Country => (1.8_f32, palette.boundary_country),
                BoundaryKind::Region => {
                    if camera.zoom < 5.0 {
                        continue;
                    }
                    (1.0_f32, palette.boundary_region)
                }
            };
            draw_polyline(
                &map_painter,
                &boundary.points,
                item.draw_origin,
                item.draw_size,
                Stroke::new(width, color),
            );
        }
    }

    // Pass 5: Road Casings + Rails/Ferries
    for item in &render_tiles {
        for line in &item.tile.lines {
            if line.kind == RoadKind::Waterway || !is_road_visible(line.kind, camera.zoom) {
                continue;
            }
            let (casing_w, fill_w) = road_widths(line.kind, camera.zoom);
            let (casing_c, fill_c) = palette.road_colors(line.kind);
            if casing_w > 0.0 && casing_c != Color32::TRANSPARENT {
                let mut c = casing_c;
                if line.is_tunnel {
                    c = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 145);
                }
                draw_polyline(
                    &map_painter,
                    &line.points,
                    item.draw_origin,
                    item.draw_size,
                    Stroke::new(casing_w, c),
                );
            } else if line.kind == RoadKind::Rail
                || line.kind == RoadKind::Ferry
                || line.kind == RoadKind::Path
            {
                draw_polyline(
                    &map_painter,
                    &line.points,
                    item.draw_origin,
                    item.draw_size,
                    Stroke::new(fill_w, fill_c),
                );
            }
        }
    }

    // Pass 6: Road Fills (non-bridges then bridges)
    for bridge_pass in [false, true] {
        for item in &render_tiles {
            for line in &item.tile.lines {
                if line.is_bridge != bridge_pass {
                    continue;
                }
                if line.kind == RoadKind::Waterway
                    || line.kind == RoadKind::Rail
                    || line.kind == RoadKind::Ferry
                    || line.kind == RoadKind::Path
                    || !is_road_visible(line.kind, camera.zoom)
                {
                    continue;
                }
                let (_, fill_w) = road_widths(line.kind, camera.zoom);
                let (_, mut fill_c) = palette.road_colors(line.kind);
                if line.is_tunnel {
                    fill_c =
                        Color32::from_rgba_unmultiplied(fill_c.r(), fill_c.g(), fill_c.b(), 150);
                }
                draw_polyline(
                    &map_painter,
                    &line.points,
                    item.draw_origin,
                    item.draw_size,
                    Stroke::new(fill_w, fill_c),
                );
            }
        }
    }

    // Coverage bounding boxes when zoomed far out of regional extracts
    for (bbox, label) in out_of_zoom_boxes {
        let stroke_c = Color32::from_rgb(46, 134, 222);
        let fill_c = Color32::from_rgba_unmultiplied(46, 134, 222, 35);
        map_painter.rect_filled(bbox, Rounding::same(4.0), fill_c);
        map_painter.rect_stroke(bbox, Rounding::same(4.0), Stroke::new(1.5_f32, stroke_c));
        map_painter.text(
            bbox.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(12.0),
            palette.label_city,
        );
    }

    // Pass 7: GPX Routes
    draw_gpx_routes(&map_painter, map_rect, camera, routes, palette);

    // Pass 8: Vector Tile Labels with screen-space collision avoidance
    if show_labels {
        draw_tile_labels(&map_painter, map_rect, &render_tiles, camera.zoom, palette);
    }

    // Pass 9: User Markers
    draw_markers(&map_painter, map_rect, camera, markers, palette);

    // Pass 10: Multi-point Distance Measurement Overlay
    if !measure_points.is_empty() {
        draw_measure_overlay(
            &map_painter,
            map_rect,
            camera,
            measure_points,
            measure_cursor,
            palette,
        );
    }
}

fn tile_screen_rect(
    z: u8,
    x: u32,
    y: u32,
    camera: &MapCamera,
    world_px: f64,
    map_rect: Rect,
) -> Rect {
    let n = (1u64 << (z as u32)) as f64;
    let nx = x as f64 / n;
    let ny = y as f64 / n;
    let px_size = (world_px / n) as f32;
    let origin = Pos2::new(
        (map_rect.center().x as f64 + (nx - camera.center_nx) * world_px) as f32,
        (map_rect.center().y as f64 + (ny - camera.center_ny) * world_px) as f32,
    );
    Rect::from_min_size(origin, Vec2::splat(px_size))
}

fn draw_subtle_grid(painter: &Painter, map_rect: Rect, camera: &MapCamera, palette: &MapPalette) {
    let grid_color = if palette.dark_mode {
        Color32::from_rgba_unmultiplied(255, 255, 255, 10)
    } else {
        Color32::from_rgba_unmultiplied(0, 0, 0, 12)
    };
    let stroke = Stroke::new(1.0_f32, grid_color);
    let step_deg = if camera.zoom < 4.0 {
        30.0
    } else if camera.zoom < 7.0 {
        10.0
    } else if camera.zoom < 10.0 {
        2.0
    } else {
        0.5
    };

    let (west, south, east, north) = camera.visible_bounds_lat_lon(map_rect);
    let mut lon = (west / step_deg).floor() * step_deg;
    while lon <= east + step_deg {
        if lon >= -180.0 && lon <= 180.0 {
            let p1 = camera.lat_lon_to_screen(north.min(85.0), lon, map_rect);
            let p2 = camera.lat_lon_to_screen(south.max(-85.0), lon, map_rect);
            painter.line_segment([p1, p2], stroke);
        }
        lon += step_deg;
    }

    let mut lat = (south / step_deg).floor() * step_deg;
    while lat <= north + step_deg {
        if lat >= -85.0 && lat <= 85.0 {
            let p1 = camera.lat_lon_to_screen(lat, west.max(-180.0), map_rect);
            let p2 = camera.lat_lon_to_screen(lat, east.min(180.0), map_rect);
            painter.line_segment([p1, p2], stroke);
        }
        lat += step_deg;
    }
}

fn append_polygon_to_mesh(
    mesh: &mut Mesh,
    poly: &crate::mvt::DecodedPolygon,
    origin: Pos2,
    size: f32,
    color: Color32,
) {
    let base_idx = mesh.vertices.len() as u32;
    for v in &poly.vertices {
        let pos = Pos2::new(origin.x + v[0] * size, origin.y + v[1] * size);
        mesh.colored_vertex(pos, color);
    }
    for tri in poly.indices.chunks_exact(3) {
        mesh.add_triangle(base_idx + tri[0], base_idx + tri[1], base_idx + tri[2]);
    }
}

fn draw_polyline(
    painter: &Painter,
    pts: &[[f32; 2]],
    origin: Pos2,
    size: f32,
    stroke: Stroke,
) {
    if pts.len() < 2 {
        return;
    }
    let screen_pts: Vec<Pos2> = pts
        .iter()
        .map(|p| Pos2::new(origin.x + p[0] * size, origin.y + p[1] * size))
        .collect();
    painter.add(Shape::line(screen_pts, stroke));
}

fn is_road_visible(kind: RoadKind, zoom: f64) -> bool {
    match kind {
        RoadKind::Motorway | RoadKind::Trunk => zoom >= 5.0,
        RoadKind::Primary => zoom >= 7.0,
        RoadKind::Secondary => zoom >= 9.0,
        RoadKind::Tertiary | RoadKind::Rail | RoadKind::Ferry => zoom >= 10.5,
        RoadKind::Residential => zoom >= 12.0,
        RoadKind::Service | RoadKind::Path => zoom >= 13.5,
        RoadKind::Waterway => zoom >= 9.0,
    }
}

fn draw_tile_labels(
    painter: &Painter,
    map_rect: Rect,
    tiles: &[RenderTileItem],
    zoom: f64,
    palette: &MapPalette,
) {
    struct LabelCandidate<'a> {
        priority: i32,
        pos: Pos2,
        text: &'a str,
        font_size: f32,
        color: Color32,
    }

    let mut candidates: Vec<LabelCandidate<'_>> = Vec::new();
    for item in tiles {
        for lb in &item.tile.labels {
            let pos = Pos2::new(
                item.draw_origin.x + lb.pos[0] * item.draw_size,
                item.draw_origin.y + lb.pos[1] * item.draw_size,
            );
            if !map_rect.contains(pos) {
                continue;
            }
            let (font_size, color) = match lb.kind {
                LabelKind::Country => (14.5, palette.label_city),
                LabelKind::City => (14.0, palette.label_city),
                LabelKind::Town => (12.5, palette.label_city),
                LabelKind::Suburb => {
                    if zoom < 11.5 {
                        continue;
                    }
                    (11.5, palette.label_city)
                }
                LabelKind::Village => {
                    if zoom < 10.0 {
                        continue;
                    }
                    (11.5, palette.label_city)
                }
                LabelKind::Street => {
                    if zoom < 12.8 {
                        continue;
                    }
                    (11.0, palette.label_street)
                }
                LabelKind::Water => (11.5, palette.label_water),
                LabelKind::Park => {
                    if zoom < 12.0 {
                        continue;
                    }
                    (11.0, palette.label_park)
                }
                LabelKind::Poi => {
                    if zoom < 14.5 {
                        continue;
                    }
                    (10.5, palette.label_poi)
                }
            };

            candidates.push(LabelCandidate {
                priority: lb.priority,
                pos,
                text: &lb.text,
                font_size,
                color,
            });
        }
    }

    candidates.sort_by(|a, b| b.priority.cmp(&a.priority));

    let mut placed_boxes: Vec<Rect> = Vec::new();
    let max_labels = 140;

    for cand in candidates {
        if placed_boxes.len() >= max_labels {
            break;
        }
        let font = FontId::proportional(cand.font_size);
        let galley = painter.layout_no_wrap(cand.text.to_string(), font.clone(), cand.color);
        let size = galley.size();
        let top_left = Pos2::new(cand.pos.x - size.x * 0.5, cand.pos.y - size.y * 0.5);
        let label_rect = Rect::from_min_size(top_left, size).expand(5.0);

        if !map_rect.contains_rect(label_rect) {
            continue;
        }
        if placed_boxes.iter().any(|r| r.intersects(label_rect)) {
            continue;
        }
        placed_boxes.push(label_rect);

        let offsets = [
            Vec2::new(-1.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, -1.0),
            Vec2::new(0.0, 1.0),
        ];
        for off in offsets {
            painter.text(
                cand.pos + off,
                Align2::CENTER_CENTER,
                cand.text,
                font.clone(),
                palette.label_halo,
            );
        }
        painter.galley(top_left, galley, cand.color);
    }
}

fn draw_gpx_routes(
    painter: &Painter,
    map_rect: Rect,
    camera: &MapCamera,
    routes: &[GpxRoute],
    palette: &MapPalette,
) {
    let casing_c = if palette.dark_mode {
        Color32::from_rgba_unmultiplied(15, 18, 22, 220)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 225)
    };

    for route in routes {
        if !route.visible {
            continue;
        }
        let route_color = route.color32();
        for seg in &route.segments {
            if seg.len() < 2 {
                continue;
            }
            let screen_pts: Vec<Pos2> = seg
                .iter()
                .map(|&(lat, lon)| camera.lat_lon_to_screen(lat, lon, map_rect))
                .collect();

            painter.add(Shape::line(
                screen_pts.clone(),
                Stroke::new(6.0_f32, casing_c),
            ));
            painter.add(Shape::line(
                screen_pts.clone(),
                Stroke::new(3.5_f32, route_color),
            ));

            if let Some(&start) = screen_pts.first() {
                if map_rect.contains(start) {
                    painter.circle_filled(start, 5.5, Color32::from_rgb(46, 178, 96));
                    painter.circle_stroke(start, 5.5, Stroke::new(1.8_f32, Color32::WHITE));
                }
            }
            if let Some(&end) = screen_pts.last() {
                if map_rect.contains(end) {
                    painter.circle_filled(end, 5.5, Color32::from_rgb(225, 68, 68));
                    painter.circle_stroke(end, 5.5, Stroke::new(1.8_f32, Color32::WHITE));
                }
            }
        }

        for &(lat, lon, ref wname) in &route.waypoints {
            let pos = camera.lat_lon_to_screen(lat, lon, map_rect);
            if map_rect.contains(pos) {
                painter.circle_filled(pos, 4.5, route_color);
                painter.circle_stroke(pos, 4.5, Stroke::new(1.5_f32, Color32::WHITE));
                if !wname.is_empty() {
                    painter.text(
                        pos + Vec2::new(0.0, -11.0),
                        Align2::CENTER_BOTTOM,
                        wname,
                        FontId::proportional(11.0),
                        palette.label_city,
                    );
                }
            }
        }
    }
}

fn draw_markers(
    painter: &Painter,
    map_rect: Rect,
    camera: &MapCamera,
    markers: &[MapMarker],
    palette: &MapPalette,
) {
    let pill_bg = if palette.dark_mode {
        Color32::from_rgba_unmultiplied(28, 34, 42, 230)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 235)
    };
    let pill_text = if palette.dark_mode {
        Color32::from_rgb(240, 244, 248)
    } else {
        Color32::from_rgb(32, 36, 42)
    };
    let pill_border = if palette.dark_mode {
        Color32::from_rgb(68, 78, 92)
    } else {
        Color32::from_rgb(206, 212, 220)
    };

    for marker in markers {
        let base = camera.lat_lon_to_screen(marker.lat, marker.lon, map_rect);
        if !map_rect.expand(40.0).contains(base) {
            continue;
        }

        let pin_color = marker.color32();
        let head = base + Vec2::new(0.0, -14.0);

        painter.circle_filled(
            base + Vec2::new(0.0, 1.5),
            4.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, 70),
        );

        let mut pin_mesh = Mesh::default();
        pin_mesh.colored_vertex(head + Vec2::new(-7.0, 2.0), pin_color);
        pin_mesh.colored_vertex(head + Vec2::new(7.0, 2.0), pin_color);
        pin_mesh.colored_vertex(base, pin_color);
        pin_mesh.add_triangle(0, 1, 2);
        painter.add(Shape::Mesh(pin_mesh));

        painter.circle_filled(head, 8.5, pin_color);
        painter.circle_stroke(head, 8.5, Stroke::new(1.8_f32, Color32::WHITE));
        painter.circle_filled(head, 3.0, Color32::WHITE);

        if !marker.name.is_empty() {
            let font = FontId::proportional(11.5);
            let galley = painter.layout_no_wrap(marker.name.clone(), font, pill_text);
            let tsize = galley.size();
            let text_pos = Pos2::new(base.x - tsize.x * 0.5, head.y - 14.0 - tsize.y);
            let bg_rect = Rect::from_min_size(text_pos, tsize).expand2(Vec2::new(6.0, 2.5));
            painter.rect_filled(bg_rect, Rounding::same(5.0), pill_bg);
            painter.rect_stroke(
                bg_rect,
                Rounding::same(5.0),
                Stroke::new(1.0_f32, pill_border),
            );
            painter.galley(text_pos, galley, pill_text);
        }
    }
}

fn draw_measure_overlay(
    painter: &Painter,
    map_rect: Rect,
    camera: &MapCamera,
    points: &[(f64, f64)],
    cursor: Option<(f64, f64)>,
    palette: &MapPalette,
) {
    let measure_color = Color32::from_rgb(0, 152, 232);
    let casing_color = if palette.dark_mode {
        Color32::from_rgba_unmultiplied(15, 20, 26, 230)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 235)
    };
    let pill_bg = if palette.dark_mode {
        Color32::from_rgba_unmultiplied(24, 30, 38, 235)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 240)
    };
    let pill_text = if palette.dark_mode {
        Color32::from_rgb(236, 242, 248)
    } else {
        Color32::from_rgb(24, 30, 38)
    };

    let screen_pts: Vec<Pos2> = points
        .iter()
        .map(|&(lat, lon)| camera.lat_lon_to_screen(lat, lon, map_rect))
        .collect();

    if screen_pts.len() >= 2 {
        painter.add(Shape::line(
            screen_pts.clone(),
            Stroke::new(5.5_f32, casing_color),
        ));
        painter.add(Shape::line(
            screen_pts.clone(),
            Stroke::new(3.0_f32, measure_color),
        ));
    }

    for i in 0..points.len().saturating_sub(1) {
        let p0 = screen_pts[i];
        let p1 = screen_pts[i + 1];
        if p0.distance(p1) >= 48.0 {
            let seg_km = haversine_km(points[i].0, points[i].1, points[i + 1].0, points[i + 1].1);
            let mid = Pos2::new((p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
            if map_rect.contains(mid) {
                draw_distance_pill(
                    painter,
                    mid,
                    &format_distance_km(seg_km),
                    pill_bg,
                    measure_color,
                    pill_text,
                );
            }
        }
    }

    if let (Some(&(last_lat, last_lon)), Some(&last_pos), Some((clat, clon))) =
        (points.last(), screen_pts.last(), cursor)
    {
        let cur_pos = camera.lat_lon_to_screen(clat, clon, map_rect);
        if last_pos.distance(cur_pos) > 4.0 {
            draw_dashed_segment(
                painter,
                last_pos,
                cur_pos,
                8.0,
                5.0,
                Stroke::new(2.2_f32, Color32::from_rgba_unmultiplied(0, 152, 232, 185)),
            );
            let seg_km = haversine_km(last_lat, last_lon, clat, clon);
            if last_pos.distance(cur_pos) >= 56.0 {
                let mid = Pos2::new(
                    (last_pos.x + cur_pos.x) * 0.5,
                    (last_pos.y + cur_pos.y) * 0.5,
                );
                draw_distance_pill(
                    painter,
                    mid,
                    &format!("+{}", format_distance_km(seg_km)),
                    pill_bg,
                    measure_color,
                    pill_text,
                );
            }
        }
    }

    for (idx, &pos) in screen_pts.iter().enumerate() {
        if !map_rect.expand(24.0).contains(pos) {
            continue;
        }
        painter.circle_filled(pos, 8.5, measure_color);
        painter.circle_stroke(pos, 8.5, Stroke::new(1.8_f32, Color32::WHITE));
        painter.text(
            pos,
            Align2::CENTER_CENTER,
            format!("{}", idx + 1),
            FontId::proportional(10.0),
            Color32::WHITE,
        );
    }
}

fn draw_distance_pill(
    painter: &Painter,
    center: Pos2,
    label: &str,
    bg: Color32,
    border: Color32,
    text_color: Color32,
) {
    let font = FontId::proportional(11.0);
    let galley = painter.layout_no_wrap(label.to_string(), font, text_color);
    let size = galley.size();
    let top_left = Pos2::new(center.x - size.x * 0.5, center.y - size.y * 0.5);
    let bg_rect = Rect::from_min_size(top_left, size).expand2(Vec2::new(6.0, 2.0));
    painter.rect_filled(bg_rect, Rounding::same(5.0), bg);
    painter.rect_stroke(bg_rect, Rounding::same(5.0), Stroke::new(1.0_f32, border));
    painter.galley(top_left, galley, text_color);
}

fn draw_dashed_segment(
    painter: &Painter,
    a: Pos2,
    b: Pos2,
    dash_len: f32,
    gap_len: f32,
    stroke: Stroke,
) {
    let delta = b - a;
    let total = delta.length();
    if total < 1.0 {
        return;
    }
    let dir = delta / total;
    let mut d = 0.0_f32;
    while d < total {
        let end_d = (d + dash_len).min(total);
        painter.line_segment([a + dir * d, a + dir * end_d], stroke);
        d += dash_len + gap_len;
    }
}
