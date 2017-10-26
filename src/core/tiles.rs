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

/*

Tiles module solves two problems: 1) background tile loading & saving and 2) background 
tile processing, to not cause lag on the GTK main thread which takes care of map canvas 
rendering.

This module both manages tile cache and takes care of downloading new tiles from network 
using worker threads. It also converts the downloaded image files into image buffers that 
are given to the GTK main thread where those buffers are cached into Cairo ImageSurfaces 
that are used to render the map.

In the future it may be worth evaluating the option to use asynchronous crates (futures-rs, 
async hyper), to make this module more efficient and elegant. It may be better to wait for 
the crates to reach a stable version first, though, and there are many higher priority things 
to do first.


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
use std::collections::{HashMap, BTreeSet};
use std::thread;
use std::cmp::{Ordering, min, max};
use std::io::{Read};
use std::vec::{Vec};
use std::fmt;
use std::path;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::mem;
use std::time;
use self::chrono::{DateTime, UTC, TimeZone, Duration};
use self::hyper::header;
use self::hyper::{Client, Url};
use self::hyper::status::{StatusCode};
use self::rand::{Rng};
use self::cairo::{Format, ImageSurface};

use core::persistence::{serialize_to, deserialize_from, serialize_datetime, deserialize_datetime};
use core::settings::{settings_read, DEFAULT_TILE_EXPIRE_DAYS};

// ---- TileObserver -------------------------------------------------------------------------------

/// Callback-like trait for objects that want to get notified when a tile is fetched.
pub trait TileObserver {
    /// Notifies about a new tile loaded.
    fn tile_loaded(&self, treq: &TileRequest);
}

// ---- TileCache ----------------------------------------------------------------------------------
/// The main access point for tiles
pub struct TileCache {
    /// TileRequest::to_key -> Tile map
    tiles: HashMap<String, Tile>,

    /// The queue accessed by the worker threads    
    tile_request_queue: Arc<RwLock<TileRequestQueue>>,

    /// Object to be notified when new tiles are ready.    
    pub observer: Option<Rc<TileObserver>>,
    
    /// Disk used by the cached tiles.
    disk_usage: i64,
    
    /// Number of inserts since last flush check
    inserts_since_flush_check: u32,
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
            tiles: HashMap::new(),
            tile_request_queue: TileRequestQueue::new(),
            observer: None,
            disk_usage: 0,
            inserts_since_flush_check: 0,
        };
        tcache
    }

    /// Return tile for the given request. The result may be an approximation.    
    pub fn get_tile(&mut self, treq: &TileRequest) -> Option<&mut Tile> {
        if self.tiles.get(&treq.to_key()).is_some() {
            debug!("get_tile: {:?}, contains: {:?}", treq, self.tiles.get(&treq.to_key()) );
        } else {
            debug!("get_tile: {:?}, contains: --", treq);
        }
        
        // If the coordinates are out of bounds, return an empty tile
        if treq.y < 0 || treq.y >= (1 << treq.z) {
            return None;
        }
    
        let tile_key = treq.to_key();
        if self.tiles.contains_key(&tile_key) {
            // Check tile state
            match self.tiles.get(&tile_key).unwrap().state {
                TileState::Void => {
                    debug!("Loading a void tile: {}", tile_key);
                    let mut tile = Tile::new_with_request(treq);
                    tile.state = TileState::Pending;
                    self.tiles.insert(tile_key.clone(), tile);
                    self.tile_request_queue.write().unwrap().push_request(treq);
                }
                TileState::Pending => {
                    return Some(self.tiles.get_mut(&tile_key).unwrap())
                }
                TileState::Ready => {
                    let tile = self.tiles.get_mut(&tile_key).unwrap();
                
                    // Check tile expiration
                    if tile.is_expired() {
                        tile.state = TileState::Pending;
                        
                        // Request a tile from disk cache first, to get a temporary tile until 
                        // tile source request is completed
                        let mut treq2 = treq.clone();
                        treq2.tile_fetch_mode = TileFetchMode::Cache;
                        treq2.tile_state_on_success = TileState::Pending;
                        self.tile_request_queue.write().unwrap().push_request(&treq2);

                        // Make another request from tile source                        
                        debug!("Memory-cached tile expired, requesting an update: {}", tile_key);
                        let mut treq3 = treq.clone();
                        treq3.tile_fetch_mode = TileFetchMode::Remote;
                        self.tile_request_queue.write().unwrap().push_request(&treq3);
                    }
                    return Some(tile)
                }
                TileState::Error => {
                    return Some(self.tiles.get_mut(&tile_key).unwrap())
                }
                TileState::NonExistent => {
                    return Some(self.tiles.get_mut(&tile_key).unwrap())
                }
                TileState::Unauthorized => {
                    return Some(self.tiles.get_mut(&tile_key).unwrap())
                }
                TileState::Flushed => {
                    debug!("Reloading a flushed tile: {}", tile_key);
                    let mut tile = Tile::new_with_request(treq);
                    tile.state = TileState::Pending;

                    // Check tile expiration
                    if tile.is_expired() {
                        // Request a tile from disk cache first, to get a temporary tile until 
                        // tile source request is completed
                        let mut treq2 = treq.clone();
                        treq2.tile_fetch_mode = TileFetchMode::Cache;
                        treq2.tile_state_on_success = TileState::Pending;
                        self.tile_request_queue.write().unwrap().push_request(&treq2);
                    
                        // Make another request from tile source                        
                        debug!("Disk-cached tile expired, requesting an update: {}", tile_key);
                        let mut treq3 = treq.clone();
                        treq3.tile_fetch_mode = TileFetchMode::Remote;
                        self.tile_request_queue.write().unwrap().push_request(&treq3);
                    } else {
                        self.tile_request_queue.write().unwrap().push_request(treq);
                    }
                }
            }
        } else {
            debug!("Requesting a new tile: {}", tile_key);
            self.tile_request_queue.write().unwrap().push_request(treq);
        }
        
        // Approximate content by scaling
        let mut tile = Tile::new_with_request(treq);
        if treq.z > 0 {
            let mut treq_up = treq.zoom_out();
            let mut up_found = false;
            while treq_up.z >= 1 {
                let tile_key_up = treq_up.to_key();
                if self.tiles.contains_key(&tile_key_up) {
                    let tile_up = self.tiles.get(&tile_key_up).unwrap();
                    if tile_up.surface.is_some() && !tile_up.surface_is_temporary {
                        tile = tile_up.zoom_in(&treq);
                        up_found = true;
                        break;
                    }
                }
                treq_up = treq_up.zoom_out();
            }
            
            // If no upper tiles were found or if it was too high...
            if !up_found || treq.z - treq_up.z > 3 {
                // Create a black tile if there aren't loaded tiles above
                if !up_found {
                    tile = Tile::new_with_color(&treq, 0.2, 0.2, 0.2);
                }

                // Enqueue a new request to prepare for similar cases
                if treq.z > 0 {
                    // Request precautionary tiles some levels higher
                    self.queue_precautionary_request(&treq, -3,  0,  0, 1);
                    self.queue_precautionary_request(&treq, -3,  1,  0, 1);
                    self.queue_precautionary_request(&treq, -3, -1,  0, 1);
                    self.queue_precautionary_request(&treq, -3,  0, -1, 1);
                    self.queue_precautionary_request(&treq, -3,  0,  1, 1);
                    self.queue_precautionary_request(&treq, -6,  0,  0, 0);
                    self.queue_precautionary_request(&treq, -9,  0,  0, 0);
                    self.queue_precautionary_request(&treq, -12,  0,  0, 0);
                }
            } else {
                debug!("Created an approximation treq_up={:?}", treq_up);
            }
        } else {
            tile = Tile::new_with_color(&treq, 0.2, 0.0, 0.0);
        }        

        // Store tile and return
        self.tiles.insert(tile_key.clone(), tile);
        self.inserts_since_flush_check += 1;
        Some(self.tiles.get_mut(&tile_key).unwrap())
    }

    /// Clears any tile request which is not about the given level.
    pub fn focus_on_zoom_level(&mut self, zoom_level: u8) {
        // Clean queue
        let ref mut trq = self.tile_request_queue;
        let mut tile_keys: Vec<String> = Vec::with_capacity(trq.read().unwrap().queue.len());
        trq.write().unwrap().focus_on_zoom_level(zoom_level, &mut tile_keys);
        
        // Mark tiles void
        for ref key in tile_keys {
            if let Some(tile) = self.tiles.get_mut(key) {
                tile.state = TileState::Void;
            }
        }
    }

    /// Request a precautionary tile for the given tile offset. Does nothing if the 
    /// request is out of range.
    fn queue_precautionary_request(&mut self, base_treq: &TileRequest, delta_z: i8, delta_x: i8, delta_y: i8, delta_pri: i8) {
        // Do nothing if the requested zoom level is out of range
        if base_treq.z as i8 + delta_z < 0 {
            return;
        }
    
        // Delta-Z
        let mut treq = base_treq.clone();
        for i in 0..(-delta_z) {
            if treq.z > 0 {
                treq = treq.zoom_out();
            }
        }
        treq.precautionary = true;
        treq.generation = max(0, min(treq.generation as i64 + delta_pri as i64, i64::max_value())) as u64;
        let side = 1 << treq.z;
        
        // Delta-X
        treq.x += delta_x as i32;
        if treq.x < 0 || treq.x >= side { return }
        
        // Delta-Y
        treq.y += delta_y as i32;
        if treq.y < 0 || treq.y >= side { return }

        // Enqueue a request if tile is not being loaded
        if {
            if let Some(tile) = self.tiles.get(&treq.to_key()) {
                if tile.state == TileState::Pending || tile.state == TileState::Ready {
                    false
                } else {
                    true
                }
            } else {
                true
            }
        } {
            self.tile_request_queue.write().unwrap().push_request(&treq);
            let tile = Tile::new_with_color(&treq, 0.0, 0.0, 0.0);
            self.tiles.insert(treq.to_key(), tile);
        }
    }
    
    /// Handle image fetch result from a worker thread. Returns true if the observer should be 
    /// notified.
    fn handle_result(&mut self, treq_result: &TileRequestResult) -> bool {
        // Assign tile information
        if let Some(ref mut tile) = self.tiles.get_mut(&treq_result.to_key()) {
            match treq_result.code {
                TileRequestResultCode::Ok => {
                    // Assign tile data
                    let treq = &treq_result.request;
                    let old_tile_disk_usage = tile.disk_usage;
                    tile.state = treq.tile_state_on_success;
                    tile.data = Some(treq_result.data.clone());
                    tile.width = treq_result.tile_width;
                    tile.height = treq_result.tile_height;
                    tile.expire_time = {
                        if let Some(treq_expire_time) = treq_result.expire_time {
                            treq_expire_time
                        } else {
                            tile.expire_time
                        }
                    };
                    tile.filepath = {
                        match treq_result.request.to_cache_path() 
                            { Ok(pathbuf) => { Some(pathbuf) }, Err(e) => { None } }
                    };
                    tile.disk_usage = {
                        if let Some(ref img_data) = treq_result.img_data { img_data.len() } else { 0 }
                    } as i64;
                    self.disk_usage = self.disk_usage + tile.disk_usage - old_tile_disk_usage
                },
                TileRequestResultCode::TransmissionError => {
                    let mut treq = treq_result.request.clone();
                    treq.generation = 0;
                    treq.priority = 0;
                    if let Some(retry) = treq.retry_count {
                        if retry > 0 {
                            treq.retry_count = Some(retry - 1);
                            treq.priority = (retry - 1) as i64;
                            self.tile_request_queue.write().unwrap().push_request(&treq);
                            debug!("Retrying loading tile {} ({} tries left)", treq.to_key(), retry - 1);
                        } else {
                            // FAIL
                            tile.state = TileState::Error;
                            warn!("Failed to load tile {} after several retries", treq.to_key());
                            return false;
                        }
                    } else {
                        let retry_count = settings_read().http_retry_count;
                        treq.retry_count = Some(retry_count);
                        treq.priority = retry_count as i64;
                        self.tile_request_queue.write().unwrap().push_request(&treq);
                        debug!("Retrying tile {} loading for {} times", treq.to_key(), retry_count);
                    }
                    return false;
                },
                TileRequestResultCode::NotFoundError => {
                    tile.state = TileState::NonExistent;
                    tile.paint_with_color(0.4, 0.4, 0.4);
                    return false;
                },
                TileRequestResultCode::NoSourceError => {
                    tile.state = TileState::NonExistent;
                    tile.paint_with_color(0.5, 0.4, 0.4);
                    return false;
                },
                TileRequestResultCode::UnauthorizedError => {
                    tile.state = TileState::Unauthorized;
                    tile.paint_with_color(1.0, 0.9, 0.8);
                    return false;
                },
                TileRequestResultCode::UnknownError => {
                    tile.state = TileState::Error;
                    tile.paint_with_color(0.7, 0.0, 0.8);
                    return false;
                },
            }
        } else {
            warn!("Received image data fetch for tile {} but tile isn't in cache!", 
                treq_result.to_key());
        }
        
        return true;
    }

    /// Flush caches if their memory usage is too high.
    pub fn check_cache(&mut self, force: bool) {
        // Run the check only if there are enough inserts
        if self.inserts_since_flush_check < 100 && !force { // TODO: non-hardcoded limit
            return;
        } else {
            self.inserts_since_flush_check = 0;
            debug!("Flushing tile cache...");
        }
    
        // Create a vector ordered by access time and count mem usage
        let mut tord: Vec<TileOrd> = Vec::with_capacity(self.tiles.len());
        let mut mem_usage = 0;
        for (ref tile_key, ref mut tile) in self.tiles.iter_mut() {
            tord.push(TileOrd::new_with_access_time(*tile_key, tile));
            mem_usage += tile.estimate_mem_usage();
        }
        tord.sort_by(|a, b| a.cmp(b) ); // For a temporary collection Vector is likely faster than using a BTreeSet
    
        // Mem-flush a tile which has been accessed the longest time ago
        if let Some(mem_capacity) = settings_read().tile_mem_cache_capacity {
            if mem_usage > mem_capacity {
                // Flush some tiles
                for to in &tord {
                    if mem_usage <= mem_capacity {
                        break;
                    }
                    
                    if let Some(tile) = self.tiles.get_mut(&to.key) {
                        // Flush only lower tiles
                        if tile.z > 3 && tile.flushable() {
                            let tmu0 = tile.estimate_mem_usage();
                            tile.flush();
                            let delta_mem_usage = tile.estimate_mem_usage() - tmu0;
                            mem_usage += delta_mem_usage;
                            debug!("Flushed mem cache tile {:?} {} -> {} bytes ({})", 
                                tile,
                                mem_usage - delta_mem_usage, mem_usage, delta_mem_usage);
                        }
                    } else {
                        warn!("Tile missing for key: {}", to.key);
                    }
                }
            }
        }
        
        // Disk-flush a tile which was accessed the longest time ago
        if let Some(disk_capacity) = settings_read().tile_disk_cache_capacity {
            if self.disk_usage > disk_capacity {
                // Flush the tiles starting from the beginning until cache size gets small enough
                for to in &tord {
                    let mut delete_tile = false;
                    {
                        if let Some(tile) = self.tiles.get_mut(&to.key) {
                            if let Some(filepath) = tile.filepath.clone() {
                                let mut file_size: i64 = 0; // false warning
                                if filepath.exists() {
                                    match fs::File::open(&filepath) {
                                        Ok(f) => {
                                            match f.metadata() {
                                                Ok(metadata) => {
                                                    // Get file size
                                                    file_size = metadata.len() as i64;
                                                    
                                                    // Delete file
                                                    match fs::remove_file(&filepath) {
                                                        Ok(()) => { 
                                                            self.disk_usage -= file_size;
                                                            delete_tile = true;
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
                            }
                        } else {
                            warn!("Tile not found for key: {}", &to.key);
                        }
                    }
                    
                    // Delete tile from cache                        
                    if delete_tile {
                        self.tiles.remove(&to.key);
                    }

                    // Stop if flushing target reached
                    if self.disk_usage <= disk_capacity { break; }
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
        match deserialize_from::<TileCacheState, path::PathBuf>(pathbuf.clone()) {
            Ok(mut tcstate) => {
                tcstate.apply(self);
                debug!("Tile cache restored: {:?}", self);
                
                // Remove the state file to get the cache cleared on a possible crash
                match fs::remove_file(pathbuf) {
                    Ok(()) => { },
                    Err(e) => {
                        warn!("Failed to delete cache state file: {}", e);
                    }
                }
            },
            Err(e) => {
                warn!("Failed to restore tile cache state: {}", e);
                
                // Clear cache because it may be in an inconsistent state after a crash
                info!("Clearing tile cache");
                match fs::remove_dir_all(settings_read().cache_directory()) { // TODO: less dangerous approach
                    Ok(()) => { },
                    Err(e) => {
                        warn!("Failed to clear cache directory: {}", e);
                    }
                }
            }
        }
    }
}

impl fmt::Debug for TileCache {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "tiles={} queue.len={} observer={} disk_usage={}",
            self.tiles.len(),
            {
                match self.tile_request_queue.read() {
                    Ok(trq) => { trq.queue.len().to_string() }, Err(e) => { "???".into() }
                }
            },
            self.observer.is_some(),
            self.disk_usage)
    }
}

// ---- TileCacheState -----------------------------------------------------------------------------

/// Needed when storing and restoring TileCache state at application startup and shutdown.
/// See TileCache::restore and TileCache::store for more info.
#[derive(Serialize, Deserialize, Debug)]
struct TileCacheState {
    /// The tiles
    tiles: HashMap<String, Tile>,
}

impl TileCacheState {
    /// Create a snapshot from TileCache
    fn new(tcache: &TileCache) -> TileCacheState {
        TileCacheState {
            tiles: tcache.tiles.clone(),
        }
    }

    /// Apply tile cache state to tile cache.    
    fn apply(&mut self, tcache: &mut TileCache) {
        tcache.tiles = self.tiles.clone();
        tcache.disk_usage = 0;

        // Make Tiles tiles flushed and fill the usage fields
        let mut black_surface: Option<cairo::ImageSurface> = None;
        for (_, tile) in &mut tcache.tiles {
            tile.state = TileState::Flushed;
            if black_surface.is_some() {
                tile.surface = black_surface.clone();
            } else {
                // Actually we are sharing the same surface with several tiles (to save memory)
                tile.paint_with_color(0.0, 0.0, 0.0);
                black_surface = tile.surface.clone();
            }
            tile.surface_is_temporary = true;
            tcache.disk_usage += tile.disk_usage;
        }
    }
}

// ---- Tile ---------------------------------------------------------------------------------------

/// Tile state.
#[derive(Copy, Clone, Serialize, Deserialize, Ord, PartialOrd, PartialEq, Eq, Debug)]
pub enum TileState {
    // Without any real information or data.
    Void,

    /// Tile created and waiting for a thread to process it.
    /// Contents of the tile is either black, approximated from a different zoom level
    /// or contains expired data.
    Pending,
    
    /// Content ready for use.
    Ready,
    
    /// Content loading resulted an error which is irrecovable.
    Error,
    
    /// The tile doesn't exist on the source or the source doesn't exist.
    NonExistent,

    /// Tile source denied access.
    Unauthorized,
    
    /// Content was removed from RAM but may be found on disk.
    Flushed,
}

/// Map tile which can be drawn always.
#[derive(Clone, Serialize, Deserialize)]
pub struct Tile {
    /// State of the tile
    pub state: TileState,

    /// x coordinate (range is 0..2^z)
    x: i32,
    
    /// y coordinate (range is 0..2^z)
    y: i32,
    
    /// zoom level
    z: u8,
    
    /// High-dpi multiplier, usually 1
    mult: u8,
    
    /// Tile width as pixels
    width: i32,

    /// Tile height as pixels
    height: i32,
    
    /// Time when this tile was needed. This should be modified either before the tile is
    #[serde(serialize_with = "serialize_datetime", deserialize_with = "deserialize_datetime")]
    access_time: DateTime<UTC>,
    
    /// Time when this tile expires.
    #[serde(serialize_with = "serialize_datetime", deserialize_with = "deserialize_datetime")]
    expire_time: DateTime<UTC>,
    
    /// Tile data as a byte array.
    #[serde(skip_serializing, skip_deserializing)]
    data: Option<Box<[u8]>>,
    
    /// Tile data converted to a surface
    #[serde(skip_serializing, skip_deserializing)]
    surface: Option<ImageSurface>,

    /// Returned a reference to this when there is no surface
    #[serde(skip_serializing, skip_deserializing)]
    surface_none: Option<ImageSurface>,
    
    /// True if the surface field content is temporary until the actual data is loaded.
    #[serde(skip_serializing, skip_deserializing)]
    surface_is_temporary: bool,
    
    /// Path for disk cache tile file.
    filepath: Option<path::PathBuf>,
    
    /// Image file size on disk.
    disk_usage: i64,
}

impl Tile {
    /// Constructor from TileRequest.
    pub fn new_with_request(treq: &TileRequest) -> Tile {
        Tile{ state: TileState::Pending, 
              x: treq.wrap_x(), y: treq.y, z: treq.z, mult: treq.mult, 
              width: treq.source.tile_width, 
              height: treq.source.tile_width,
              access_time: UTC::now(),
              expire_time: UTC::now() + Duration::days(DEFAULT_TILE_EXPIRE_DAYS),
              data: None,
              surface: None,
              surface_none: None,
              surface_is_temporary: false,
              filepath: None,
              disk_usage: 0,
        }
    }

    /// Constructor a black tile for TileRequest.
    fn new_with_color(treq: &TileRequest, r: f64, g: f64, b: f64) -> Tile {
        // Return tile        
        let mut tile = Tile { 
            state: TileState::Pending, 
            x: treq.wrap_x(), y: treq.y, z: treq.z, mult: treq.mult, 
            width: treq.source.tile_width, 
            height: treq.source.tile_height,
            access_time: UTC::now(),
            expire_time: UTC::now() + Duration::days(DEFAULT_TILE_EXPIRE_DAYS),
            data: None,
            surface: None,
            surface_none: None,
            surface_is_temporary: false,
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
        };
        tile.paint_with_color(r, g, b);
        tile.surface_is_temporary = true;
        tile
    }

    /// Creates a surface which is entirely black.
    fn paint_with_color(&mut self, r: f64, g: f64, b: f64) {
        let isurface = ImageSurface::create(Format::ARgb32, self.width, self.height);
        let c = cairo::Context::new(&isurface);
        c.set_source_rgb(r, g, b);
        c.paint();
        self.surface = Some(isurface);
    }

    // Getters   
    pub fn x(&self) -> i32 { self.x }
    pub fn y(&self) -> i32 { self.y }
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
        if self.surface.is_none() || (self.surface_is_temporary && self.data.is_some()) {
            if let Some(data) = self.data.take() {
                let stride = cairo_format_stride_for_width(Format::ARgb32, self.width);
                let isurface = ImageSurface::create_for_data(
                    data, |box_u8| { }, Format::ARgb32, self.width, self.height, stride);
                self.surface = Some(isurface);
                self.surface_is_temporary = false;
            } else {
                return None;
            }
        }
        self.surface.as_ref()
    }

    /// Scale and crop surface of this tile to meet the requirements of treq.
    fn zoom_in(&self, treq: &TileRequest) -> Tile {
        // Math
        let q2 = 1 << (treq.z - self.z) as i32;
        let offset_x = (-self.width * (treq.x as i32 % q2) / q2) as f64;
        let offset_y = (-self.height * (treq.y as i32 % q2) / q2) as f64;
        let q2f = q2 as f64;
        let q2m = q2 >> 1;
        let q2mf = q2m as f64;

        // Crop and scale self to a new surface
        let isurface = ImageSurface::create(Format::ARgb32, self.width, self.height);
        let c = cairo::Context::new(&isurface);
        if let Some(ref self_surface) = self.surface {
            // Paint from source surface
            if true {
                // This is a mediocre workaround to get rid of scaling artifacts on edges.
                // There are couple ways that may fix this but none of them is supported by
                // gtk-rs binding at moment:
                //  1) convert tile to Pixbuf and scale it with InterpType::NEAREST
                //  2) set SurfacePattern as source
                //  3) something even smarter
                let tsurface = ImageSurface::create(Format::Rgb24, self.width + q2m * 2, self.height + q2m * 2);
                {
                    let c = cairo::Context::new(&tsurface);
                    let w = self.width;
                    let h = self.height;
                    if let Some(ref self_surface) = self.surface {
                        // Background with medium gray for the corners
                        c.set_source_rgb(0.5, 0.5, 0.5);
                        c.paint();
                        
                        // Copy the tile to the sides of the "frame"
                        for (x, y, w, h, xp, yp) in vec![
                            // west edge
                            (0.0, q2mf, 
                             q2mf, self.height as f64, 
                             0.0, q2mf), 
                             
                            // east edge
                            (q2mf + self.width as f64, q2mf, 
                             q2mf, self.height as f64, 
                             q2mf * 2.0, q2mf), 
                             
                            // north edge
                            (q2mf, 0.0, 
                             self.width as f64, q2mf, 
                             q2mf, 0.0), 
                             
                            // south edge
                            (q2mf, q2mf + self.height as f64, 
                             self.width as f64, q2mf, 
                             q2mf, q2mf * 2.0), 
                        ] {
                            c.rectangle(x, y, w, h);
                            c.clip();
                            c.set_source_surface(self_surface, xp, yp);
                            c.paint();
                            c.reset_clip();
                        }
                    
                        // Copy tile to the center of the surface
                        c.set_source_surface(self_surface, q2mf, q2mf);
                        c.paint();
                    } else {
                        // Blue tile in case of missing surface (that shouldn't happen)
                        c.set_source_rgb(0.0, 0.0, 0.8);
                        c.paint();
                    }
                }
            
                // Complicated scaling resulting no noticeable seams
                c.scale(q2f, q2f);
                c.translate(-q2mf, -q2mf);
                c.set_source_surface(&tsurface, offset_x, offset_y);
                c.paint();
                debug!("zoom_in: q2={} offset={},{}", q2, offset_x, offset_y);
            } else {
                // Simple scaling resulting unwanted seams
                c.scale(q2f, q2f);
                c.set_source_surface(self_surface, offset_x, offset_y);
                c.paint();
                debug!("zoom_in: q2={} offset={},{}", q2, offset_x, offset_y);
            }
        } else {
            // Blue tile in case of missing surface (that shouldn't happen)
            c.set_source_rgb(0.0, 0.0, 0.8);
            c.paint();
        }
        
        // Return
        Tile {
            state: TileState::Pending, 
            x: treq.x, y: treq.y, z: treq.z, mult: treq.mult, 
            width: self.width, height: self.height,
            access_time: UTC::now(),
            expire_time: UTC::now(), // TODO: future
            data: None,
            surface: Some(isurface),
            surface_none: None,
            surface_is_temporary: true,
            filepath: None,
            disk_usage: 0,
        }
    }
    
    /// Estimates memory usage of the tile in bytes.
    fn estimate_mem_usage(&self) -> isize {
        let mut u: isize = mem::size_of::<Tile>() as isize;
        if let Some(ref data) = self.data {
            u += data.len() as isize; // bytes
        }
        if self.surface.is_some() && !self.surface_is_temporary {
            u += (self.width * self.height * 4) as isize; // RGBA assumed
        }
        u
    }

    /// True if flushing reduces memory usage, false otherwise.
    fn flushable(&self) -> bool {
        self.data.is_some() || self.surface.is_some()
    }

    /// Remove cached tile data from memory    
    fn flush(&mut self) {
        self.data = None;
        self.surface = None;
        self.surface_is_temporary = false;
        self.state = TileState::Flushed;
    }
}

impl fmt::Debug for Tile {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let data_state = {
            if self.surface.is_some() { 
                if self.surface_is_temporary {
                    "surface(tmp)" 
                } else {
                    "surface"
                }
            } else if self.data.is_some() { 
                "data" 
            } else { 
                "empty" 
            }
        };
        write!(f, "{{{},{} L{} {}x{} {} [{:?}]}}", 
            self.x, self.y, self.z, self.width, self.height, data_state, self.state)
    }
}

// ---- TileInfoOrd --------------------------------------------------------------------------------

pub struct TileOrd {
    key: String,
    datetime: DateTime<UTC>,
}

impl TileOrd {
    pub fn new_with_access_time(tile_key: &String, tile: &Tile) -> TileOrd {
        TileOrd {
            key: tile_key.clone(),
            datetime: tile.access_time,
        }
    }
    
    pub fn new_with_expire_time(tile_key: &String, tile: &Tile) -> TileOrd {
        TileOrd {
            key: tile_key.clone(),
            datetime: tile.expire_time,
        }
    }
}

impl Ord for TileOrd {
    fn cmp(&self, other: &Self) -> Ordering {
        self.datetime.cmp(&other.datetime)
    }
}

impl PartialOrd for TileOrd {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for TileOrd {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for TileOrd {}


// ---- TileRequest --------------------------------------------------------------------------------

/// The source where the tile is expected to be retrieved.
#[derive(Ord, PartialOrd, PartialEq, Eq, Copy, Clone)]
pub enum TileFetchMode {
    Remote,
    Any,
    Cache,
}

/// Cloneable TileRequest.
#[derive(Clone)]
pub struct TileRequest {
    /// Tile request generation (a group of tile requests). Affects to the order in the request queue.
    generation: u64,

    /// Priority within the generation. Affects to the order in the request queue.
    priority: i64,
    
    /// X-position
    pub x: i32,
    
    /// Y-position
    pub y: i32,
    
    /// Zoom level
    pub z: u8,
    
    /// High-dpi multiplier, usually 1
    pub mult: u8,

    /// Source where tiles are loaded.
    pub source: TileSource,
    
    /// Load tile from the source even if it was found in disk cache.
    tile_fetch_mode: TileFetchMode,
    
    /// Tile state to be set if tile fetching succeeds.
    tile_state_on_success: TileState,
    
    /// True if surface should be created after loading.
    precautionary: bool,
    
    /// Retry count. This is decreased every time when retried.
    retry_count: Option<u8>,
}

impl TileRequest {
    /// Constructor for a tile request.
    pub fn new(generation: u64, priority: i64, x: i32, y: i32, z: u8, mult: u8, source: TileSource) -> TileRequest {
        TileRequest {
            generation: generation, priority: priority, 
            x: x, y: y, z: z, mult: mult,
            source: source,
            tile_fetch_mode: TileFetchMode::Any,
            tile_state_on_success: TileState::Ready,
            precautionary: false,
            retry_count: None,
        }
    }
    
    /// If x is out of bounds wrap it.
    pub fn wrap_x(&self) -> i32 {
        let mut x = self.x;
        let w = 1 << self.z;
        while x < 0 {
            x += w;
        }
        if x >= w {
            x = x % w;
        }
        x
    }
    
    /// Unique key of this tile
    fn to_key(&self) -> String { // TODO: instead of a String a custom data type would be faster
        format!("{}/{}/{}/{}@{}", self.source.slug, self.z, self.y, self.wrap_x(), self.mult)
    }

    /// Returns a copy of this with zoom level decreased and the (x,y) adjusted according to that.
    fn zoom_out(&self) -> TileRequest {
        TileRequest {
            generation: self.generation, priority: self.priority,
            x: self.wrap_x() / 2, y: self.y / 2, z: self.z - 1, mult: self.mult,
            source: self.source.clone(), tile_fetch_mode: self.tile_fetch_mode,
            tile_state_on_success: TileState::Ready,
            precautionary: false,
            retry_count: None,
        }
    }

    // Get tile path in disk cache. Also, ensure that the needed directory exists.
    fn to_cache_path(&self) -> Result<path::PathBuf, io::Error> {
        // Directory (ensure that it exists)
        let mut cache_path = settings_read().cache_directory();
        cache_path.push(&self.source.slug);
        
        // Zoom level directory 
        cache_path.push(format!("{:02}", self.z));
        
        // X and Y coordinate parts (max 256 items per subdirectory)
        if self.z <= 4 {
            fs::create_dir_all(&cache_path)?;
            cache_path.push(format!("{},{}", self.y, self.wrap_x()));
        } else if self.z <= 8 {
            cache_path.push(self.y.to_string());
            fs::create_dir_all(&cache_path)?;
            cache_path.push(self.x.to_string());
        } else if self.z <= 16 {
            let name = format!("{:04x}{:04x}", self.y, self.wrap_x());
            cache_path.push(name[0..2].to_string());
            cache_path.push(name[2..4].to_string());
            cache_path.push(name[4..6].to_string());
            fs::create_dir_all(&cache_path)?;
            cache_path.push(name[6..8].to_string());
        } else if self.z <= 24 {
            let name = format!("{:06x}{:06x}", self.y, self.wrap_x());
            cache_path.push(name[0..2].to_string());
            cache_path.push(name[2..4].to_string());
            cache_path.push(name[4..6].to_string());
            cache_path.push(name[6..8].to_string());
            cache_path.push(name[8..10].to_string());
            fs::create_dir_all(&cache_path)?;
            cache_path.push(name[10..12].to_string());
        } else {
            let name = format!("{:08x}{:08x}", self.y, self.wrap_x());
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
    /// Compare tile requests based on generation, priority and other distinctive values.
    fn cmp(&self, other: &TileRequest) -> Ordering {
        if self.generation == other.generation {
            if self.priority == other.priority {
                if self.z == other.z {
                    if self.y == other.y {
                        if self.wrap_x() == other.wrap_x() {
                            self.tile_fetch_mode.cmp(&other.tile_fetch_mode)
                        } else {
                            self.wrap_x().cmp(&other.wrap_x())
                        }
                    } else {
                        self.y.cmp(&other.y)
                    }
                } else {
                    self.z.cmp(&other.z)
                }
            } else {
                self.priority.cmp(&other.priority)
            }
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
        let extra = {
            match self.tile_fetch_mode {
                TileFetchMode::Any => { " from-any" }
                TileFetchMode::Cache => { " from-cache" }
                TileFetchMode::Remote => { " from-remote" }
            }
        };
        write!(f, "{{{},{} L{} {}{} gen={} pri={}}}", self.wrap_x(), self.y, self.z, self.source.slug, extra, self.generation, self.priority)
    }
}

// ---- TileRequestResult --------------------------------------------------------------------------

/// Result codes of TileRequestResult. 
#[derive(Copy, Clone, Serialize, Deserialize, Ord, PartialOrd, PartialEq, Eq, Debug)]
pub enum TileRequestResultCode {
    Ok,
    TransmissionError,
    NotFoundError,
    NoSourceError,
    UnauthorizedError,
    UnknownError,
}

/// Tile request result containing image data. This object doesn't live long.
#[derive(Clone)]
struct TileRequestResult {
    pub code: TileRequestResultCode,

    // The request resulting this instance.
    pub request: TileRequest,

    // Expire time
    pub expire_time: Option<DateTime<UTC>>,

    /// Image raw bitmap data
    data: Box<[u8]>,
    
    /// Tile width in pixels.
    pub tile_width: i32,
    
    /// Tile height in pixels.
    pub tile_height: i32,
    
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
                    code: TileRequestResultCode::Ok,
                    request: treq.clone(),
                    expire_time: expires,
                    data: raw_data,
                    tile_width: tile_width,
                    tile_height: tile_height,
                    img_data: Some(img_data.clone()),
                }
            },
            Err(e) => {
                return Self::with_code(treq, TileRequestResultCode::TransmissionError);
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
            debug!("Read {} bytes from file {}", img_data.len(), 
                treq.to_cache_path().unwrap().to_str().unwrap_or("???"));
        }
    
        let mut tile_width: i32 = 0;
        let mut tile_height: i32 = 0;
        match convert_image_to_buffer(&mut img_data, &mut tile_width, &mut tile_height) {
            Ok(raw_data) => {
                Ok(TileRequestResult {
                    code: TileRequestResultCode::Ok,
                    request: treq.clone(),
                    expire_time: None,
                    data: raw_data,
                    tile_width: tile_width,
                    tile_height: tile_height,
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
    fn with_code(treq: &TileRequest, code: TileRequestResultCode) -> TileRequestResult {
        TileRequestResult {
            code: code,
            request: treq.clone(),
            expire_time: None,
            data: Box::new([0u8]),
            tile_width: 0,
            tile_height: 0,
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
            let cache_path = self.request.to_cache_path()?;
            debug!("cache img file: {}", cache_path.to_str().unwrap());

            // Save image file
            let mut f = fs::File::create(&cache_path)?;
            f.write_all(img_data)?;
            
            Ok(())
        } else {
            warn!("No img_data, can't save; {:?}", self.request);
            Ok(()) // Well...
        }
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
                        let notify = tcache.borrow_mut().handle_result(&treq_result);
                        
                        // Notify tile observer
                        if notify {
                            let treq = treq_result.request;
                            if treq.precautionary {
                                // Precautionary tiles should be loaded beforehand
                                if let Some(ref mut tile) = tcache.borrow_mut().tiles.get_mut(&treq.to_key()) {
                                    tile.get_surface();
                                } else {
                                    warn!("Precautionary tile not found for: {}", treq.to_key());
                                }
                            }
                            
                            // Notify tile observer
                            if let Some(observer) = tcache.borrow().observer.clone() {
                                observer.tile_loaded(&treq);
                            }
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
        // HTTP client
        let mut http_client = settings_read().http_client(false);
        http_client.set_read_timeout(
            Some(time::Duration::from_secs(settings_read().tile_read_timeout)));
        http_client.set_write_timeout(
            Some(time::Duration::from_secs(settings_read().tile_write_timeout)));
        let http_client_a = Arc::new(http_client);

        // HTTPS client
        let mut https_client = settings_read().http_client(true);
        https_client.set_read_timeout(
            Some(time::Duration::from_secs(settings_read().tile_read_timeout)));
        https_client.set_write_timeout(
            Some(time::Duration::from_secs(settings_read().tile_write_timeout)));
        let https_client_a = Arc::new(https_client);
        
        // Start worker threads        
        let n = settings_read().worker_threads();
        for i in 1..(n + 1) {

            // Put self into thread local storage
            let (tx, rx) = channel();
            let tcache_t = tcache.clone();
            GLOBAL.with( move |global| {
                let mut g = global.borrow_mut();
                if g.is_some() {
                    let gg = g.as_mut().unwrap();
                    gg.receivers.push(rx);
                } else {
                    *g = Some(TileThreadGlobal{tile_cache: tcache_t, receivers: vec![rx]});
                }
            });

            // Start the worker threads        
            let trqueue_t = self_ar.clone();
            let http_client_t = http_client_a.clone();
            let https_client_t = https_client_a.clone();
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
                    
                    // Lock the queue to get the tile request
                    let mut treq_o: Option<TileRequest> = None;
                    match trqueue_t.write() {
                        Ok(mut trqueue) => {
                            // Get the most urgent TileRequest
                            treq_o = trqueue.pull_request()
                        }
                        Err(e) => {
                            warn!("Failed to unlock tile request queue: {}", e);
                        }
                    }

                    // Start processing the request
                    if let Some(treq) = treq_o {
                        debug!("treq={:?} trq={:?}", treq, *trqueue_t.read().unwrap());
                    
                        // Load tile from tile cache
                        let mut download_needed = treq.tile_fetch_mode != TileFetchMode::Cache;
                        if treq.tile_exists_on_disk() {
                            if treq.tile_fetch_mode != TileFetchMode::Remote {
                                debug!("Tile {} exists on disk", treq.to_key());
                                
                                // Load tile from file
                                match TileRequestResult::new_from_file(&treq) {
                                    Ok(res) => {
                                        // Notify TileCache about the loaded tile
                                        glib::idle_add(receive_treq_result);
                                        match tx.send(res) {
                                            Ok(()) => { }, 
                                            Err(e) => {
                                                panic!("Send to TileCache failed: {}", e);
                                            }
                                        }
                                        download_needed = false;
                                    },
                                    Err(e) => {
                                        warn!("Failed to read tile from disk: {}", e);
                                    }
                                }
                            } else {
                                debug!("Tile {} exists on disk but remote is forced", treq.to_key());
                            }
                        } else {
                            debug!("Tile {} doesn't exists on disk", treq.to_key());
                        }
                        
                        // Download the requested tile
                        if download_needed {
                            let res = treq.source.fetch_tile_data(&treq, &http_client_t, &https_client_t);
                        
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
                            if res_cloned.code == TileRequestResultCode::Ok {
                                match res_cloned.save_to_disk() {
                                    Ok(()) => { 
                                        debug!("Tile {} saved to disk cache", treq.to_key());
                                    },
                                    Err(e) => {
                                        warn!("Failed to save the tile to disk: {}", e);
                                    }
                                }
                            }
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

    /// Push a new request to the queue to be processed by the tile worker threads..
    fn push_request(&mut self, treq: &TileRequest) {
        let mut mu = self.new_reqs_mutex.lock().unwrap();
        self.queue.insert(treq.clone());
        *mu = self.queue.len() as u32; // +1 can't work here because treqs can be equal
        assert!(*mu > 0);
        self.new_reqs_condvar.notify_one();
    }

    /// Returns the most urgent tile to be loaded and sets it to TileState::Prosessed before that.
    /// Blocks if there are not tiles to process.
    fn pull_request(&mut self) -> Option<TileRequest> {
        // Decrease available request count by one
        let mut mu = self.new_reqs_mutex.lock().unwrap();
        if *mu > 0 {
            assert_eq!(*mu, self.queue.len() as u32);
            *mu -= 1;

            // Return the request with highest score
            let treq = { 
                self.queue.iter().last().unwrap().clone()
            };
            self.queue.remove(&treq);
            assert_eq!(*mu, self.queue.len() as u32);
            Some(treq)
        } else {
            debug!("Request queue is empty");
            None
        }
    }
    
    /// Clears any tile request which is not about the given level.
    pub fn focus_on_zoom_level(&mut self, zoom_level: u8, abort_keys: &mut Vec<String>) {
        // Create a new queue and copy the wanted elements from the old one
        let mut mu = self.new_reqs_mutex.lock().unwrap();
        let mut new_queue: BTreeSet<TileRequest> = BTreeSet::new();
        for treq in &self.queue {
            if treq.z == zoom_level {
                new_queue.insert(treq.clone());
            } else {
                abort_keys.push(treq.to_key());
            }
        }
        self.queue = new_queue;
        *mu = self.queue.len() as u32;
    }

}

impl fmt::Debug for TileRequestQueue {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "queue.len={}", self.queue.len())
    }
}

// ---- TileSource ---------------------------------------------------------------------------------

/// The network source where tiles are loaded.
#[derive(Serialize, Deserialize, Clone)]
pub struct TileSource {
    // File system friendly name
    pub slug: String,

    /// An array of mutually optional url templates
    pub url_templates: Vec<String>,
    
    /// Token required by the service provider
    pub token: String,
    
    /// User agent header field to be used in HTTP requests. None results a default.
    pub user_agent: Option<String>,

    /// Referer header field to be used in HTTP requests. None results a default.
    pub referer: Option<String>,

    /// Override the server expire value with the given one. The value is in days.
    pub expire_override: Option<u16>,

    /// Tile width which has to be known
    pub tile_width: i32,
    
    /// Tile height which has to be known
    pub tile_height: i32,

}

impl TileSource {
    pub fn new(slug: String, url_templates: Vec<String>, token: String, tile_width: i32, tile_height: i32) -> TileSource {
        TileSource {
            slug: slug,
            url_templates: url_templates,
            token: token,
            user_agent: None,
            referer: None,
            expire_override: None,
            tile_width: tile_width,
            tile_height: tile_height,
        }
    }

    /// Add a new url template.
    ///
    /// the following strings will be substituted:
    /// ${x} - x coordinate
    /// ${y} - y coordinate
    /// ${z} - zoom level
    /// ${token} - token required by the service provider
    pub fn add_url_template(&mut self, url_template: String) {
        self.url_templates.push(url_template);
    }
    
    /// Download tile data from the source. 
    fn fetch_tile_data(&self, treq: &TileRequest, http_client: &Arc<Client>, https_client: &Arc<Client>) -> TileRequestResult {
        if self.url_templates.len() > 0 {
            let url = self.make_url(&treq).unwrap();
            let mut data: Vec<u8> = Vec::new();
            
            let mut expires = None; // false warning
            if url.scheme() == "file" {
                // Load data from local disk 
                return TileRequestResult::with_code(treq, TileRequestResultCode::UnknownError); // TODO
            } else {
                // Add request headers
                let mut headers = header::Headers::new();
                
                // User-Agent
                if let Some(user_agent) = treq.source.user_agent.clone() {
                    headers.set(header::UserAgent(user_agent));
                } else {
                    headers.set(header::UserAgent(settings_read().user_agent_header()));
                }
                
                // Referer
                if let Some(referer) = treq.source.referer.clone() {
                    headers.set(header::Referer(referer));
                }
            
                // Request tile data from a remote server with GET
                let client = {
                    if url.scheme() == "https" {
                        https_client
                    } else {
                        http_client
                    }
                };
                match client.get(url.as_str()).headers(headers).send() {
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
                                        debug!("expires_header: {}", expires_header);
                                    } else {
                                        expires = Some(UTC::now() + Duration::days(DEFAULT_TILE_EXPIRE_DAYS));
                                        debug!("Expires header missing, using a default");
                                    }

                                    // Consider expire override
                                    if let Some(expire_override) = treq.source.expire_override {
                                        expires = Some(UTC::now() + Duration::days(expire_override as i64));
                                    }
                                    
                                },
                                Err(e) => {
                                    warn!("Failed to read tile from a remote server; {}", e);
                                    return TileRequestResult::with_code(treq, TileRequestResultCode::TransmissionError);
                                }
                            }
                        } else if response.status == StatusCode::NotFound {
                            debug!("Tile not found on server");
                            return TileRequestResult::with_code(treq, TileRequestResultCode::NotFoundError);
                        } else if response.status == StatusCode::Unauthorized {
                            debug!("Unauthorized: {}", url);
                            return TileRequestResult::with_code(treq, TileRequestResultCode::UnauthorizedError);
                        } else if response.status == StatusCode::InternalServerError {
                            debug!("Internal server error when fetching tile");
                            return TileRequestResult::with_code(treq, TileRequestResultCode::UnknownError);
                        } else {
                            warn!("HTTP GET returned status code {}", response.status);
                            return TileRequestResult::with_code(treq, TileRequestResultCode::UnknownError);
                        }
                    },
                    Err(e) => {
                        warn!("Failed to get tile from a remote server; {}", e);
                        return TileRequestResult::with_code(treq, 
                            TileRequestResultCode::TransmissionError);
                    },
                }
            }
            TileRequestResult::new(&treq, &mut data, expires)
        } else {
            TileRequestResult::with_code(treq, TileRequestResultCode::NoSourceError)
        }
    }
    
    /// Make a url substituting url template variables with values from the TileRequest.
    pub fn make_url(&self, treq: &TileRequest) -> Result<Url, String> {
        if self.url_templates.len() > 0 {
            let index = rand::thread_rng().gen::<usize>() % self.url_templates.len();
            let ut = self.url_templates.get(index).unwrap();
            let url_string_with_vars = 
                    ut.replace("${x}", &(format!("{}", treq.wrap_x()).as_str()))
                      .replace("${y}", &(format!("{}", treq.y).as_str()))
                      .replace("${z}", &(format!("{}", treq.z).as_str()))
                      .replace("${token}", self.token.as_str());
            match Url::parse(url_string_with_vars.as_str()) {
                Ok(url) => {
                    debug!("make_url: url={}", url.to_string());
                    Ok(url)
                },
                Err(e) => {
                    Err(format!("Tile url creation error: {}", e.to_string()))
                }
            }
        } else {
            Err(format!("No tile urls defined for the tile source {}", self.slug))
        }
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
    //use std::sync::{Arc};
    //use super::hyper::{Client};
    //use std::thread::{sleep};
    //use std::collections::{HashMap};
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
                assert!(url.as_str() == "http://a.tile.openstreetmap.org/1/0/0.png" || 
                        url.as_str() == "http://b.tile.openstreetmap.org/1/0/0.png");
                assert!(url.scheme() == "http");
                assert!(url.host_str().unwrap() == "a.tile.openstreetmap.org" || 
                        url.host_str().unwrap() == "b.tile.openstreetmap.org");
            },
            Err(e) => {
                panic!(e.to_string());
            }
        }

/*        
        // Test GET
        let http_client_a = Arc::new(Client::new());
        let trr = tile_source.fetch_tile_data(&treq, &http_client_a);        
        assert!(trr.to_key() == treq.to_key());
        assert!(trr.data.len() > 4000);
        assert!(trr.code == TileRequestResultCode::Ok);
*/        
    }
    
    #[test]
    fn test_tile_request() {
        let tile_source = TileSource::new("osm-carto".into(), Vec::new(), "".into(), 256,  256, );
        assert_eq!(2, TileRequest::new(1, 1, -2, 0, 2, 1, tile_source.clone()).wrap_x());
        assert_eq!(3, TileRequest::new(1, 1, -1, 0, 2, 1, tile_source.clone()).wrap_x());
        assert_eq!(0, TileRequest::new(1, 1, 0, 0, 2, 1, tile_source.clone()).wrap_x());
        assert_eq!(0, TileRequest::new(1, 1, 4, 0, 2, 1, tile_source.clone()).wrap_x());
        assert_eq!(1, TileRequest::new(1, 1, 5, 0, 2, 1, tile_source.clone()).wrap_x());
        
    }
}

