// Garta - GPX viewer and editor
// Copyright (C) 2016-2017, Timo Saarinen
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

// -------------------------------------------------------------------------------------------------
// Currently [Dec 2016] gtk-rs bindings (0.1.1) don't support asynchronous Pixbuf loading
// The tile cache implementation relies on synchronous ImageSurface::create_for_data call on gtk 
// main thread that doesn't guarantee the smoothest possible user experience. If gtk-rs becomes 
// more complete in the future this module will be rewritten.
// -------------------------------------------------------------------------------------------------

/*
TILE LOADING SEQUENCE DIAGRAM

     TileObserver   TileCache   TileRequestQueue   TileSource         hyper::Client
           |            |               |               |                   |    
      draw |            |               |               |                   |    
 --------->|   get_tile |               |               |                   |    
           |----------->|  push_request |               |                   |    
           |            |-------------->|               |                   |    
           |            |<- - - - - - - |               |                   |    
           |<- - - - - -|                               |                   |    
 <- - - - -|            |                               |                   |    
           |            |        (worker thread)        |                   |    
           |            |               :               |                   |    
           |            |               |fetch_tile_data|                   |    
           |            |               |-------------->|               get |    
           |            |               |               |------------------>|     HTTP GET  
           |            |               |               |                   |------------->
           |            |               |               |                   |<- - - - - - -    
           |            |               |               |<- - - - - - - - - |    
           |            |               |<- - - - - - - |                   |    
           |            |               |               |                   |    
           |            |               :               |                   |    
           |            |                               |                   |    
           |            |                               |                   |    
           |            | handle_result |               |                   |    
           |            |<--------------|               |                   |    
           |            |- - - - - - - >|               |                   |    
           |                            |               |                   |    
           | tile_loaded                |               |                   |    
           |<---------------------------|               |                   |    
 queue_draw|                            |               |                   |    
 <---------|                            |               |                   |    
 - - - - ->|                            |               |                   |    
           |- - - - - - - - - - - - - ->|               |                   |    
           |                            |               |                   |    
           |                            |               |                   |    
           |                            |               |                   |    
*/

extern crate cairo;
extern crate glib;
extern crate gdk;
extern crate gdk_pixbuf;
extern crate chrono;
extern crate rand;
extern crate hyper;
extern crate image;
extern crate serde_json;

use std::cell::{RefCell};
use std::rc::{Rc};
use std::sync::{Arc, RwLock, Mutex, Condvar};
use std::sync::mpsc::{channel, Receiver};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::thread;
use std::cmp::{Ordering};
use std::io::{Read};
use std::vec::{Vec};
use std::fmt;
use std::path;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::ffi;
use std::mem;
use std::time;
use self::chrono::{DateTime, UTC, TimeZone, Duration};
use self::hyper::header;
use self::hyper::{Client};
use self::hyper::status::{StatusCode};
use self::rand::{Rng};
use self::cairo::{Format, ImageSurface};

use core::persistence::{serialize_to, deserialize_from};
use core::settings::{settings_read, DEFAULT_TILE_EXPIRE_DAYS};

// ---- TileObserver -------------------------------------------------------------------------------

/// Callback-like trait for objects that want to get notified when a tile is fetched.
pub trait TileObserver {
    fn tile_loaded(&self, treq: &TileRequest);
}

// ---- TileCache ----------------------------------------------------------------------------------
/// The main access point for tiles
pub struct TileCache {
    /// TileRequest::to_key -> Tile map
    tiles: BTreeMap<String, Tile>,

    /// The queue accessed by the worker threads    
    tile_request_queue: Arc<RwLock<TileRequestQueue>>,

    /// Object to be notified when new tiles are ready.    
    pub observer: Option<Rc<RefCell<TileObserver>>>,
    
    /// Memory used by the cached tiles
    mem_usage: usize,
    
    /// Disk used by the cached tiles.
    disk_usage: u64,
}

/// The first function to be called in this module.
pub fn create_tile_cache() -> Rc<RefCell<TileCache>> {
    // Create tcache and trqueue
    let mut tcache = TileCache::new();
    let trqueue_ar = TileRequestQueue::new();
    
    // Add a reference from tcache to trqueue
    tcache.tile_request_queue = trqueue_ar.clone();
    
    // Initialize trqueue that results a cyclic reference back to tcache from trqueue thread locals
    let tcache_rr = Rc::new(RefCell::new(tcache));
    trqueue_ar.write().unwrap().init(trqueue_ar.clone(), tcache_rr.clone());
    
    // Initialize tcache
    tcache_rr.borrow_mut().restore();
    
    // Return tcache
    tcache_rr
}

impl TileCache {
    // Private constructor. Use function create_tile_cache to create an instance.
    fn new() -> TileCache {
        let tcache = TileCache {
            tiles: BTreeMap::new(),
            tile_request_queue: TileRequestQueue::new(),
            observer: None,
            mem_usage: 0,
            disk_usage: 0,
        };
        tcache
    }

