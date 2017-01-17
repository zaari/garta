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
rendering. So far gtk-rs crate doesn't support asynchronous I/O of glib.

This module both manages tile cache and takes care of downloading new tiles from network 
using worker threads. It also converts the downloaded image files into image buffers that 
are given to the GTK main thread where those buffers are cached into Cairo ImageSurfaces 
that are used to render the map.

In the future it may be worth evaluating the option to use futures-rs based crates, to make 
this module more efficient and elegant. It may be better to wait for the crates to reach a 
stable version first, though.


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
use std::collections::{BTreeMap, BTreeSet};
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

use core::persistence::{serialize_to, deserialize_from, serialize_datetime, deserialize_datetime};
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
    pub observer: Option<Rc<TileObserver>>,
    
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
        //debug!("get_tile: {:?}, contains: {:?}", treq, self.tiles.get(&tile_key).unwrap());
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
                    }
                    TileState::Error => {
                    }
                    TileState::NonExistent => {
                    }
                    TileState::Unauthorized => {
                    }
                    TileState::Flushed => {
                        debug!("Reloading a flushed tile: {}", tile_key);
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

                // Update access time
                tile.access_time = UTC::now();
                
                // Return
                Some(tile)
            }
        } else {
            // If the coordinates are out of bounds, return an empty tile
            if treq.y < 0 || treq.y >= (1 << treq.z) {
                return None;
            }
        
            // Enqueue the request and create a new empty tile
            debug!("Requesting a new tile: {}", tile_key);
            self.tile_request_queue.write().unwrap().push_request(treq);
            let mut tile = Tile::new_with_request(treq);
            
            // Approximate content
            if treq.z > 0 {
                let mut treq_up = treq.zoom_out();
                while treq_up.z >= 1 {
                    let tile_key_up = treq_up.to_key();
                    if self.tiles.contains_key(&tile_key_up) {
                        tile = self.tiles.get(&tile_key_up).unwrap().zoom_in(&treq);
                        break;
                    }
                    treq_up = treq_up.zoom_out();
                }
                
                // Create a black tile on top
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
    
    /// Handle image fetch result from a worker thread. Returns true if the observer should be 
    /// notified.
    fn handle_result(&mut self, treq_result: &TileRequestResult) -> bool {
        // Assign tile information
        if let Some(ref mut tile) = self.tiles.get_mut(&treq_result.to_key()) {
            match treq_result.code {
                TileRequestResultCode::Ok => {
                    let old_mem_usage = tile.estimate_mem_usage();
                
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
                    } as u64;
                    self.mem_usage = self.mem_usage + tile.estimate_mem_usage() - old_mem_usage;
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

/*
        // Mem-flush a tile which would expire the soonest
        // FIXME: analyze access times
        if let Some(mem_capacity) = settings_read().tile_mem_cache_capacity {
            if self.mem_usage > mem_capacity {
                let ref mut tiles = self.tiles;
                for (ref tile_id, ref mut tile) in tiles.iter_mut() {
                    if self.mem_usage <= mem_capacity {
                        break;
                    }

                    // Flush only lower tiles
                    if tile.z > 3 {
                        let tmu0 = tile.estimate_mem_usage();
                        tile.flush();
                        self.mem_usage = self.mem_usage + tile.estimate_mem_usage() - tmu0;
                    }
                }
            }
        }
*/        
        
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
            return true;
        }
        return false;
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
                // TODO: remove the state file
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

// ---- TileCacheState -----------------------------------------------------------------------------

/// Needed when storing and restoring TileCache state at application startup and shutdown.
/// See TileCache::restore and TileCache::store for more info.
#[derive(Serialize, Deserialize, Debug)]
struct TileCacheState {
    /// The tiles
    tiles: BTreeMap<String, Tile>,
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
        tcache.mem_usage = 0;
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
            tcache.mem_usage += tile.estimate_mem_usage();
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

    /// Just created and waiting for a thread to process it.
    /// Contents of the tile is either black, approximated from a different zoom level
    /// or contains expired data.
    Pending,
    
    /// Content been loaded from tile source.
    Fetching,
    
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
    x: i64,
    
    /// y coordinate (range is 0..2^z)
    y: i64,
    
    /// zoom level
    z: u8,
    
    /// High-dpi multiplier, usually 1
    mult: u8,
    
    /// Tile width as pixels
    width: i32,

    /// Tile height as pixels
    height: i32,
    
    /// Time when this tile was needed.
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
    disk_usage: u64,
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
    fn new(treq: &TileRequest, r: f64, g: f64, b: f64) -> Tile {
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
    pub fn x(&self) -> i64 { self.x }
    pub fn y(&self) -> i64 { self.y }
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

    /// Approximate a new tile by zooming this one in.
    fn zoom_in(&self, treq: &TileRequest) -> Tile {
        // Math
        let q2 = 1 << (treq.z - self.z) as i32;
        let offset_x = (-self.width * (treq.x as i32 % q2) / q2) as f64;
        let offset_y = (-self.height * (treq.y as i32 % q2) / q2) as f64;

        // Create a new
        let isurface = ImageSurface::create(Format::ARgb32, self.width, self.height);
        let c = cairo::Context::new(&isurface);
        c.scale(q2 as f64, q2 as f64);
        if let Some(ref self_surface) = self.surface {
            // Paint from source surface
            c.set_source_surface(self_surface, offset_x, offset_y);
            c.paint();
            debug!("zoom_in: q2={} offset={},{}", q2, offset_x, offset_y);
        } else {
            // Red tile in case of missing surface
            c.set_source_rgb(0.8, 0.0, 0.0);
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
    fn estimate_mem_usage(&self) -> usize {
        let mut u: usize = mem::size_of::<Tile>();
        if let Some(ref data) = self.data {
            u += data.len() as usize; // bytes
        }
        if self.surface.is_some() && !self.surface_is_temporary {
            u += (self.width * self.height * 4) as usize; // RGBA assumed
        }
        u
    }

    /// Remove cached tile data from memory    
    fn flush(&mut self) {
        self.data = None;
        self.surface = None;
        self.surface_is_temporary = false;
        self.state = TileState::Flushed;
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
    pub x: i64,
    
    /// Y-position
    pub y: i64,
    
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
    
    /// Retry count. This is decreased every time when retried.
    retry_count: Option<u8>,
}

impl TileRequest {
    /// Constructor for a tile request.
    pub fn new(generation: u64, priority: i64, x: i64, y: i64, z: u8, mult: u8, source: TileSource) -> TileRequest {
        TileRequest {
            generation: generation, priority: priority, 
            x: x, y: y, z: z, mult: mult,
            source: source,
            tile_fetch_mode: TileFetchMode::Any,
            tile_state_on_success: TileState::Ready,
            retry_count: None,
        }
    }
    
    /// If x is out of bounds wrap it.
    pub fn wrap_x(&self) -> i64 {
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
    fn to_key(&self) -> String {
        format!("{}/{}/{}/{}@{}", self.source.slug, self.z, self.y, self.wrap_x(), self.mult)
    }

    /// Returns a copy of this with zoom level decreased and the (x,y) adjusted according to that.
    fn zoom_out(&self) -> TileRequest {
        TileRequest {
            generation: self.generation, priority: self.priority,
            x: self.wrap_x() / 2, y: self.y / 2, z: self.z - 1, mult: self.mult,
            source: self.source.clone(), tile_fetch_mode: self.tile_fetch_mode,
            tile_state_on_success: TileState::Ready,
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
                            if let Some(observer) = tcache.borrow().observer.clone() {
                                observer.tile_loaded(&treq_result.request);
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
        // Start worker threads        
        let n = settings_read().worker_threads();
        let mut http_client = Client::new();
        http_client.set_read_timeout(
            Some(time::Duration::from_secs(settings_read().tile_read_timeout)));
        http_client.set_write_timeout(
            Some(time::Duration::from_secs(settings_read().tile_write_timeout)));
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

            // Start the worker threads        
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
                        debug!("treq={:?} trq={:?}", treq, trqueue_t.read());
                    
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

    fn push_request(&mut self, treq: &TileRequest) {
        // Add a new request to the queue and notify the worker threads.
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

    /// An array of mutually optional urls
    pub urls: Vec<String>,
    
    /// Token required by the service provider
    pub token: String,
    
    /// User agent header field to be used in HTTP requests. None results a default.
    pub user_agent: Option<String>,

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
            user_agent: None,
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

            let mut expires = None; // false warning (with Rust 1.15 nightly at least)
            if url.starts_with("file:") {
                // Load data from local disk 
                return TileRequestResult::with_code(treq, TileRequestResultCode::UnknownError); // TODO
            } else {
                // Add request headers
                let mut headers = header::Headers::new();
                if let Some(user_agent) = treq.source.user_agent.clone() {
                    headers.set(header::UserAgent(user_agent));
                } else {
                    headers.set(header::UserAgent(settings_read().user_agent_header()));
                }
            
                // Request tile data from a remote server with GET
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
                                },
                                Err(e) => {
                                    error!("Failed to read tile from a remote server; {}", e);
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
                        error!("Failed to get tile from a remote server; {}", e);
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
    
    /// Make a url substituting url variables with values from the TileRequest.
    pub fn make_url(&self, treq: &TileRequest) -> Result<String, String> {
        if self.urls.len() > 0 {
            let index = rand::thread_rng().gen::<usize>() % self.urls.len();
            let url = self.urls.get(index).unwrap();
            let url_with_vars = url.replace("${x}", &(format!("{}", treq.wrap_x()).as_str()))
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
        /* Doesn't work with url parameters
        if let Some(ref url) = self.urls.get(0) {
            let n = url.len();
            return Some(url[(n - 3) .. n].into()); // TODO: smarter way
        }
        */
        
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
    //use std::sync::{Arc};
    //use super::hyper::{Client};
    use super::*;

/*
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
        assert!(trr.code == TileRequestResultCode::Ok);
    }
*/    
    
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

