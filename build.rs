use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR no definido"));
    let ico_path = out_dir.join("app_icon.ico");
    let ico_bytes = build_ico_bytes(&[32, 48, 64, 128]);
    fs::write(&ico_path, ico_bytes).expect("No se pudo escribir app_icon.ico");

    let mut res = winresource::WindowsResource::new();
    res.set_icon(&ico_path.to_string_lossy().replace('\\', "/"));
    res.set("ProductName", "NomadMaps");
    res.set(
        "FileDescription",
        "NomadMaps - Mapas Vectoriales Offline (Desarrollado por Jorge Cuevas Ramos)",
    );
    res.set("CompanyName", "Jorge Cuevas Ramos");
    res.set("LegalCopyright", "Desarrollado por Jorge Cuevas Ramos");
    res.compile()
        .expect("No se pudo incrustar el icono en el ejecutable de Windows");
}

fn build_ico_bytes(sizes: &[u32]) -> Vec<u8> {
    let mut dir_entries = Vec::new();
    let mut images_data = Vec::new();
    let mut current_offset = 6usize + sizes.len() * 16;

    for &size in sizes {
        let rgba = render_icon_rgba(size);
        let dib = encode_dib_32bpp(&rgba, size);

        let w_byte = if size >= 256 { 0u8 } else { size as u8 };
        let h_byte = if size >= 256 { 0u8 } else { size as u8 };

        dir_entries.push(w_byte);
        dir_entries.push(h_byte);
        dir_entries.push(0);
        dir_entries.push(0);
        dir_entries.extend_from_slice(&1u16.to_le_bytes());
        dir_entries.extend_from_slice(&32u16.to_le_bytes());
        dir_entries.extend_from_slice(&(dib.len() as u32).to_le_bytes());
        dir_entries.extend_from_slice(&(current_offset as u32).to_le_bytes());

        current_offset += dib.len();
        images_data.extend_from_slice(&dib);
    }

    let mut ico = Vec::with_capacity(current_offset);
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    ico.extend_from_slice(&dir_entries);
    ico.extend_from_slice(&images_data);
    ico
}

fn encode_dib_32bpp(rgba: &[u8], size: u32) -> Vec<u8> {
    let and_stride = ((size + 31) / 32) * 4;
    let and_len = (and_stride * size) as usize;
    let xor_len = (size * size * 4) as usize;
    let mut dib = Vec::with_capacity(40 + xor_len + and_len);

    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&(size as i32).to_le_bytes());
    dib.extend_from_slice(&((size * 2) as i32).to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&32u16.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());

    for y in (0..size).rev() {
        for x in 0..size {
            let i = ((y * size + x) * 4) as usize;
            dib.push(rgba[i + 2]);
            dib.push(rgba[i + 1]);
            dib.push(rgba[i]);
            dib.push(rgba[i + 3]);
        }
    }

    for y in (0..size).rev() {
        let mut row = vec![0u8; and_stride as usize];
        for x in 0..size {
            let a = rgba[((y * size + x) * 4 + 3) as usize];
            if a == 0 {
                row[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        dib.extend_from_slice(&row);
    }

    dib
}

fn render_icon_rgba(size: u32) -> Vec<u8> {
    let mut out = vec![0u8; (size * size * 4) as usize];
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
                out[idx] = (acc_r * inv_total * norm).round().clamp(0.0, 255.0) as u8;
                out[idx + 1] = (acc_g * inv_total * norm).round().clamp(0.0, 255.0) as u8;
                out[idx + 2] = (acc_b * inv_total * norm).round().clamp(0.0, 255.0) as u8;
                out[idx + 3] = final_a as u8;
            }
        }
    }

    out
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
