use egui::Color32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AreaKind {
    Earth = 0,
    Farmland = 1,
    Residential = 2,
    Commercial = 3,
    Industrial = 4,
    Sand = 5,
    Grass = 6,
    Park = 7,
    Forest = 8,
    Wetland = 9,
    Glacier = 10,
    Aeroway = 11,
    OceanOrLake = 12,
    Building = 13,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoadKind {
    Waterway = 0,
    Ferry = 1,
    Path = 2,
    Rail = 3,
    Service = 4,
    Residential = 5,
    Tertiary = 6,
    Secondary = 7,
    Primary = 8,
    Trunk = 9,
    Motorway = 10,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryKind {
    Country,
    Region,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelKind {
    Country,
    City,
    Town,
    Village,
    Suburb,
    Street,
    Water,
    Park,
    Poi,
}

pub struct MapPalette {
    pub dark_mode: bool,
    pub land_bg: Color32,
    pub water: Color32,
    pub forest: Color32,
    pub park: Color32,
    pub grass: Color32,
    pub farmland: Color32,
    pub residential_area: Color32,
    pub commercial_area: Color32,
    pub industrial_area: Color32,
    pub sand: Color32,
    pub wetland: Color32,
    pub glacier: Color32,
    pub aeroway: Color32,
    pub building_fill: Color32,
    pub building_stroke: Color32,
    pub road_motorway_fill: Color32,
    pub road_motorway_casing: Color32,
    pub road_trunk_fill: Color32,
    pub road_trunk_casing: Color32,
    pub road_primary_fill: Color32,
    pub road_primary_casing: Color32,
    pub road_secondary_fill: Color32,
    pub road_secondary_casing: Color32,
    pub road_minor_fill: Color32,
    pub road_minor_casing: Color32,
    pub road_service_fill: Color32,
    pub road_path: Color32,
    pub rail: Color32,
    pub ferry: Color32,
    pub boundary_country: Color32,
    pub boundary_region: Color32,
    pub label_city: Color32,
    pub label_street: Color32,
    pub label_water: Color32,
    pub label_park: Color32,
    pub label_poi: Color32,
    pub label_halo: Color32,
}

impl MapPalette {
    pub fn light() -> Self {
        Self {
            dark_mode: false,
            land_bg: Color32::from_rgb(243, 239, 230),
            water: Color32::from_rgb(156, 200, 236),
            forest: Color32::from_rgb(191, 221, 171),
            park: Color32::from_rgb(205, 231, 184),
            grass: Color32::from_rgb(220, 238, 198),
            farmland: Color32::from_rgb(236, 233, 206),
            residential_area: Color32::from_rgb(235, 230, 220),
            commercial_area: Color32::from_rgb(236, 223, 226),
            industrial_area: Color32::from_rgb(228, 224, 228),
            sand: Color32::from_rgb(244, 234, 198),
            wetland: Color32::from_rgb(201, 224, 212),
            glacier: Color32::from_rgb(246, 250, 253),
            aeroway: Color32::from_rgb(218, 214, 212),
            building_fill: Color32::from_rgb(221, 213, 199),
            building_stroke: Color32::from_rgb(198, 188, 174),
            road_motorway_fill: Color32::from_rgb(247, 166, 70),
            road_motorway_casing: Color32::from_rgb(212, 118, 38),
            road_trunk_fill: Color32::from_rgb(249, 193, 104),
            road_trunk_casing: Color32::from_rgb(214, 144, 56),
            road_primary_fill: Color32::from_rgb(252, 224, 123),
            road_primary_casing: Color32::from_rgb(214, 180, 74),
            road_secondary_fill: Color32::from_rgb(255, 247, 189),
            road_secondary_casing: Color32::from_rgb(207, 198, 178),
            road_minor_fill: Color32::from_rgb(255, 255, 255),
            road_minor_casing: Color32::from_rgb(212, 206, 195),
            road_service_fill: Color32::from_rgb(249, 247, 242),
            road_path: Color32::from_rgb(176, 148, 122),
            rail: Color32::from_rgb(162, 158, 152),
            ferry: Color32::from_rgb(104, 152, 196),
            boundary_country: Color32::from_rgb(156, 126, 168),
            boundary_region: Color32::from_rgb(184, 166, 192),
            label_city: Color32::from_rgb(42, 46, 52),
            label_street: Color32::from_rgb(68, 72, 80),
            label_water: Color32::from_rgb(56, 108, 156),
            label_park: Color32::from_rgb(66, 112, 54),
            label_poi: Color32::from_rgb(92, 74, 56),
            label_halo: Color32::from_rgba_unmultiplied(255, 255, 255, 225),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark_mode: true,
            land_bg: Color32::from_rgb(28, 33, 39),
            water: Color32::from_rgb(24, 49, 72),
            forest: Color32::from_rgb(31, 51, 39),
            park: Color32::from_rgb(35, 58, 44),
            grass: Color32::from_rgb(33, 52, 41),
            farmland: Color32::from_rgb(36, 42, 36),
            residential_area: Color32::from_rgb(33, 39, 46),
            commercial_area: Color32::from_rgb(42, 37, 45),
            industrial_area: Color32::from_rgb(39, 41, 47),
            sand: Color32::from_rgb(50, 48, 39),
            wetland: Color32::from_rgb(28, 50, 48),
            glacier: Color32::from_rgb(46, 56, 66),
            aeroway: Color32::from_rgb(45, 51, 58),
            building_fill: Color32::from_rgb(43, 50, 59),
            building_stroke: Color32::from_rgb(56, 65, 76),
            road_motorway_fill: Color32::from_rgb(186, 108, 38),
            road_motorway_casing: Color32::from_rgb(112, 62, 18),
            road_trunk_fill: Color32::from_rgb(184, 132, 52),
            road_trunk_casing: Color32::from_rgb(118, 82, 26),
            road_primary_fill: Color32::from_rgb(176, 156, 66),
            road_primary_casing: Color32::from_rgb(106, 94, 36),
            road_secondary_fill: Color32::from_rgb(104, 115, 128),
            road_secondary_casing: Color32::from_rgb(66, 74, 84),
            road_minor_fill: Color32::from_rgb(64, 73, 84),
            road_minor_casing: Color32::from_rgb(38, 44, 52),
            road_service_fill: Color32::from_rgb(52, 60, 70),
            road_path: Color32::from_rgb(82, 92, 102),
            rail: Color32::from_rgb(92, 98, 106),
            ferry: Color32::from_rgb(68, 116, 156),
            boundary_country: Color32::from_rgb(116, 98, 132),
            boundary_region: Color32::from_rgb(82, 74, 94),
            label_city: Color32::from_rgb(232, 236, 242),
            label_street: Color32::from_rgb(196, 204, 214),
            label_water: Color32::from_rgb(128, 178, 224),
            label_park: Color32::from_rgb(138, 194, 134),
            label_poi: Color32::from_rgb(212, 188, 152),
            label_halo: Color32::from_rgba_unmultiplied(18, 22, 27, 225),
        }
    }

    pub fn area_color(&self, kind: AreaKind) -> Color32 {
        match kind {
            AreaKind::Earth => self.land_bg,
            AreaKind::OceanOrLake => self.water,
            AreaKind::Forest => self.forest,
            AreaKind::Park => self.park,
            AreaKind::Grass => self.grass,
            AreaKind::Farmland => self.farmland,
            AreaKind::Residential => self.residential_area,
            AreaKind::Commercial => self.commercial_area,
            AreaKind::Industrial => self.industrial_area,
            AreaKind::Sand => self.sand,
            AreaKind::Wetland => self.wetland,
            AreaKind::Glacier => self.glacier,
            AreaKind::Aeroway => self.aeroway,
            AreaKind::Building => self.building_fill,
        }
    }

    pub fn road_colors(&self, kind: RoadKind) -> (Color32, Color32) {
        match kind {
            RoadKind::Motorway => (self.road_motorway_casing, self.road_motorway_fill),
            RoadKind::Trunk => (self.road_trunk_casing, self.road_trunk_fill),
            RoadKind::Primary => (self.road_primary_casing, self.road_primary_fill),
            RoadKind::Secondary => (self.road_secondary_casing, self.road_secondary_fill),
            RoadKind::Tertiary | RoadKind::Residential => {
                (self.road_minor_casing, self.road_minor_fill)
            }
            RoadKind::Service => (self.road_minor_casing, self.road_service_fill),
            RoadKind::Path => (Color32::TRANSPARENT, self.road_path),
            RoadKind::Rail => (Color32::TRANSPARENT, self.rail),
            RoadKind::Ferry => (Color32::TRANSPARENT, self.ferry),
            RoadKind::Waterway => (Color32::TRANSPARENT, self.water),
        }
    }
}

pub fn road_widths(kind: RoadKind, zoom: f64) -> (f32, f32) {
    let zf = ((zoom - 8.0).max(0.0) / 8.0).clamp(0.15, 1.8) as f32;
    let hi = ((zoom - 13.0).max(0.0) * 0.8) as f32;
    match kind {
        RoadKind::Motorway => {
            let fill = (2.6 * zf + hi * 1.4).clamp(1.6, 12.0);
            (fill + 2.2, fill)
        }
        RoadKind::Trunk => {
            let fill = (2.2 * zf + hi * 1.2).clamp(1.4, 10.5);
            (fill + 2.0, fill)
        }
        RoadKind::Primary => {
            let fill = (1.8 * zf + hi * 1.1).clamp(1.2, 9.5);
            (fill + 1.8, fill)
        }
        RoadKind::Secondary => {
            let fill = (1.5 * zf + hi * 0.95).clamp(1.0, 8.2);
            (fill + 1.6, fill)
        }
        RoadKind::Tertiary => {
            let fill = (1.2 * zf + hi * 0.85).clamp(0.8, 7.2);
            (fill + 1.4, fill)
        }
        RoadKind::Residential => {
            let fill = (1.0 * zf + hi * 0.75).clamp(0.7, 6.2);
            (fill + 1.3, fill)
        }
        RoadKind::Service => {
            let fill = (0.8 * zf + hi * 0.5).clamp(0.6, 4.2);
            (fill + 1.0, fill)
        }
        RoadKind::Path => (0.0, (0.9 * zf + hi * 0.25).clamp(0.7, 2.2)),
        RoadKind::Rail => (0.0, (1.1 * zf + hi * 0.2).clamp(0.8, 2.5)),
        RoadKind::Ferry => (0.0, 1.2),
        RoadKind::Waterway => (0.0, (1.2 * zf + hi * 0.5).clamp(0.9, 5.0)),
    }
}

pub fn classify_area(layer: &str, kind: &str, class: &str) -> Option<AreaKind> {
    let tag = if !kind.is_empty() { kind } else { class };
    match layer {
        "earth" => Some(AreaKind::Earth),
        "water" => Some(AreaKind::OceanOrLake),
        "buildings" | "building" => Some(AreaKind::Building),
        "aeroway" => Some(AreaKind::Aeroway),
        "landcover" | "landuse" | "natural" | "park" => match tag {
            "forest" | "wood" => Some(AreaKind::Forest),
            "park" | "national_park" | "nature_reserve" | "garden" | "recreation_ground"
            | "pitch" | "playground" | "cemetery" | "golf_course" => Some(AreaKind::Park),
            "grass" | "meadow" | "scrub" | "heath" | "village_green" => Some(AreaKind::Grass),
            "farmland" | "farm" | "orchard" | "vineyard" | "allotments" => Some(AreaKind::Farmland),
            "residential" | "neighbourhood" => Some(AreaKind::Residential),
            "commercial" | "retail" | "hospital" | "school" | "university" | "college" => {
                Some(AreaKind::Commercial)
            }
            "industrial" | "railway" | "quarry" | "construction" => Some(AreaKind::Industrial),
            "sand" | "beach" | "dune" => Some(AreaKind::Sand),
            "wetland" | "marsh" | "swamp" => Some(AreaKind::Wetland),
            "glacier" | "ice" | "snow" => Some(AreaKind::Glacier),
            _ => {
                if layer == "park" {
                    Some(AreaKind::Park)
                } else {
                    Some(AreaKind::Grass)
                }
            }
        },
        _ => None,
    }
}

pub fn classify_road(layer: &str, kind: &str, kind_detail: &str, class: &str) -> Option<RoadKind> {
    match layer {
        "waterway" | "water" => Some(RoadKind::Waterway),
        "transit" => match kind {
            "rail" => Some(RoadKind::Rail),
            "ferry" => Some(RoadKind::Ferry),
            _ => None,
        },
        "roads" | "transportation" | "transportation_name" => {
            let primary_tag = if !kind_detail.is_empty() {
                kind_detail
            } else if !class.is_empty() {
                class
            } else {
                kind
            };
            match primary_tag {
                "motorway" | "motorway_link" | "highway" => Some(RoadKind::Motorway),
                "trunk" | "trunk_link" => Some(RoadKind::Trunk),
                "primary" | "primary_link" | "major_road" => Some(RoadKind::Primary),
                "secondary" | "secondary_link" | "medium_road" => Some(RoadKind::Secondary),
                "tertiary" | "tertiary_link" => Some(RoadKind::Tertiary),
                "residential" | "living_street" | "unclassified" | "minor" | "minor_road" => {
                    Some(RoadKind::Residential)
                }
                "service" | "track" => Some(RoadKind::Service),
                "path" | "footway" | "cycleway" | "pedestrian" | "steps" | "bridleway" => {
                    Some(RoadKind::Path)
                }
                "rail" | "tram" | "subway" | "light_rail" => Some(RoadKind::Rail),
                "ferry" => Some(RoadKind::Ferry),
                _ => match kind {
                    "highway" => Some(RoadKind::Motorway),
                    "major_road" => Some(RoadKind::Primary),
                    "medium_road" => Some(RoadKind::Secondary),
                    "minor_road" => Some(RoadKind::Residential),
                    "path" => Some(RoadKind::Path),
                    "rail" => Some(RoadKind::Rail),
                    "ferry" => Some(RoadKind::Ferry),
                    _ => Some(RoadKind::Residential),
                },
            }
        }
        _ => None,
    }
}