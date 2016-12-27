// Garta - GPX viewer and editor
// Copyright (C) 2016  Timo Saarinen
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
// Currently [Dec 2016] gtk-rs binding (0.1.1) doesn't support asynchronous Pixbuf loading
// or conversion from Pixbuf to ImageSure. The tile cache implementation relies on 
// synchronous ImageSurface::create_for_data call on gtk main thread that doesn't guarantee the 
// smoothest possible user experience. If gtk-rs becomes more complete in the future this module 
// will be rewritten.
// -------------------------------------------------------------------------------------------------

// Procedure:
//
//  canvas::draw -> TileCache.get_tile
//      a) return an up-to-date cached tile
//      b) return an expired tile and add a TileRequest to TileRequestQueue
//      c) return a blank tile and add a TileRequest to TileRequestQueue
//
//  worker-thread::run
//      1. pull the highest priority item from TileRequestQueue
//      2. download tile image data to RAM
//      3. save tile image data to Tile struct
//      4. notify canvas about the changes
//      5. save tile image data to disk
//  


extern crate cairo;
extern crate gdk;
extern crate gdk_pixbuf;
extern crate time;

use std::sync::{Arc, RwLock, Mutex, Condvar};
use std::collections::{BTreeMap, BTreeSet};
use std::thread;
use std::cmp::Ordering;
use self::cairo::{/*Context, */Format, ImageSurface};

use core::settings::{settings_read};

// ---- Tile ---------------------------------------------------------------------------------------

/// Tile state.
#[derive(Clone)]
pub enum TileState {
    // Without any real information or data.
    Void,

    /// Just created and waiting for a thread to process it.
    /// Contents of the tile is either black, approximated from a different zoom level
    /// or contains expired data.
    Pending,
    
    /// Content loading from tile source.
    Fetching,
    
    /// Content loading from tile source.
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

    /// x coordinate
    x: u32,
    
    /// y coordinate
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
    pub access_time: time::Tm,
    
    /// Time when this tile expires.
    pub expire_time: time::Tm,
    
    /// Tile data as a byte array.
    data: Option<Box<[u8]>>,
    
    /// Tile data converted to a surface
    surface: Option<ImageSurface>,
    
    /// True if the tile image exists in disk cache
    saved: bool,
}

impl Tile {
    /// Constructor from TileRequest.
    pub fn new_from_req(treq: &TileRequest) -> Tile {
        Tile{ state: TileState::Pending, 
              x: treq.x, y: treq.y, z: treq.z, mult: treq.mult, 
              width: treq.source.tile_width, height: treq.source.tile_height,
              access_time: time::now(),
              expire_time: time::now(), // TODO: epoch
              data: None,
              surface: None,
              saved: false,
        }
    }

    /// Constructor a black tile for TileRequest.
    pub fn new(treq: &TileRequest, r: f64, g: f64, b: f64) -> Tile {
        // Create black isurface
        let isurface = ImageSurface::create(Format::ARgb32, treq.source.tile_width, treq.source.tile_width);
        let c = cairo::Context::new(&isurface);
        c.set_source_rgb(r, g, b);
        c.paint();

        // Return tile        
        Tile{ state: TileState::Pending, 
              x: treq.x, y: treq.y, z: treq.z, mult: treq.mult, 
              width: treq.source.tile_width, height: treq.source.tile_height,
              access_time: time::now(),
              expire_time: time::now(), // TODO: epoch
              data: None,
              surface: None,
              saved: false,
        }
    }

    // Getters   
    pub fn x(&self) -> u32 { self.x }
    pub fn y(&self) -> u32 { self.y }
    pub fn z(&self) -> u8 { self.z }
    pub fn mult(&self) -> u8 { self.mult }
    