    /// Return tile for the given request. The result may be an approximation.    
    pub fn get_tile(&mut self, treq: &TileRequest) -> Option<&mut Tile> {
        let tile_key = treq.to_key();
        if self.tiles.contains_key(&tile_key) {
            // Check tile state
            if self.tiles.get(&tile_key).unwrap().state == TileState::Void {
                // Special case for Void which requires mutating the tile hashmap
                debug!("Loading a void tile: {}", tile_key);
                let mut tile = Tile::new_with_request(treq);
                tile.state = TileState::Pending;
                self.tiles.insert(tile_key.clone(), tile);
                self.tile_request_queue.write().unwrap().push_request(treq);
                
                // Return
                Some(self.tiles.get_mut(&tile_key).unwrap())
            } else {
                // Faster case for the rest of the states
                let tile = self.tiles.get_mut(&tile_key).unwrap();
                match tile.state {
                    TileState::Void => {
                        panic!("Unexpected code path!");
                    }
                    TileState::Pending => {
                    }
                    TileState::Fetching => {
                    }
                    TileState::Ready => {
                        // Check tile expiration
                        if tile.is_expired() {
                            debug!("Memory-cached tile expired, requesting an update: {}", tile_key);
                            self.tile_request_queue.write().unwrap().push_request(treq);
                        }
                    }
                    TileState::Error => {
                    }
                    TileState::Flushed => {
                        debug!("Reloading a flushed tile: {}", tile_key);
                        tile.state = TileState::Pending;
                        self.tile_request_queue.write().unwrap().push_request(treq);
                    }
                }

                // Update access time
                tile.access_time = UTC::now();
                
                // Return
                Some(tile)
            }
        } else {
            // Enqueue the request and create a new empty tile
            debug!("Requesting a new tile: {}", tile_key);
            self.tile_request_queue.write().unwrap().push_request(treq);
            let mut tile = Tile::new_with_request(treq);
            
            // Approximate content
            if treq.z > 0 {
                let mut treq_up = treq.zoom_out();
                let mut n = 2;
                while treq_up.z >= 1 {
                    let tile_key_up = treq.to_key();
                    if self.tiles.contains_key(&tile_key_up) {
                        tile = self.tiles.get(&tile_key).unwrap().zoom_in(&treq);
                        break;
                    }
                    treq_up = treq_up.zoom_out();
                    n *= 2;
                }
                
                // Create a black tile
                if treq_up.z == 0 {
                    tile = Tile::new(&treq, 0.0, 0.0, 0.0);
                }
            } else {
                tile = Tile::new(&treq, 0.0, 0.0, 0.0);
            }
            
            // Store tile and return
            self.tiles.insert(tile_key.clone(), tile);
            Some(self.tiles.get_mut(&tile_key).unwrap())
        }
    }
    
    /// Handle image fetch result from a worker thread.
    fn handle_result(&mut self, treq_result: &TileRequestResult) {
        // Assign tile information
        if let Some(ref mut tile) = self.tiles.get_mut(&treq_result.to_key()) {
            let old_mem_usage = tile.estimate_mem_usage();
        
            // Assign tile data
            let old_tile_disk_usage = tile.disk_usage;
            tile.state = TileState::Ready;
            tile.data = Some(treq_result.data.clone());
            tile.width = treq_result.tile_width;
            tile.height = treq_result.tile_height;
            tile.expire_time = match treq_result.expire_time {
                Some(expire_time) => { expire_time },
                None => { UTC::now() + Duration::days(DEFAULT_TILE_EXPIRE_DAYS) }
            };
            tile.filepath = {
                match treq_result.request.to_cache_path() 
                    { Ok(pathbuf) => { Some(pathbuf) }, Err(e) => { None } }
            };
            tile.disk_usage = {
                if let Some(ref img_data) = treq_result.img_data { img_data.len() } else { 0 }
            } as u64;
            self.mem_usage = self.mem_usage + tile.estimate_mem_usage() - old_mem_usage;
            self.disk_usage = self.disk_usage + tile.disk_usage - old_tile_disk_usage
        } else {
            warn!("Received image data fetch for tile {} but tile isn't in cache!", treq_result.to_key());
        }
        
        // Mem-flush a tile which would expire the soonest
        if let Some(mem_capacity) = settings_read().tile_mem_cache_capacity {
            while self.mem_usage > mem_capacity && !self.tiles.is_empty() {
                // Flush the soonest-to-expire tile
                if let Some((ref tile_id, ref mut tile)) = self.tiles.iter_mut().next() {
                    let tmu0 = tile.estimate_mem_usage();
                    tile.flush();
                    self.mem_usage = self.mem_usage + tile.estimate_mem_usage() - tmu0;
                }
            }
        }
        
        // Disk-flush a tile which would expire the soonest
        if let Some(disk_capacity) = settings_read().tile_disk_cache_capacity {
            if self.disk_usage > disk_capacity {
                // Flush the last tile
                for (ref tile_id, ref mut tile) in self.tiles.iter_mut() {
                    if let Some(filepath) = tile.filepath.clone() {
                        let mut delete_file = false;
                        let mut file_size: u64 = 0;
                        if filepath.exists() {
                            match fs::File::open(&filepath) {
                                Ok(f) => {
                                    match f.metadata() {
                                        Ok(metadata) => {
                                            // Get file size
                                            file_size = metadata.len();
                                            
                                            // Delete file
                                            match fs::remove_file(&filepath) {
                                                Ok(()) => { 
                                                    delete_file = true;
                                                }
                                                Err(e) => {
                                                    warn!("Failed to remove file {}: {}", 
                                                        filepath.to_str().unwrap_or("???"), e);
                                                }
                                            }
                                            
                                        },
                                        Err(e) => {
                                            warn!("No metadata for file {}: {}", 
                                                filepath.to_str().unwrap_or("???"), e);
                                        }
                                    }
                                },
                                Err(e) => {
                                    warn!("Failed to stat file {}: {}", 
                                        filepath.to_str().unwrap_or("???"), e);
                                }
                            }
                        }
                        
                        // Update disk_cache size
                        if delete_file {
                            debug!("Removing file {} from cache {} -> {} bytes", 
                                filepath.to_str().unwrap_or("???"), 
                                self.disk_usage, self.disk_usage - file_size);
                            self.disk_usage -= file_size;
                            tile.filepath = None;
                            if self.disk_usage <= disk_capacity { break; }
                        }
                    }
                }
            }
        }
        
    }

