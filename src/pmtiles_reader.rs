use flate2::read::GzDecoder;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_LEAF_CACHE_ENTRIES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    Unknown,
    None,
    Gzip,
    Brotli,
    Zstd,
}

impl Compression {
    fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::None,
            2 => Self::Gzip,
            3 => Self::Brotli,
            4 => Self::Zstd,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileType {
    Unknown,
    Mvt,
    Png,
    Jpeg,
    Webp,
    Avif,
}

impl TileType {
    fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::Mvt,
            2 => Self::Png,
            3 => Self::Jpeg,
            4 => Self::Webp,
            5 => Self::Avif,
            _ => Self::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Mvt => "Vector (MVT)",
            Self::Png => "Raster (PNG)",
            Self::Jpeg => "Raster (JPEG)",
            Self::Webp => "Raster (WebP)",
            Self::Avif => "Raster (AVIF)",
            Self::Unknown => "Desconocido",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PmtilesHeader {
    pub root_dir_offset: u64,
    pub root_dir_bytes: u64,
    pub json_metadata_offset: u64,
    pub json_metadata_bytes: u64,
    pub leaf_dirs_offset: u64,
    pub tile_data_offset: u64,
    pub internal_compression: Compression,
    pub tile_compression: Compression,
    pub tile_type: TileType,
    pub min_zoom: u8,
    pub max_zoom: u8,
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
    pub center_zoom: u8,
    pub center_lon: f64,
    pub center_lat: f64,
}

impl PmtilesHeader {
    fn parse(buf: &[u8; 127]) -> Result<Self, String> {
        if &buf[0..7] != b"PMTiles" {
            return Err("El archivo no tiene cabecera PMTiles válida".to_string());
        }
        let spec_version = buf[7];
        if spec_version != 3 {
            return Err(format!(
                "Versión PMTiles no soportada (v{spec_version}); se requiere PMTiles v3"
            ));
        }

        let read_u64 = |start: usize| -> u64 {
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&buf[start..start + 8]);
            u64::from_le_bytes(arr)
        };
        let read_i32 = |start: usize| -> i32 {
            let mut arr = [0u8; 4];
            arr.copy_from_slice(&buf[start..start + 4]);
            i32::from_le_bytes(arr)
        };

        Ok(Self {
            root_dir_offset: read_u64(8),
            root_dir_bytes: read_u64(16),
            json_metadata_offset: read_u64(24),
            json_metadata_bytes: read_u64(32),
            leaf_dirs_offset: read_u64(40),
            tile_data_offset: read_u64(56),
            internal_compression: Compression::from_byte(buf[97]),
            tile_compression: Compression::from_byte(buf[98]),
            tile_type: TileType::from_byte(buf[99]),
            min_zoom: buf[100],
            max_zoom: buf[101],
            min_lon: read_i32(102) as f64 / 10_000_000.0,
            min_lat: read_i32(106) as f64 / 10_000_000.0,
            max_lon: read_i32(110) as f64 / 10_000_000.0,
            max_lat: read_i32(114) as f64 / 10_000_000.0,
            center_zoom: buf[118],
            center_lon: read_i32(119) as f64 / 10_000_000.0,
            center_lat: read_i32(123) as f64 / 10_000_000.0,
        })
    }

    pub fn has_valid_bounds(&self) -> bool {
        self.max_lon > self.min_lon
            && self.max_lat > self.min_lat
            && self.min_lon >= -180.0
            && self.max_lon <= 180.0
    }
}

#[derive(Debug, Clone, Default)]
pub struct PmtilesMetadata {
    pub name: Option<String>,
    pub attribution: Option<String>,
    pub description: Option<String>,
}

#[derive(Deserialize)]
struct RawJsonMetadata {
    name: Option<String>,
    attribution: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PmtilesEntry {
    pub tile_id: u64,
    pub offset: u64,
    pub length: u32,
    pub run_length: u32,
}

struct LeafDirCache {
    entries: HashMap<(u64, u32), (u64, Arc<Vec<PmtilesEntry>>)>,
    tick: u64,
}

impl LeafDirCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            tick: 0,
        }
    }

    fn get(&mut self, key: (u64, u32)) -> Option<Arc<Vec<PmtilesEntry>>> {
        self.tick += 1;
        let t = self.tick;
        if let Some((last_used, dir)) = self.entries.get_mut(&key) {
            *last_used = t;
            Some(Arc::clone(dir))
        } else {
            None
        }
    }

    fn insert(&mut self, key: (u64, u32), dir: Arc<Vec<PmtilesEntry>>) {
        self.tick += 1;
        if self.entries.len() >= MAX_LEAF_CACHE_ENTRIES {
            if let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, (used, _))| *used)
                .map(|(k, _)| *k)
            {
                self.entries.remove(&oldest_key);
            }
        }
        self.entries.insert(key, (self.tick, dir));
    }
}

pub struct PmTilesArchive {
    pub path: PathBuf,
    pub file_size_bytes: u64,
    pub header: PmtilesHeader,
    pub metadata: PmtilesMetadata,
    root_dir: Vec<PmtilesEntry>,
    file: Mutex<File>,
    leaf_cache: Mutex<LeafDirCache>,
}

