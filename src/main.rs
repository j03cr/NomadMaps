#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod gpx;
mod map_math;
mod markers;
mod mvt;
mod pmtiles_reader;
mod renderer;
mod style;
mod tile_manager;

fn build_app_icon() -> egui::IconData {
    let size = 64u32;
    let mut rgba = vec![0u8; (size * size * 4) as usize];
    let sub = 4u32;
    let inv_total = 1.0 / ((sub * sub) as f32);

    for y in 0..size {
        for x in 0..size {
            let mut acc_r = 0.0_f32;
            let mut acc_g = 0.0_f32;
            let mut acc_b = 0.0_f32;
            let mut acc_a = 0.0_f32;

            for sy in 0..sub {
                for sx in 0..sub {
                    let u = (x as f32 + (sx as f32 + 0.5) / sub as f32) / size as f32;
                    let v = (y as f32 + (sy as f32 + 0.5) / sub as f32) / size as f32;
                    let [r, g, b, a] = sample_map_pin_icon(u, v);
                    let af = a / 255.0;
                    acc_r += r * af;
                    acc_g += g * af;
                    acc_b += b * af;
                    acc_a += a;
                }
            }

            let idx = ((y * size + x) * 4) as usize;
            let final_a = (acc_a * inv_total).round().clamp(0.0, 255.0);
            if final_a > 0.0 {
                let norm = 255.0 / final_a;
                rgba[idx] = (acc_r * inv_total * norm).round().clamp(0.0, 255.0) as u8;
                rgba[idx + 1] = (acc_g * inv_total * norm).round().clamp(0.0, 255.0) as u8;
                rgba[idx + 2] = (acc_b * inv_total * norm).round().clamp(0.0, 255.0) as u8;
                rgba[idx + 3] = final_a as u8;
            }
        }
    }

    egui::IconData {
        rgba,
        width: size,
        height: size,
    }
}

fn sample_map_pin_icon(u: f32, v: f32) -> [f32; 4] {
    let dx = u - 0.5;
    let dy = v - 0.5;
    let dist_center = (dx * dx + dy * dy).sqrt();

    if dist_center > 0.465 {
        return [0.0, 0.0, 0.0, 0.0];
    }
    if dist_center > 0.43 {
        return [248.0, 250.0, 249.0, 255.0];
    }

    let t = ((v - 0.07) / 0.86).clamp(0.0, 1.0);
    let mut r = 42.0 + (26.0 - 42.0) * t;
    let mut g = 172.0 + (126.0 - 172.0) * t;
    let mut b = 96.0 + (72.0 - 96.0) * t;

    let river_d = (u * 0.75 + v * 0.65 - 0.82).abs();
    if river_d < 0.045 {
        r = r * 0.72 + 125.0 * 0.28;
        g = g * 0.72 + 205.0 * 0.28;
        b = b * 0.72 + 235.0 * 0.28;
    }

    let shadow_d = ((u - 0.5) * (u - 0.5) / 0.015 + (v - 0.785) * (v - 0.785) / 0.0022).sqrt();
    if shadow_d < 1.0 {
        let s = (1.0 - shadow_d) * 0.35;
        r *= 1.0 - s;
        g *= 1.0 - s;
        b *= 1.0 - s;
    }

    let pin_cx = 0.5_f32;
    let pin_cy = 0.39_f32;
    let pin_r = 0.165_f32;
    let tip_y = 0.77_f32;

    let pdx = u - pin_cx;
    let pdy = v - pin_cy;
    let pin_head_d = (pdx * pdx + pdy * pdy).sqrt();

    let tan_y = 0.4636_f32;
    let tan_half_w = 0.1477_f32;
    let in_tail = v >= tan_y
        && v <= tip_y
        && pdx.abs() <= tan_half_w * ((tip_y - v) / (tip_y - tan_y));

    if pin_head_d <= pin_r || in_tail {
        if pin_head_d <= 0.068 {
            return [26.0, 130.0, 72.0, 255.0];
        }
        return [255.0, 255.0, 255.0, 255.0];
    }

    [r, g, b, 255.0]
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("NomadMaps")
            .with_icon(build_app_icon())
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([840.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "NomadMaps",
        native_options,
        Box::new(|cc| Ok(Box::new(app::NomadMapsApp::new(cc)))),
    )
}