    /// Save cache state to disk. Typically this is called before the application is closed.
    pub fn store(&self) {
        // Create state
        let state = TileCacheState::new(self);
        
        // Write to cache dir
        let mut pathbuf = settings_read().cache_directory();
        pathbuf.push("state");
        match serialize_to(&state, pathbuf) {
            Ok(()) => {
                debug!("Tile cache state stored successfully: {:?}", self);
            },
            Err(e) => {
                warn!("Failed to store tile cache state: {}", e);
            }
        }
    }

    /// Load cache state from disk. This should be called at startup of the application.
    pub fn restore(&mut self) {
        // Read from cache dir
        let mut pathbuf = settings_read().cache_directory();
        pathbuf.push("state");
        match deserialize_from::<TileCacheState, path::PathBuf>(pathbuf) {
            Ok(mut tcstate) => {
                tcstate.apply(self);
                debug!("Tile cache restored: {:?}", self);
            },
            Err(e) => {
                warn!("Failed to restore tile cache state: {}", e);
                info!("Clearing tile cache");
                // TODO: clear cache
            }
        }        
    }
}

impl fmt::Debug for TileCache {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "tiles={} queue.len={} observer={} mem_usage={} disk_usage={}",
            self.tiles.len(),
            {
                match self.tile_request_queue.read() {
                    Ok(trq) => { trq.queue.len().to_string() }, Err(e) => { "???".into() }
                }
            },
            self.observer.is_some(),
            self.mem_usage,
            self.disk_usage)
    }
}

// ---- TileCacheState ---------------------------------------------------------------------------

/// Needed when storing and restoring TileCache state at application startup and shutdown.
/// See TileCache::restore and TileCache::store for more info.
#[derive(Serialize, Deserialize, Debug)]
struct TileCacheState {
    // TileRequests representing Tiles.
    requests: Vec<TileRequest>,
    
    // Tile key to disk usage map.
    tile_disk_usages: HashMap<String, u64>,
}

impl TileCacheState {
    /// Create a snapshot from TileCache
    fn new(tcache: &TileCache) -> TileCacheState {
        let mut tcc = TileCacheState {
            requests: Vec::new(),
            tile_disk_usages: HashMap::new(),
        };
        
        // Convert Tiles to TileRequests for serialization.
        for (tile_id, ref tile) in &tcache.tiles {
            tcc.requests.push(TileRequest::new_from_tile(tile));
            tcc.tile_disk_usages.insert(tile_id.clone(), tile.disk_usage);
        }
        
        tcc
    }

    /// Apply tile cache state to tile cache.    
    fn apply(&mut self, tcache: &mut TileCache) {
        tcache.mem_usage = 0;
        tcache.disk_usage = 0;

        // Convert deserialized TileRequests to Tiles
        for treq in &self.requests {
            let mut tile = Tile::new_with_request(treq);
            tile.state = TileState::Flushed;
            tile.filepath = {
                match treq.to_cache_path() {
                    Ok(pathbuf) => { Some(pathbuf) },
                    Err(e) => { warn!("Failed to form tile cache path: {}", e); None }
                }
            };
            tile.disk_usage = *self.tile_disk_usages.get(&treq.to_key()).unwrap_or(&0u64);
            tcache.mem_usage += tile.estimate_mem_usage();
            tcache.disk_usage += tile.disk_usage;
            tcache.tiles.insert(treq.to_key(), tile);
        }
    }
}

// ---- Tile ---------------------------------------------------------------------------------------

/// Tile state.
#[derive(Clone, Ord, PartialOrd, PartialEq, Eq, Debug)]
pub enum TileState {
    // Without any real information or data.
    Void,

    /// Just created and waiting for a thread to process it.
    /// Contents of the tile is either black, approximated from a different zoom level
    /// or contains expired data.
    Pending,
    
    /// Content been loaded from tile source.
    Fetching,
    
    /// Content ready for use.
    Ready,
    
    /// Content loading resulted an error.
    Error,
    
    /// Content was removed from RAM but may be found on disk.
    Flushed,
}

/// Map tile which can be drawn always.
#[derive(Clone)]
pub struct Tile {
    /// State of the tile
    pub state: TileState,

    // Source where the tile was (or will be) loaded.
    source: TileSource,

    /// x coordinate (range is 0..2^z)
    x: u32,
    
    /// y coordinate (range is 0..2^z)
    y: u32,
    
    /// zoom level
    z: u8,
    
    /// High-dpi multiplier, usually 1
    mult: u8,
    
    /// Tile width as pixels
    width: i32,

    /// Tile height as pixels
    height: i32,
    
    /// Time when this tile was needed.
    access_time: DateTime<UTC>,
    
    /// Time when this tile expires.
    expire_time: DateTime<UTC>,
    
    /// Tile data as a byte array.
    data: Option<Box<[u8]>>,
    
    /// Tile data converted to a surface
    surface: Option<ImageSurface>,
    surface_none: Option<ImageSurface>,
    
    /// Path for disk cache tile file.
    filepath: Option<path::PathBuf>,
    
    /// Image file size on disk.
    disk_usage: u64,
}

impl Tile {
    /// Constructor from TileRequest.
    pub fn new_with_request(treq: &TileRequest) -> Tile {
        Tile{ state: TileState::Pending, 
              source: treq.source.clone(),
              x: treq.x, y: treq.y, z: treq.z, mult: treq.mult, 
              width: treq.source.tile_width, 
              height: treq.source.tile_width,
              access_time: UTC::now(),
              expire_time: UTC::now(), // TODO: future
              data: None,
              surface: None,
              surface_none: None,
              filepath: None,
              disk_usage: 0,
        }
    }

