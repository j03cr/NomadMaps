use crate::mvt::{decode_mvt_tile, DecodedTile};
use crate::pmtiles_reader::PmTilesArchive;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

pub const MAX_CACHE_BYTES: usize = 150 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub map_id: usize,
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

struct TileJob {
    key: TileKey,
    archive: Arc<PmTilesArchive>,
}

struct TileResult {
    key: TileKey,
    tile: Option<Arc<DecodedTile>>,
}

struct JobQueue {
    queue: Mutex<VecDeque<TileJob>>,
    condvar: Condvar,
    shutdown: AtomicBool,
}

struct CacheEntry {
    tile: Option<Arc<DecodedTile>>,
    bytes: usize,
    last_used: u64,
}

pub enum TileLookup {
    Ready(Option<Arc<DecodedTile>>),
    Parent {
        z: u8,
        x: u32,
        y: u32,
        tile: Arc<DecodedTile>,
    },
    Missing,
}

pub struct TileManager {
    job_queue: Arc<JobQueue>,
    result_rx: mpsc::Receiver<TileResult>,
    workers: Vec<JoinHandle<()>>,
    cache: HashMap<TileKey, CacheEntry>,
    in_flight: HashSet<TileKey>,
    current_bytes: usize,
    tick: u64,
}

impl TileManager {
    pub fn new(ctx: egui::Context) -> Self {
        let num_workers = std::thread::available_parallelism()
            .map(|n| n.get().clamp(2, 6))
            .unwrap_or(4);

        let job_queue = Arc::new(JobQueue {
            queue: Mutex::new(VecDeque::new()),
            condvar: Condvar::new(),
            shutdown: AtomicBool::new(false),
        });

        let (result_tx, result_rx) = mpsc::channel::<TileResult>();
        let mut workers = Vec::with_capacity(num_workers);

        for _ in 0..num_workers {
            let jq = Arc::clone(&job_queue);
            let tx = result_tx.clone();
            let worker_ctx = ctx.clone();

            let handle = thread::spawn(move || loop {
                let job = {
                    let mut guard = match jq.queue.lock() {
                        Ok(g) => g,
                        Err(_) => break,
                    };
                    loop {
                        if jq.shutdown.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Some(j) = guard.pop_front() {
                            break j;
                        }
                        guard = match jq.condvar.wait(guard) {
                            Ok(g) => g,
                            Err(_) => return,
                        };
                    }
                };

                let decoded = match job.archive.get_tile_bytes(job.key.z, job.key.x, job.key.y) {
                    Ok(Some(raw)) => decode_mvt_tile(&raw, job.key.z).ok().map(Arc::new),
                    Ok(None) | Err(_) => None,
                };

                if tx
                    .send(TileResult {
                        key: job.key,
                        tile: decoded,
                    })
                    .is_err()
                {
                    break;
                }
                worker_ctx.request_repaint();
            });
            workers.push(handle);
        }

        Self {
            job_queue,
            result_rx,
            workers,
            cache: HashMap::new(),
            in_flight: HashSet::new(),
            current_bytes: 0,
            tick: 0,
        }
    }

    pub fn poll_results(&mut self) -> usize {
        let mut count = 0;
        while let Ok(res) = self.result_rx.try_recv() {
            self.in_flight.remove(&res.key);
            let bytes = match &res.tile {
                Some(t) => t.byte_size,
                None => 64,
            };

            if let Some(prev) = self.cache.remove(&res.key) {
                self.current_bytes = self.current_bytes.saturating_sub(prev.bytes);
            }

            self.tick += 1;
            self.current_bytes = self.current_bytes.saturating_add(bytes);
            self.cache.insert(
                res.key,
                CacheEntry {
                    tile: res.tile,
                    bytes,
                    last_used: self.tick,
                },
            );
            count += 1;
        }

        if self.current_bytes > MAX_CACHE_BYTES {
            self.evict_lru();
        }

        count
    }

    fn evict_lru(&mut self) {
        let target_bytes = (MAX_CACHE_BYTES * 9) / 10;
        let mut entries: Vec<(TileKey, u64, usize)> = self
            .cache
            .iter()
            .map(|(&k, e)| (k, e.last_used, e.bytes))
            .collect();
        entries.sort_unstable_by_key(|&(_, last_used, _)| last_used);

        for (key, _, bytes) in entries {
            if self.current_bytes <= target_bytes {
                break;
            }
            if self.cache.remove(&key).is_some() {
                self.current_bytes = self.current_bytes.saturating_sub(bytes);
            }
        }
    }

    pub fn request_tile(
        &mut self,
        map_id: usize,
        archive: &Arc<PmTilesArchive>,
        z: u8,
        x: u32,
        y: u32,
    ) {
        let key = TileKey { map_id, z, x, y };
        if self.cache.contains_key(&key) || self.in_flight.contains(&key) {
            return;
        }

        self.in_flight.insert(key);
        if let Ok(mut guard) = self.job_queue.queue.lock() {
            if guard.len() > 256 {
                if let Some(dropped) = guard.pop_back() {
                    self.in_flight.remove(&dropped.key);
                }
            }
            guard.push_front(TileJob {
                key,
                archive: Arc::clone(archive),
            });
            self.job_queue.condvar.notify_one();
        }
    }

    pub fn get_or_parent(&mut self, map_id: usize, z: u8, x: u32, y: u32) -> TileLookup {
        let key = TileKey { map_id, z, x, y };
        self.tick += 1;
        let now = self.tick;

        if let Some(entry) = self.cache.get_mut(&key) {
            entry.last_used = now;
            return TileLookup::Ready(entry.tile.clone());
        }

        for step in 1..=3u8 {
            if z < step {
                break;
            }
            let pz = z - step;
            let px = x >> step;
            let py = y >> step;
            let pkey = TileKey {
                map_id,
                z: pz,
                x: px,
                y: py,
            };
            if let Some(entry) = self.cache.get_mut(&pkey) {
                entry.last_used = now;
                if let Some(tile) = &entry.tile {
                    return TileLookup::Parent {
                        z: pz,
                        x: px,
                        y: py,
                        tile: Arc::clone(tile),
                    };
                }
            }
        }

        TileLookup::Missing
    }

    pub fn cache_bytes(&self) -> usize {
        self.current_bytes
    }

    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }
}

impl Drop for TileManager {
    fn drop(&mut self) {
        self.job_queue.shutdown.store(true, Ordering::Relaxed);
        if let Ok(mut q) = self.job_queue.queue.lock() {
            q.clear();
        }
        self.job_queue.condvar.notify_all();
        for handle in self.workers.drain(..) {
            let _ = handle.join();
        }
    }
}