impl PmTilesArchive {
    pub fn open(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path)
            .map_err(|e| format!("No se pudo abrir '{}': {e}", path.display()))?;
        let file_size_bytes = file
            .metadata()
            .map(|m| m.len())
            .unwrap_or(0);

        let mut header_buf = [0u8; 127];
        file.read_exact(&mut header_buf)
            .map_err(|e| format!("Error leyendo cabecera PMTiles: {e}"))?;
        let header = PmtilesHeader::parse(&header_buf)?;

        let root_raw = read_range(&mut file, header.root_dir_offset, header.root_dir_bytes as usize)?;
        let root_decompressed = decompress_bytes(&root_raw, header.internal_compression)?;
        let root_dir = parse_directory(&root_decompressed)?;

        let metadata = if header.json_metadata_bytes > 0 && header.json_metadata_bytes < 4 * 1024 * 1024
        {
            match read_range(
                &mut file,
                header.json_metadata_offset,
                header.json_metadata_bytes as usize,
            ) {
                Ok(raw_meta) => {
                    match decompress_bytes(&raw_meta, header.internal_compression) {
                        Ok(dec_meta) => {
                            if let Ok(parsed) = serde_json::from_slice::<RawJsonMetadata>(&dec_meta) {
                                PmtilesMetadata {
                                    name: parsed.name,
                                    attribution: parsed.attribution,
                                    description: parsed.description,
                                }
                            } else {
                                PmtilesMetadata::default()
                            }
                        }
                        Err(_) => PmtilesMetadata::default(),
                    }
                }
                Err(_) => PmtilesMetadata::default(),
            }
        } else {
            PmtilesMetadata::default()
        };

        Ok(Self {
            path: path.to_path_buf(),
            file_size_bytes,
            header,
            metadata,
            root_dir,
            file: Mutex::new(file),
            leaf_cache: Mutex::new(LeafDirCache::new()),
        })
    }

    pub fn formatted_size(&self) -> String {
        let bytes = self.file_size_bytes as f64;
        if bytes >= 1_073_741_824.0 {
            format!("{:.2} GB", bytes / 1_073_741_824.0)
        } else if bytes >= 1_048_576.0 {
            format!("{:.1} MB", bytes / 1_048_576.0)
        } else {
            format!("{:.0} KB", bytes / 1024.0)
        }
    }

    pub fn covers_bounds(&self, min_lon: f64, min_lat: f64, max_lon: f64, max_lat: f64) -> bool {
        if !self.header.has_valid_bounds() {
            return true;
        }
        self.header.min_lon <= max_lon
            && self.header.max_lon >= min_lon
            && self.header.min_lat <= max_lat
            && self.header.max_lat >= min_lat
    }

    pub fn get_tile_bytes(&self, z: u8, x: u32, y: u32) -> Result<Option<Vec<u8>>, String> {
        if z < self.header.min_zoom || z > self.header.max_zoom {
            return Ok(None);
        }
        let target_id = zxy_to_tile_id(z, x, y);
        let mut current_dir: Option<Arc<Vec<PmtilesEntry>>> = None;

        for depth in 0..5 {
            let entries: &[PmtilesEntry] = match &current_dir {
                None => &self.root_dir,
                Some(arc_dir) => arc_dir.as_slice(),
            };

            let Some(entry) = find_entry(entries, target_id) else {
                return Ok(None);
            };

            if entry.run_length > 0 {
                let abs_offset = self.header.tile_data_offset.saturating_add(entry.offset);
                let raw_tile = {
                    let mut f = self
                        .file
                        .lock()
                        .map_err(|_| "Error de bloqueo leyendo archivo".to_string())?;
                    read_range(&mut f, abs_offset, entry.length as usize)?
                };
                let decompressed = decompress_tile(&raw_tile, self.header.tile_compression)?;
                return Ok(Some(decompressed));
            } else {
                if depth >= 4 {
                    return Err("Profundidad excesiva en directorios PMTiles".to_string());
                }
                let leaf_key = (entry.offset, entry.length);
                let cached = self
                    .leaf_cache
                    .lock()
                    .ok()
                    .and_then(|mut cache| cache.get(leaf_key));

                let next_dir = if let Some(hit) = cached {
                    hit
                } else {
                    let abs_offset = self.header.leaf_dirs_offset.saturating_add(entry.offset);
                    let raw_leaf = {
                        let mut f = self
                            .file
                            .lock()
                            .map_err(|_| "Error de bloqueo leyendo directorio".to_string())?;
                        read_range(&mut f, abs_offset, entry.length as usize)?
                    };
                    let dec_leaf = decompress_bytes(&raw_leaf, self.header.internal_compression)?;
                    let parsed = Arc::new(parse_directory(&dec_leaf)?);
                    if let Ok(mut cache) = self.leaf_cache.lock() {
                        cache.insert(leaf_key, Arc::clone(&parsed));
                    }
                    parsed
                };
                current_dir = Some(next_dir);
            }
        }

        Ok(None)
    }
}