    /// Constructor a black tile for TileRequest.
    fn new(treq: &TileRequest, r: f64, g: f64, b: f64) -> Tile {
        // Create black isurface
        let isurface = ImageSurface::create(
            Format::ARgb32, 
            treq.source.tile_width, 
            treq.source.tile_height);
        let c = cairo::Context::new(&isurface);
        c.set_source_rgb(r, g, b);
        c.paint();

        // Return tile        
        Tile { 
            state: TileState::Pending, 
            source: treq.source.clone(),
            x: treq.x, y: treq.y, z: treq.z, mult: treq.mult, 
            width: treq.source.tile_width, 
            height: treq.source.tile_height,
            access_time: UTC::now(),
            expire_time: UTC::now(), // TODO: future
            data: None,
            surface: None,
            surface_none: None,
            filepath: {
                match treq.to_cache_path() {
                    Ok(pathbuf) => {
                        Some(pathbuf)
                    },
                    Err(e) => {
                        None
                    }
                }
            },
            disk_usage: 0,
        }
    }

    // Getters   
    pub fn x(&self) -> u32 { self.x }
    pub fn y(&self) -> u32 { self.y }
    pub fn z(&self) -> u8 { self.z }
    pub fn mult(&self) -> u8 { self.mult }
    pub fn width(&self) -> i32 { self.width }
    pub fn height(&self) -> i32 { self.height }

    /// Returns true if the tile is expired, false otherwise.
    pub fn is_expired(&self) -> bool {
        self.expire_time < UTC::now()
    }
    
    /// Return image surface. May involve an in-memory data conversion.
    pub fn get_surface(&mut self) -> Option<&ImageSurface> {
        if self.surface.is_none() {
            if let Some(data) = self.data.take() {
                let stride = cairo_format_stride_for_width(Format::ARgb32, self.width);
                let isurface = ImageSurface::create_for_data(data, |box_u8| { }, Format::ARgb32, self.width, self.height, stride);
                self.surface = Some(isurface);
            } else {
                return None;
            }
        }
        self.surface.as_ref()
    }

    /// Approximate a new tile by zooming this one in.
    fn zoom_in(&self, treq: &TileRequest) -> Tile {
        // Math
        let q2 = 1 << (self.z - treq.z) as i32;
        let offset_x = -self.width * (treq.x as i32 % q2);
        let offset_y = -self.height * (treq.y as i32 % q2);

        // Create a new
        let isurface = ImageSurface::create(Format::ARgb32, self.width, self.height);
        let c = cairo::Context::new(&isurface);
        c.scale(q2 as f64, q2 as f64);
        if let Some(ref self_surface) = self.surface {
            // Paint from source surface
            c.set_source_surface(self_surface, offset_x as f64, offset_y as f64);
            c.paint();
        } else {
            // Red tile in case of missing surface
            c.set_source_rgb(0.8, 0.0, 0.0);
            c.paint();
        }
        
        // Return
        Tile {
            state: TileState::Pending, 
            source: treq.source.clone(),
            x: treq.x, y: treq.y, z: treq.z, mult: treq.mult, 
            width: self.width, height: self.height,
            access_time: UTC::now(),
            expire_time: UTC::now(), // TODO: future
            data: None,
            surface: Some(isurface),
            surface_none: None,
            filepath: None,
            disk_usage: 0,
        }
    }
    
    /// Estimates memory usage of the tile in bytes.
    fn estimate_mem_usage(&self) -> usize {
        let mut u: usize = mem::size_of::<Tile>();
        if let Some(ref data) = self.data {
            u += data.len() as usize; // bytes
        }
        if self.surface.is_some() {
            u += (self.width * self.height * 4) as usize; // RGBA assumed
        }
        u
    }

    /// Remove cached tile data from memory    
    fn flush(&mut self) {
        self.data = None;
        self.surface = None;
    }
}

impl Ord for Tile {
    fn cmp(&self, other: &Self) -> Ordering {
        self.expire_time.cmp(&other.expire_time)
    }
}

impl PartialOrd for Tile {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Tile {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Tile {}

impl fmt::Debug for Tile {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let data_state = {
            if self.surface.is_some() { "surface" }
            else if self.data.is_some() { "data" }
            else { "empty" }
        };
        write!(f, "{{{},{} L{} {}x{} {} [{:?}]}}", 
            self.x, self.y, self.z, self.width, self.height, data_state, self.state)
    }
}

// ---- TileRequest --------------------------------------------------------------------------------

/// Cloneable TileRequest.
#[derive(Serialize, Deserialize, Clone)]
pub struct TileRequest {
    /// Tile request generation (a group of tile requests).
    generation: u64,

    /// Priority in generation.
    priority: u16,

    /// X-position
    x: u32,
    
    /// Y-position
    y: u32,
    
    /// Zoom level
    z: u8,
    
    /// High-dpi multiplier, usually 1
    mult: u8,

    /// Source where tiles are loaded.
    source: TileSource,    
}

impl TileRequest {
    /// Constructor for a tile request.
    pub fn new(generation: u64, priority: u16, x: u32, y: u32, z: u8, mult: u8, source: TileSource) -> TileRequest {
        TileRequest {
            generation: generation, priority: priority,
            x: x, y: y, z: z, mult: mult,
            source: source,
        }
    }
    
    /// Constructor for TileCacheState.
    fn new_from_tile(tile: &Tile) -> TileRequest {
        TileRequest {
            generation: 0, priority: 0,
            x: tile.x, y: tile.y, z: tile.z, mult: tile.mult,
            source: tile.source.clone(),
        }
    }
    
    /// Unique key of this tile
    fn to_key(&self) -> String {
        format!("{}/{}/{}/{}@{}", self.source.slug, self.z, self.y, self.x, self.mult)
    }

