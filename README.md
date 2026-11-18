# NomadMaps

Visor de mapas vectoriales **100% offline, nativo y portable para Windows**, escrito íntegramente en **Rust puro** (sin Electron, sin Tauri y sin WebView2), inspirado en la experiencia limpia y fluida de **Maps.me / Organic Maps**.

> **Desarrollado por Jorge Cuevas Ramos**

---

## Características principales

- **Un único ejecutable 100% portable (`NomadMaps.exe`):**
  - Enlazado estático de la CRT de Windows (`+crt-static`), sin instalador ni dependencias externas (`vcruntime140.dll` / WebView2).
  - Funciona con doble clic desde cualquier carpeta o memoria USB en **Windows 10 (1809+)** y **Windows 11**.
  - **Cero escritura en el Registro de Windows o `AppData`:** todo se lee y guarda de forma relativa junto a `NomadMaps.exe` (`./mapas/`, `./config.json` y `./marcadores.json`).
- **Motor cartográfico rápido y ligero:**
  - Lectura directa por rangos de bytes desde archivos locales **PMTiles v3** (sin cargar países enteros en memoria RAM), con soporte de compresión Gzip, Brotli y Zstd.
  - Decodificador MVT (Mapbox Vector Tiles) y triangulación de polígonos cóncavos y agujeros (islas/lagos) ejecutados en segundo plano con hilos nativos (`std::thread`) para mantener una interfaz fluida.
  - Caché LRU en memoria RAM limitada automáticamente a **150 MB**, con respaldo visual de tiles padre mientras cargan los detalles.
- **Navegación moderna:**
  - Arrastrar para mover el mapa, rueda del ratón con zoom suave centrado en el cursor, doble clic para acercar, atajos de teclado (flechas, `+` / `-`) y controles flotantes.
  - **Modo Claro** y **Modo Oscuro** inspirados en Organic Maps.
  - Barra inferior con coordenadas (`Lat / Lon`), nivel de zoom y uso de caché RAM en tiempo real.
- **Gestor de mapas locales (`.pmtiles`):**
  - Escaneo automático de `./mapas/` e importación de archivos desde cualquier ruta del sistema.
  - Activación simultánea de múltiples países o regiones, vista de metadatos (tamaño, zooms, límites geográficos) y centrado automático.
- **Medición de distancias multi-punto:**
  - Selecciona múltiples puntos consecutivos sobre el mapa desde el botón **`Medir distancia`** o con clic derecho (**`Medir distancia desde aquí`**).
  - Visualización de nodos numerados, distancia por tramo, vista previa discontinua al cursor, distancia total (`m` / `km`) y botones **Deshacer** (`Backspace`), **Limpiar** y **Cerrar** (`Esc`).
- **Marcadores y rutas GPX:**
  - Clic derecho en cualquier punto del mapa para guardar marcadores personalizados con nombre, color y notas en `./marcadores.json`.
  - Importación y visualización de rutas **`.gpx`** con cálculo automático de distancia total y waypoints.
- **Ayuda integrada para conseguir mapas:**
  - Modal **`? Descargar Mapas / Ayuda`** con instrucciones paso a paso y un **generador interactivo de comandos para `pmtiles.exe`**.

---

## Uso rápido (Portable)

1. Descarga **`NomadMaps.exe`** desde la pestaña **Actions -> Artifacts** de este repositorio.
2. Coloca `NomadMaps.exe` en la carpeta o memoria USB donde quieras llevarlo.
3. Al abrirlo por primera vez creará automáticamente la carpeta `./mapas/` a su lado.
4. Copia dentro tus archivos `.pmtiles` (o usa el botón **`+ Añadir mapa (.pmtiles)`**).

### ¿Dónde conseguir mapas `.pmtiles`?

- **Países y regiones listas para descargar:**  
  Entra en [download.tiles.coalition.space/bundles](https://download.tiles.coalition.space/bundles), descarga el `.pmtiles` de tu país y ponlo en `./mapas/`.
- **Recortes a medida (solo tu ciudad o zona):**  
  Usa la herramienta oficial [`pmtiles.exe`](https://github.com/protomaps/go-pmtiles/releases) junto con el generador de comandos incluido dentro del botón **`? Descargar Mapas / Ayuda`** de NomadMaps.

---

## Controles principales

| Acción | Control |
| :--- | :--- |
| Mover el mapa | Arrastrar con botón izquierdo o teclas de flecha |
| Acercar / Alejar | Rueda del ratón, doble clic, botones `+` / `-` o teclas `+` / `-` |
| Guardar marcador / Medir / Copiar coordenadas | Clic derecho sobre el mapa |
| Deshacer último punto de medición | Botón `Deshacer` o tecla `Backspace` |
| Salir de medición / Cerrar menú | Botón `Cerrar` o tecla `Esc` |

---

## Compilación en la nube (GitHub Actions)

Este repositorio incluye el flujo automático `.github/workflows/build.yml`, que compila **`NomadMaps.exe`** en `windows-latest` para el target `x86_64-pc-windows-msvc` y publica el artefacto **`NomadMaps-Portable-Windows-x64`** listo para descargar.

### Compilación local (opcional)

Si tienes Rust instalado en Windows con el toolchain MSVC:

```bash
rustup target add x86_64-pc-windows-msvc
cargo build --release --target x86_64-pc-windows-msvc
```

El ejecutable generado estará en:

```text
target/x86_64-pc-windows-msvc/release/NomadMaps.exe
```

---

## Estructura del proyecto

```text
NomadMaps/
├── .cargo/
│   └── config.toml
├── .github/
│   └── workflows/
│       └── build.yml
├── build.rs
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs
    ├── app.rs
    ├── config.rs
    ├── gpx.rs
    ├── map_math.rs
    ├── markers.rs
    ├── mvt.rs
    ├── pmtiles_reader.rs
    ├── renderer.rs
    ├── style.rs
    └── tile_manager.rs
```

---

## Autor

**Jorge Cuevas Ramos**