    /// Return image surface. May involve an in-memory data conversion.
    pub fn get_surface(&mut self) -> Option<ImageSurface> {
        if self.surface.is_none() {
            if let Some(ref data) = self.data {
                self.surface = Some(ImageSurface::create_for_data(data.clone(), |box_u8| { }, Format::ARgb32, self.width, self.height, 4));
            } else {
                return None;
            }
        }
        self.surface.clone()
    }

    /// Approximate a new tile by zooming this one in.
    pub fn zoom_in(&self, treq: &TileRequest) -> Tile {
        // Math
        let q2 = 1 << (self.z - treq.z) as i32;
        let offset_x = -treq.source.tile_width * (treq.x as i32 % q2);
        let offset_y = -treq.source.tile_height * (treq.y as i32 % q2);

        // Create a new
        let isurface = ImageSurface::create(Format::ARgb32, treq.source.tile_width, treq.source.tile_width);
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
            x: treq.x, y: treq.y, z: treq.z, mult: treq.mult, 
            width: treq.source.tile_width, height: treq.source.tile_height,
            access_time: time::now(),
            expire_time: time::now(), // TODO: epoch
            data: None,
            surface: Some(isurface),
            saved: false,
        }
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

// ---- TileRequest --------------------------------------------------------------------------------

/// Cloneable TileRequest.
#[derive(Clone)]
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

    /// Source
    source: TileSource,    
}

impl TileRequest {
    pub fn new(generation: u64, priority: u16, x: u32, y: u32, z: u8, mult: u8, source: TileSource) -> TileRequest {
        TileRequest {
            generation: generation, priority: priority,
            x: x, y: y, z: z, mult: mult,
            source: source,
        }
    }
    
    pub fn fetch(&self) {
    }

    /// Unique key of this tile
    pub fn to_key(&self) -> String {
        format!("{}/{}/{}/{}@{}", self.source.name, self.z, self.y, self.x, self.mult)
    }

    /// Returns a copy of this with zoom level decreased and the (x,y) adjusted according to that.
    pub fn zoom_out(&self) -> TileRequest {
        TileRequest {
            generation: self.generation, priority: self.priority,
            x: self.x / 2, y: self.y / 2, z: self.z - 1, mult: self.mult,
            source: self.source.clone(),
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

// ---- TileCache ----------------------------------------------------------------------------------
/// The main access point for tiles
pub struct TileCache {
    /// TileRequest::to_key -> Tile map
    tiles: BTreeMap<String, Tile>,

    /// The queue accessed by the worker threads    
    tile_request_queue: Arc<RwLock<TileRequestQueue>>,
}

impl TileCache {
    pub fn new() -> TileCache {
        TileCache {
            tiles: BTreeMap::new(),
            tile_request_queue: TileRequestQueue::new(),
        }
    }

    /// Initialize the cache
    pub fn init(&self) {
        // TODO
    }

    /// Return tile for the given request. The result may be an approximation.    
    pub fn get_tile(&mut self, treq: &TileRequest) -> Option<&Tile> {
        let tile_key = treq.to_key();
        if self.tiles.contains_key(&tile_key) {
            // Check tile state
            match self.tiles.get(&tile_key).unwrap().state {
                TileState::Void => {
                    self.tiles.insert(tile_key.clone(), Tile::new_from_req(treq));
                    self.tile_request_queue.write().unwrap().push_request(treq);
                }
                TileState::Pending => {
                }
                TileState::Fetching => {
                }
                TileState::Ready => {
                }
                TileState::Error => {
                }
                TileState::Flushed => {
                    self.tile_request_queue.write().unwrap().push_request(treq);
                }
            }
            
            // Return
            Some(self.tiles.get(&tile_key).unwrap())
        } else {
            // Enqueue the request and create a new empty tile
            self.tile_request_queue.write().unwrap().push_request(treq);
            let tile = Tile::new_from_req(treq);
            let mut tile = self.tiles.get(&tile_key).unwrap().clone();
            
            // Approximate content
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
            
            // Store tile and return
            self.tiles.insert(tile_key.clone(), tile);
            Some(self.tiles.get(&tile_key).unwrap())
        }
    }
}


// ---- TileRequestQueue ---------------------------------------------------------------------------

/// Representing a queue of tiles to be completed.
struct TileRequestQueue {
    queue: BTreeSet<TileRequest>, // OrderedSet would be ideal (maybe in the future)
    
    new_reqs_mutex: Arc<Mutex<u32>>,
    new_reqs_condvar: Arc<Condvar>,
}

impl TileRequestQueue {
    pub fn new() -> Arc<RwLock<TileRequestQueue>> {
        // Create a new tile grid
        let trqueue = Arc::new(RwLock::new(TileRequestQueue{ 
            queue: BTreeSet::new(),
            new_reqs_mutex: Arc::new(Mutex::new(0)),
            new_reqs_condvar: Arc::new(Condvar::new()),
         }));

        // Start worker threads        
        let n = settings_read().worker_threads();
        for i in 0..n {
            debug!("thread {}", i);
            let trqueue_t   = trqueue.clone();
            let nt_m  = trqueue_t.write().unwrap().new_reqs_mutex.clone();
            let nt_cv = trqueue_t.write().unwrap().new_reqs_condvar.clone();
            thread::spawn(move || {
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
                            let treq = trqueue.pull_request();
                            treq.fetch();
                        }
                        Err(e) => {
                            panic!("Failed to unlock tile request queue: {}", e);
                        }
                    }
                }
            });
        }
        
        trqueue
    }