    /// Returns a copy of this with zoom level decreased and the (x,y) adjusted according to that.
    fn zoom_out(&self) -> TileRequest {
        TileRequest {
            generation: self.generation, priority: self.priority,
            x: self.x / 2, y: self.y / 2, z: self.z - 1, mult: self.mult,
            source: self.source.clone(),
        }
    }

    // Get tile path in disk cache. Also, ensure that the needed directory exists.
    fn to_cache_path(&self) -> Result<path::PathBuf, io::Error> {
        // Directory (ensure that it exists)
        let mut cache_path = settings_read().cache_directory();
        cache_path.push(&self.source.slug);
        
        // Zoom level directory 
        cache_path.push(self.z.to_string());
        
        // X and Y coordinate parts (max 256 items per subdirectory)
        if self.z <= 4 {
            fs::create_dir_all(&cache_path)?;
            cache_path.push(format!("{},{}", self.y, self.x));
        } else if self.z <= 8 {
            cache_path.push(self.y.to_string());
            fs::create_dir_all(&cache_path)?;
            cache_path.push(self.x.to_string());
        } else if self.z <= 16 {
            let name = format!("{:4x}{:4x}", self.y, self.x);
            cache_path.push(name[0..2].to_string());
            cache_path.push(name[2..4].to_string());
            cache_path.push(name[4..6].to_string());
            fs::create_dir_all(&cache_path)?;
            cache_path.push(name[6..8].to_string());
        } else if self.z <= 24 {
            let name = format!("{:6x}{:6x}", self.y, self.x);
            cache_path.push(name[0..2].to_string());
            cache_path.push(name[2..4].to_string());
            cache_path.push(name[4..8].to_string());
            cache_path.push(name[8..10].to_string());
            fs::create_dir_all(&cache_path)?;
            cache_path.push(name[10..12].to_string());
        } else {
            let name = format!("{:8x}{:8x}", self.y, self.x);
            cache_path.push(name[0..2].to_string());
            cache_path.push(name[2..4].to_string());
            cache_path.push(name[4..6].to_string());
            cache_path.push(name[6..8].to_string());
            cache_path.push(name[8..10].to_string());
            cache_path.push(name[10..12].to_string());
            cache_path.push(name[12..14].to_string());
            fs::create_dir_all(&cache_path)?;
            cache_path.push(name[14..16].to_string());
        }
        
        // Filename extension (if any)
        if let Some(ext_str) = self.source.to_filename_extension() {
            let ext = ffi::OsStr::new(ext_str.as_str());
            cache_path.set_extension(ext);
        }
        
        // Success
        Ok(cache_path)
    }

    /// True if the file exists on the disk, false if not or if there is an access error.   
    pub fn tile_exists_on_disk(&self) -> bool {
        match self.to_cache_path() {
            Ok(path_buf) => {
                path_buf.exists()
            },
            Err(e) => {
                false
            }
        }
    }
}

impl Ord for TileRequest {
    fn cmp(&self, other: &TileRequest) -> Ordering {
        if self.generation == other.generation {
            self.priority.cmp(&other.priority)
        } else {
            self.generation.cmp(&other.generation)
        }
    }
}

impl PartialOrd for TileRequest {
    fn partial_cmp(&self, other: &TileRequest) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for TileRequest {
    fn eq(&self, other: &TileRequest) -> bool {
        self.generation == other.generation && self.priority == other.priority
    }
}

impl Eq for TileRequest { }

impl fmt::Debug for TileRequest {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{{{},{} L{} {}}}", self.x, self.y, self.z, self.source.slug)
    }
}


// ---- TileRequestResult --------------------------------------------------------------------------

// Tile request result containing image data. This object doesn't live long.
#[derive(Clone)]
struct TileRequestResult {
    // The request resulting this instance.
    pub request: TileRequest,

    // Expire time
    pub expire_time: Option<DateTime<UTC>>,

    /// Image raw bitmap data
    pub data: Box<[u8]>,
    
    /// Tile width in pixels.
    pub tile_width: i32,
    
    /// Tile height in pixels.
    pub tile_height: i32,
    
    /// Error message
    pub error: String,

    // The original image file data.
    img_data: Option<Vec<u8>>
}

impl TileRequestResult {
    /// Non-error constructor.
    fn new(treq: &TileRequest, img_data: &mut Vec<u8>, expires: Option<DateTime<UTC>>) -> TileRequestResult {
        let mut tile_width: i32 = 0;
        let mut tile_height: i32 = 0;
        match convert_image_to_buffer(img_data, &mut tile_width, &mut tile_height) {
            Ok(raw_data) => {
                TileRequestResult {
                    request: treq.clone(),
                    expire_time: expires,
                    data: raw_data,
                    tile_width: tile_width,
                    tile_height: tile_height,
                    error: "".into(),
                    img_data: Some(img_data.clone()),
                }
            },
            Err(e) => {
                return Self::with_error(treq, e.to_string());
            }
        }
    }
    