fn read_range(file: &mut File, offset: u64, length: usize) -> Result<Vec<u8>, String> {
    if length == 0 {
        return Ok(Vec::new());
    }
    if length > 64 * 1024 * 1024 {
        return Err("Rango de lectura PMTiles demasiado grande".to_string());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| format!("Error de seek en archivo PMTiles: {e}"))?;
    let mut buf = vec![0u8; length];
    file.read_exact(&mut buf)
        .map_err(|e| format!("Error leyendo bytes del archivo PMTiles: {e}"))?;
    Ok(buf)
}

fn decompress_tile(data: &[u8], compression: Compression) -> Result<Vec<u8>, String> {
    match compression {
        Compression::None => {
            if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
                decompress_bytes(data, Compression::Gzip)
            } else {
                Ok(data.to_vec())
            }
        }
        other => decompress_bytes(data, other),
    }
}

fn decompress_bytes(data: &[u8], compression: Compression) -> Result<Vec<u8>, String> {
    match compression {
        Compression::None => Ok(data.to_vec()),
        Compression::Gzip => {
            let mut decoder = GzDecoder::new(data);
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .map_err(|e| format!("Error descomprimiendo Gzip: {e}"))?;
            Ok(out)
        }
        Compression::Brotli => {
            let mut decoder = brotli::Decompressor::new(data, 4096);
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .map_err(|e| format!("Error descomprimiendo Brotli: {e}"))?;
            Ok(out)
        }
        Compression::Zstd => {
            let mut decoder = ruzstd::StreamingDecoder::new(data)
                .map_err(|e| format!("Error iniciando Zstd: {e}"))?;
            let mut out = Vec::new();
            decoder
                .read_to_end(&mut out)
                .map_err(|e| format!("Error descomprimiendo Zstd: {e}"))?;
            Ok(out)
        }
        Compression::Unknown => Err("Compresión PMTiles desconocida".to_string()),
    }
}

fn read_varint(data: &[u8], cursor: &mut usize) -> Result<u64, String> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    loop {
        if *cursor >= data.len() {
            return Err("EOF inesperado leyendo varint en directorio PMTiles".to_string());
        }
        if shift >= 64 {
            return Err("Overflow de varint en directorio PMTiles".to_string());
        }
        let byte = data[*cursor];
        *cursor += 1;
        result |= ((byte & 0x7f) as u64) << shift;
        if (byte & 0x80) == 0 {
            return Ok(result);
        }
        shift += 7;
    }
}

fn parse_directory(data: &[u8]) -> Result<Vec<PmtilesEntry>, String> {
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let mut cursor = 0;
    let n_entries = read_varint(data, &mut cursor)? as usize;
    if n_entries > 2_000_000 {
        return Err("Directorio PMTiles demasiado grande".to_string());
    }

    let mut entries = Vec::with_capacity(n_entries);
    let mut last_id = 0u64;
    for _ in 0..n_entries {
        let delta = read_varint(data, &mut cursor)?;
        last_id = last_id.saturating_add(delta);
        entries.push(PmtilesEntry {
            tile_id: last_id,
            offset: 0,
            length: 0,
            run_length: 0,
        });
    }

    for entry in &mut entries {
        entry.run_length = read_varint(data, &mut cursor)? as u32;
    }

    for entry in &mut entries {
        entry.length = read_varint(data, &mut cursor)? as u32;
    }

    for i in 0..n_entries {
        let val = read_varint(data, &mut cursor)?;
        if val == 0 && i > 0 {
            entries[i].offset = entries[i - 1]
                .offset
                .saturating_add(entries[i - 1].length as u64);
        } else if val > 0 {
            entries[i].offset = val - 1;
        }
    }

    Ok(entries)
}

fn find_entry(entries: &[PmtilesEntry], target_id: u64) -> Option<&PmtilesEntry> {
    if entries.is_empty() {
        return None;
    }
    let idx = entries.partition_point(|e| e.tile_id <= target_id);
    if idx == 0 {
        return None;
    }
    let entry = &entries[idx - 1];
    if entry.run_length > 0 {
        if target_id < entry.tile_id.saturating_add(entry.run_length as u64) {
            Some(entry)
        } else {
            None
        }
    } else {
        Some(entry)
    }
}

pub fn zxy_to_tile_id(z: u8, x: u32, y: u32) -> u64 {
    if z == 0 {
        return 0;
    }
    let acc = ((1u64 << (2 * z as u32)) - 1) / 3;
    let n = 1u64 << (z as u32);
    acc + xy_to_hilbert(n, x as u64, y as u64)
}

fn xy_to_hilbert(n: u64, mut x: u64, mut y: u64) -> u64 {
    let mut d: u64 = 0;
    let mut s: u64 = n / 2;
    while s > 0 {
        let rx = u64::from((x & s) > 0);
        let ry = u64::from((y & s) > 0);
        d += s * s * ((3 * rx) ^ ry);
        if ry == 0 {
            if rx == 1 {
                x = n - 1 - x;
                y = n - 1 - y;
            }
            std::mem::swap(&mut x, &mut y);
        }
        s /= 2;
    }
    d
}