    pub fn push_request(&mut self, treq: &TileRequest) {
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
#[derive(Clone)]
pub struct TileSource {
    // A unique name of the tile source.
    name: String,

    /// The current string in urls vector.
    index: usize,
    
    // Tile dimension
    tile_width: i32, tile_height: i32,

    /// An array of mutually optional urls
    urls: Vec<String>,
}

impl TileSource {
    pub fn new() -> TileSource {
        TileSource {
            name: "".into(),
            index: 0,
            tile_width: 256, tile_height: 256,
            urls: Vec::new(),
        }
    }

    /// Add a new url with vars.
    ///
    /// the following strings will be substituted:
    /// ${x} - x coordinate
    /// ${y} - y coordinate
    /// ${z} - zoom level
    pub fn add_url(&mut self, url: String) {
        self.urls.push(url);
    }
    
    /// Download tile from the source. 
    /// Parameters x, y and z represent the coordinates and zoom level.
    /// Parameter multiplier 
    pub fn fetch_tile(&mut self, treq: TileRequest) -> Result<TileRequest, String> {
        if self.urls.len() > 0 {
            let url = self.make_url(&treq).unwrap();
            
            if url.starts_with("file:") {
                // TODO: load
            } else {
                // TODO: download
            }
            
            Ok(treq)
        } else {
            Err("No urls defined for the tile source".to_string())
        }
    }
    
    fn make_url(&mut self, treq: &TileRequest) -> Result<String, String> {
        if self.urls.len() > 0 {
            let url = self.urls.get(self.index).unwrap();
            let url_with_vars = url.replace("${x}", &(format!("{}", treq.x).as_str()))
                                   .replace("${y}", &(format!("{}", treq.y).as_str()))
                                   .replace("${z}", &(format!("{}", treq.z).as_str()));
            self.index = (self.index + 1) % self.urls.len();
            Ok(url_with_vars) // TODO: vars
        } else {
            Err("No urls defined for the tile source".to_string())
        }
    }
}

// ---- TileMap ------------------------------------------------------------------------------------

pub trait TileMap {
    fn slug(&self) -> &String;
    fn tile_width(&self) -> i32;
    fn tile_height(&self) -> i32;
}

pub struct MercatorTileMap {
    slug: String,
    tile_width: i32,
    tile_height: i32,
}

// ---- tests --------------------------------------------------------------------------------------

#[test]
fn test_tile_request_queue() {
    let tile_cache = TileCache::new();
    tile_cache.init();
}