    /// Create a new tile result from a tile on disk cache.
    fn new_from_file(treq: &TileRequest) -> Result<TileRequestResult, io::Error> {
        // Load image file
        let mut f = fs::File::open(treq.to_cache_path()?)?;
        let mut img_data: Vec<u8> = Vec::new();
        {
            img_data.reserve(16384); // TODO: actual size
            f.read_to_end(&mut img_data)?;
            debug!("Read {} bytes from file {}", img_data.len(), treq.to_cache_path().unwrap().to_str().unwrap_or("???"));
        }
    
        // Load metadata file
        let tmeta = {
            let mut meta_file_path = treq.to_cache_path()?;
            meta_file_path.set_extension("json");
            let tmeta: TileMetadata = deserialize_from(meta_file_path)?;
            tmeta
        };
    
        let mut tile_width: i32 = 0;
        let mut tile_height: i32 = 0;
        match convert_image_to_buffer(&mut img_data, &mut tile_width, &mut tile_height) {
            Ok(raw_data) => {
                Ok(TileRequestResult {
                    request: treq.clone(),
                    expire_time: tmeta.to_expire_time(),
                    data: raw_data,
                    tile_width: tile_width,
                    tile_height: tile_height,
                    error: "".into(),
                    img_data: Some(img_data.clone()),
                })
            },
            Err(e) => {
                return Err(io::Error::new(io::ErrorKind::Other, format!(
                    "Conversion from image data ({}) to image buffer failed: {}", 
                    treq.to_cache_path().unwrap().to_str().unwrap_or("???"),
                    e.to_string())));
            }
        }
    }
    
    /// Error constructor.
    fn with_error(treq: &TileRequest, err: String) -> TileRequestResult {
        TileRequestResult {
            request: treq.clone(),
            expire_time: None,
            data: Box::new([0u8]),
            tile_width: 0,
            tile_height: 0,
            error: err,
            img_data: None,
        }
    }
    
    /// Return TileRequest key.
    pub fn to_key(&self) -> String {
        self.request.to_key()
    }
    
    /// Returns true if the tile is expired, false otherwise.
    pub fn is_expired(&self) -> bool {
        if let Some(expire_time) = self.expire_time {
            expire_time < UTC::now()
        } else {
            false
        }
    }
    
    /// Save the file to the well-known location in the disk cache.
    fn save_to_disk(&self) -> Result<(), io::Error> {
        if let Some(ref img_data) = self.img_data {
            // Make path and ensure that it exists
            let mut cache_path = self.request.to_cache_path()?;
            debug!("cache img file: {}", cache_path.to_str().unwrap());

            // Save image file
            {
                let mut f = fs::File::create(&cache_path)?;
                f.write_all(img_data)?;
            }
            
            // Save meta data
            {
                let tmeta = TileMetadata::new(self);
                cache_path.set_extension("json");
                serialize_to(&tmeta, cache_path)?;
            }
            
            Ok(())
        } else {
            warn!("No img_data, can't save; {:?}", self.request);
            Ok(()) // Well...
        }
    }
}

// ---- TileMetadata -------------------------------------------------------------------------------

/// Tile metadata which is saved to disk in JSON format.
#[derive(Serialize, Deserialize, Debug)]
struct TileMetadata {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    expire_time: String,
}

impl TileMetadata {
    /// The constructor based on TileRequestResult.
    pub fn new(res: &TileRequestResult) -> TileMetadata {
        if let Some(time) = res.expire_time {
            TileMetadata { expire_time: time.to_rfc3339() }    
        } else {
            TileMetadata { expire_time: "".into() }
        }
    }

    /// Expire time getter.    
    pub fn to_expire_time(&self) -> Option<DateTime<UTC>> {
        if self.expire_time != "" {
            let utc = UTC::now();
            match DateTime::parse_from_rfc3339(self.expire_time.as_str()) {
                Ok(dt) => { return Some(dt.with_timezone(&utc.timezone())); }
                Err(e) => { warn!("Failed to parse tile metadata expire time: {}", e) }
            }
        }
        None
    }
}

// ---- TileThreadGlobal ---------------------------------------------------------------------------

/// Purpose of this struct is to simplify inter-thread communication.
struct TileThreadGlobal {
    tile_cache: Rc<RefCell<TileCache>>,
    receivers: Vec<Receiver<TileRequestResult>>,
}

// ---- TileRequestQueue ---------------------------------------------------------------------------

/// Representing a queue of tiles to be completed.
struct TileRequestQueue {
    queue: BTreeSet<TileRequest>, // OrderedSet would be ideal (maybe in the future)
    
    new_reqs_mutex: Arc<Mutex<u32>>,
    new_reqs_condvar: Arc<Condvar>,
}

// Declare a new thread local storage key
thread_local!(
    static GLOBAL: RefCell<Option<TileThreadGlobal>> = RefCell::new(None)
);

/// GTK main thread receives image data here.
fn receive_treq_result() -> glib::Continue {
    //debug!("receive_treq_result()");
    GLOBAL.with( |global| {
        if let Some(ref ttglobal) = *global.borrow() {
            let tcache = &ttglobal.tile_cache;
            for rx in &ttglobal.receivers {
                match rx.try_recv() {
                    Ok(treq_result) => {
                        // Save tile data. We clone the tile to avoid a mutable borrow of TileCache.
                        tcache.borrow_mut().handle_result(&treq_result);
                        
                        // Notify tile observer
                        if let Some(observer) = tcache.borrow().observer.clone() {
                            observer.borrow_mut().tile_loaded(&treq_result.request);
                        }
                    },
                    Err(e) => { 
                        if format!("{}", e) != "receiving on an empty channel" { // FIXME
                            warn!("Failed to receive from a worker thread: {}", e);
                        }
                    },
                }
            }
        }
    });
    glib::Continue(false) 
}

impl TileRequestQueue {
    /// Private constructor returning a reference counted locked object.
    fn new() -> Arc<RwLock<TileRequestQueue>> {
        let trqueue = Arc::new(RwLock::new(TileRequestQueue{ 
            queue: BTreeSet::new(),
            new_reqs_mutex: Arc::new(Mutex::new(0)),
            new_reqs_condvar: Arc::new(Condvar::new()),
        }));
        
        trqueue
    }
    
    fn init(&mut self, self_ar: Arc<RwLock<TileRequestQueue>>, tcache: Rc<RefCell<TileCache>>) {
        // Start worker threads        
        let n = settings_read().worker_threads();
        let mut http_client = Client::new();
        http_client.set_read_timeout(
            Some(time::Duration::from_secs(settings_read().tile_read_timeout)));
        let http_client_a = Arc::new(http_client);
        for i in 1..(n + 1) {

            // Put self into thread local storage
            let (tx, rx) = channel();
            let tcache_t = tcache.clone();
            GLOBAL.with( move |global| {
                let mut g = global.borrow_mut();
                if g.is_some() {
                    let mut gg = g.as_mut().unwrap();
                    gg.receivers.push(rx);
                } else {
                    *g = Some(TileThreadGlobal{tile_cache: tcache_t, receivers: vec![rx]});
                }
            });
        
            let trqueue_t = self_ar.clone();
            let http_client_t = http_client_a.clone();
            let nt_m  = self.new_reqs_mutex.clone();
            let nt_cv = self.new_reqs_condvar.clone();
            match thread::Builder::new().name(format!("worker-{}", i)).spawn( move || {
                loop {
                    // Wait for a pending tile to become available
                    {
                        let mut mu = nt_m.lock().unwrap();
                        while *mu < 1 {
                            mu = nt_cv.wait(mu).unwrap();
                        }
                    }
                    
                    // Get the tile and start processing
                    match trqueue_t.write() {
                        Ok(mut trqueue) => {
                            // Get the most urgent TileRequest
                            let treq = trqueue.pull_request();
                            
                            // Load tile from tile cache
                            let mut download_needed = true;
                            if treq.tile_exists_on_disk() {
                                debug!("Tile {} exists on disk", treq.to_key());
                                
                                // Load tile from file
                                match TileRequestResult::new_from_file(&treq) {
                                    Ok(res) => {
                                        let expired = res.is_expired();
                                    
                                        // Notify TileCache about the loaded tile
                                        glib::idle_add(receive_treq_result);
                                        match tx.send(res) {
                                            Ok(()) => { }, 
                                            Err(e) => {
                                                panic!("Send to TileCache failed: {}", e);
                                            }
                                        }
                                        
                                        // Check expiration
                                        if expired {
                                            debug!("Tile {} is expired", treq.to_key());
                                        } else {
                                            download_needed = false;
                                        }
                                    },
                                    Err(e) => {
                                        warn!("Failed to read tile from disk: {}", e);
                                    }
                                }
                            } else {
                                debug!("Tile {} doesn't exists on disk", treq.to_key());
                            }
                            
                            // Download the requested tile
                            if download_needed {
                                let res = treq.source.fetch_tile_data(&treq, &http_client_t);
                            
                                // Notify TileCache first
                                let res_cloned = res.clone();
                                glib::idle_add(receive_treq_result); // this has to be before the send and after the clone
                                match tx.send(res) {
                                    Ok(()) => { }, 
                                    Err(e) => {
                                        panic!("Send to TileCache failed: {}", e);
                                    }
                                }
                            
                                // Save image data to disk cache 
                                match res_cloned.save_to_disk() {
                                    Ok(()) => { },
                                    Err(e) => {
                                        warn!("Failed to save the tile to disk: {}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            panic!("Failed to unlock tile request queue: {}", e);
                        }
                    }
                }
            }) {
                Ok(join_handle) => {
                    debug!("Worker thread {} created", i);
                },
                Err(e) => {
                    panic!("Failed to create a worker thread: {}", e);
                }
            }
        }
    }

    fn push_request(&mut self, treq: &TileRequest) {
        self.queue.insert(treq.clone());
        self.incr_and_notify();
    }

    /// Returns the most urgent tile to be loaded and sets it to TileState::Prosessed before that.
    /// Blocks if there are not tiles to process.
    fn pull_request(&mut self) -> TileRequest {
        // Decrease available request count by one
        let mut mu = self.new_reqs_mutex.lock().unwrap();
        *mu -= 1;

        // Return the first request
        let treq = { 
            self.queue.iter().next().unwrap().clone()
        };
        self.queue.remove(&treq);
        treq
    }

    // Increment request count by one and notify worker threads    
    fn incr_and_notify(&self) {
        let mut mu = self.new_reqs_mutex.lock().unwrap();
        *mu += 1;
        self.new_reqs_condvar.notify_one();
    }
}

impl Drop for TileCache {
    fn drop(&mut self) {
    }
}

// ---- TileSource ---------------------------------------------------------------------------------

/// The network source where tiles are loaded.
#[derive(Serialize, Deserialize, Clone)]
pub struct TileSource {
    // File system friendly name
    pub slug: String,

    /// An array of mutually optional urls
    pub urls: Vec<String>,
    
    /// Token required by the service provider
    pub token: String,

    /// Tile width which has to be known
    pub tile_width: i32,
    
    /// Tile height which has to be known
    pub tile_height: i32,
}

impl TileSource {
    pub fn new(slug: String, urls: Vec<String>, token: String, tile_width: i32, tile_height: i32) -> TileSource {
        TileSource {
            slug: slug,
            urls: urls,
            token: token,
            tile_width: tile_width,
            tile_height: tile_height,
        }
    }

    /// Add a new url with vars.
    ///
    /// the following strings will be substituted:
    /// ${x} - x coordinate
    /// ${y} - y coordinate
    /// ${z} - zoom level
    /// ${token} - token required by the service provider
    pub fn add_url(&mut self, url: String) {
        self.urls.push(url);
    }
    
    /// Download tile data from the source. 
    fn fetch_tile_data(&self, treq: &TileRequest, client: &Arc<Client>) -> TileRequestResult {
        if self.urls.len() > 0 {
            let url = self.make_url(&treq).unwrap();
            let mut data: Vec<u8> = Vec::new();

            let mut expires = None; // TODO            
            if url.starts_with("file:") {
                // Load data from local disk 
                return TileRequestResult::with_error(treq, 
                    "File system based tile sources are not supperted yet".into()); // TODO
            } else {
                // Request tile data from a remote server with GET
                match client.get(url.as_str()).send() {
                    Ok(mut response) => {
                        debug!("Received response {} for tile {} data request for url {}", 
                                response.status, treq.to_key(), url);
                        if response.status == StatusCode::Ok {
                            data.reserve(16384);
                            match response.read_to_end(&mut data) {
                                Ok(size) => {
                                    debug!("Successfully read {} bytes of image data", data.len());
                                    
                                    // Get expires header
                                    if let Some(ref expires_header) = response.headers.get::<header::Expires>() {
                                        let timespec = (expires_header.0).0.to_timespec();
                                        expires = Some(UTC.timestamp(timespec.sec, timespec.nsec as u32));
                                        println!("expires_header: {}", expires_header);
                                    }
                                },
                                Err(e) => {
                                    error!("Failed to get tile from a remote server; {}", e);
                                    return TileRequestResult::with_error(treq, e.to_string());
                                }
                            }
                        } else {
                            return TileRequestResult::with_error(treq,
                                    format!("HTTP GET returned status code {}", response.status));
                        }
                    },
                    Err(e) => {
                        error!("Failed to get tile from a remote server; {}", e);
                        return TileRequestResult::with_error(treq, e.to_string());
                    },
                }
            }
            TileRequestResult::new(&treq, &mut data, expires)
        } else {
            TileRequestResult::with_error(treq, "No source urls!".into())
        }
    }
    
    /// Make a url substituting url variables with values from the TileRequest.
    pub fn make_url(&self, treq: &TileRequest) -> Result<String, String> {
        if self.urls.len() > 0 {
            let index = rand::thread_rng().gen::<usize>() % self.urls.len();
            let url = self.urls.get(index).unwrap();
            let url_with_vars = url.replace("${x}", &(format!("{}", treq.x).as_str()))
                                   .replace("${y}", &(format!("{}", treq.y).as_str()))
                                   .replace("${z}", &(format!("{}", treq.z).as_str()))
                                   .replace("${token}", self.token.as_str());
            Ok(url_with_vars)
        } else {
            Err(format!("No urls defined for the tile source {}", self.slug))
        }
    }
    
    /// Returns the extension of image file ("jpg", "png", etc).
    fn to_filename_extension(&self) -> Option<String> {
        if let Some(ref url) = self.urls.get(0) {
            let n = url.len();
            return Some(url[(n - 3) .. n].into()); // TODO: smarter way
        }
        
        None
    }
}

// ---- helpers --------------------------------------------------------------------------------------

/// Adapted from cairo-image-surface.c.
fn cairo_format_stride_for_width(format: Format, width: i32) -> i32 {
    assert!(format == Format::ARgb32);
    let bpp = 32;
    let cairo_stride_alignment = 4; // sizeof(uint32_t)
    let stride = (bpp * width + 7) / 8 + (cairo_stride_alignment - 1) & -cairo_stride_alignment;
    assert!(stride > 0);
    stride
}

/// Convert image file data (PNG/JPEG/GIF/etc) to a raw bitmap data.
/// Returns a tuple of (data, width, height).
fn convert_image_to_buffer(img_data: &mut Vec<u8>, width_out: &mut i32, height_out: &mut i32) -> Result<Box<[u8]>, String> {
    match image::load_from_memory(img_data.as_ref()) {
        Ok(dynamic_image) => {
            // Convert to a byte buffer
            let rgba_image = dynamic_image.to_rgba();
            *width_out = rgba_image.width() as i32;
            *height_out = rgba_image.height() as i32;
            let mut bu8 = rgba_image.into_raw().into_boxed_slice();
            
            // Reorder bytes
            for i in 0..(bu8.len()) { // TODO: in the future: .step_by(4)
                if i % 4 == 0 {
                    bu8.swap(i + 0, i + 2); // RGBA -> BGRA (Cairo expects this; ARGB32 in big-endian)
                    // TODO: what about big-endian machines? [cfg(target_endian="little")]
                }
            }
            
            // Return
            Ok(bu8)
        },
        Err(e) => {
            return Err(format!("Failed to read tile image data: {}", e));
        },
    }
}

// ---- tests --------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    extern crate env_logger;
    use std::sync::{Arc};
    use super::hyper::{Client};
    use super::*;

    #[test]
    fn test_tile_source() {
        // Initialize logger
        env_logger::init().unwrap();

        // Tile source
        let tile_source = TileSource::new(
            "osm-carto".into(),
            {   let mut urls: Vec<String> = Vec::new();
                urls.push("http://a.tile.openstreetmap.org/${z}/${x}/${y}.png".to_string());
                urls.push("http://b.tile.openstreetmap.org/${z}/${x}/${y}.png".to_string());
                urls
            },
            "".into(), 256,  256, );
        
        // Tile request
        let treq = TileRequest::new(1, 1, 0, 0, 1, 1, tile_source.clone());
        
        // Test making urls
        match tile_source.make_url(&treq) {
            Ok(url) => {
                assert!(url == "http://a.tile.openstreetmap.org/1/0/0.png" || 
                        url == "http://b.tile.openstreetmap.org/1/0/0.png");
            },
            Err(e) => {
                panic!(e.to_string());
            }
        }
        
        // Test GET
        let http_client_a = Arc::new(Client::new());
        let trr = tile_source.fetch_tile_data(&treq, &http_client_a);        
        assert!(trr.to_key() == treq.to_key());
        assert!(trr.data.len() > 4000);
        assert!(trr.error.len() == 0);
    }
}

