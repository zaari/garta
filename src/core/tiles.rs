// Garta - GPX editor and analyser
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
// Currently [2016] gtk-rs binding (0.1.1) supports only loading Pixbufs from files 
// (instead of memory). So, the only option is to save tile images to file and re-load them in 
// mainthread. We use an external crate for image loading and saving (no Pixbuf saving in gtk-rs).
// TIFF file format is used because of its fast loading time. It would be ideal to be able to move 
// Pixbufs between Rust threads but that seem not to be possible at moment.
//
// Currently this file is more like a proof of concept than a serious implementation. 
// At least one complete rewrite is expected in the future.

extern crate cairo;
extern crate gdk;
extern crate gdk_pixbuf;
extern crate time;

use std::sync::{Arc, RwLock, Mutex, Condvar};
use std::collections::{VecDeque};
use std::thread;
use std::cmp::Ordering;
use self::gdk_pixbuf::{Pixbuf};
use self::cairo::{Context, Format, ImageSurface};
//use gdk::prelude::ContextExt;

use core::settings::{settings_read};

// ---- Tile ---------------------------------------------------------------------------------------

/// Tile content mode.
pub enum TileMode {
    /// Dummy content, typically single color
    Blank,
    
    /// Generated based on upper or lower level tile(s), or by reusing an expired tile
    Approximated,
    
    /// Tile is loaded and up-to-date
    Complete,
}

/// Tile state.
pub enum TileState {
    /// Just created and waiting for a thread to process it.
    Pending,
    
    /// Tile which a thread has started to process but not loading or ready yet.
    Processed,
    
    /// Content loading from tile source.
    Loading,
    
    /// Content loading from tile source.
    Ready,
    
    /// Content loading resulted an error.
    Error,
}

/// Map tile which can be drawn always.
pub struct Tile {
    // State of the tile
    pub state: TileState,

    // Tile content mode
    pub mode: TileMode,

    // Time when tile expires    
    expires: time::Tm,
    
    // x coordinate
    x: i32,
    
    // y coordinate
    y: i32,
    
    // zoom level
    z: i8,
    
    // High-dpi multiplier, usually 1
    mult: i8,
    
    pixbuf: Pixbuf,
}

// Cairo::Context
// - set_source_surface(surface, x, y)
// - set_source_pixbuf(pixbuf, x, y) -- Gdk::prelude::ContextExt
// Cairo::ImageSurface
// - 
// Gdk::Pixbuf
// - (new_from_surface(surface, x, y))
// Gtk::Image
// - new_from_file(filename)

impl Tile {
    // Constructor.
    pub fn new_from_pixbuf(pixbuf: Pixbuf, x: i32, y: i32, z: i8, mult: i8, expires: time::Tm, mode: TileMode) -> Tile {
        Tile{ state: TileState::Ready, mode: mode, 
              expires: expires, 
              x: x, y: y, z: z, mult: mult, 
              pixbuf: pixbuf}
    }

   
    pub fn x(&self) -> i32 { self.x }
    pub fn y(&self) -> i32 { self.y }
    pub fn z(&self) -> i8 { self.z }
    pub fn mult(&self) -> i8 { self.mult }
}

// ------------------------------------------------------------------------------------------------

/// Cloneable TileRequest.
#[derive(Clone)]
pub struct TileRequest {
    /// Tile request generation (a group of tile requests).
    generation: i64,

    /// Priority in generation.
    priority: i16,

    // Tile source
    source: Arc<Mutex<TileSource>>,
    
    /// x coordinate
    x: i32,
    
    /// y coordinate
    y: i32,
    
    /// zoom level
    z: i8,

    /// High-dpi multiplier, usually 1
    mult: i8,
    
    // Time when tile data expires
    expires: time::Tm,

    // TIFF file saved    
    filename: Arc<String>,
}

impl TileRequest {
    pub fn new(generation: i64, priority: i16, source: Arc<Mutex<TileSource>>, x: i32, y: i32, z: i8, mult: i8, filename: String) -> TileRequest {
        TileRequest {
            generation: generation, priority: priority, source: source,
            x: x, y: y, z: z, mult: mult, expires: time::now(), filename: Arc::new(filename), 
        }
    }
    
    pub fn fetch(&self) {
    }
    
    /// Convert the tile request to a tile (if ready).
    pub fn to_tile(&self) -> Result<Tile, String> {
        match Pixbuf::new_from_file(self.filename.as_str()) {
            Ok(pb) => {
                Ok(Tile::new_from_pixbuf(pb, self.x, self.y, self.z, self.mult, self.expires, TileMode::Complete))
            },
            Err(e) => {
                Err(format!("Tile data not loaded yet: {}", e))
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

// ------------------------------------------------------------------------------------------------

/// Representing a queue of tiles to be completed.
pub struct TileRequestQueue {
    queue: VecDeque<TileRequest>, // OrderedSet would be ideal (maybe in the future)
    new_tiles_mutex: Arc<Mutex<i32>>,
    new_tiles_condvar: Arc<Condvar>,
}

impl TileRequestQueue {
    pub fn new() -> Arc<RwLock<TileRequestQueue>> {
        // Create a new tile grid
        let trq = Arc::new(RwLock::new(TileRequestQueue{ 
                    queue: VecDeque::new(),
                    new_tiles_mutex: Arc::new(Mutex::new(0)),
                    new_tiles_condvar: Arc::new(Condvar::new()),
                 }));

        // Start worker threads        
        let n = settings_read().worker_threads();
        for i in 0..n {
            debug!("thread {}", i);
            let trqt   = trq.clone();
            let nt_m  = trqt.write().unwrap().new_tiles_mutex.clone();
            let nt_cv = trqt.write().unwrap().new_tiles_condvar.clone();
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
                    {
                        let treq = trqt.write().unwrap().pull_request();
                        treq.fetch();
                    }
                }
            });
        }
        
        trq
    }

    /// Return a blank pixbuf using a default color.
    fn make_blank_pixbuf(&self, tw: i32, th: i32) -> Pixbuf {
        let surface = ImageSurface::create(Format::ARgb32, tw, th);
        let c = Context::new(&surface);
        c.set_source_rgb(0.16, 0.16, 0.16);
        c.rectangle(0.0, 0.0, (tw - 1) as f64, (th - 1) as f64);
        c.fill();
        
        // TODO: https://github.com/gtk-rs/gdk-pixbuf/issues/22
        //Pixbuf::get_from_surface(surface, 0, 0, tw, th);
        Pixbuf::new_from_file("/tmp/testi.jpg").unwrap()
        
        // TODO
//			// Dimensions of pixbuf
//			var tw = treq.map.tile_width;
//			var th = treq.map.tile_height;

//			if (cached_solid_pixbuf[num] == null || ((!)cached_solid_pixbuf[num]).width != tw || ((!)cached_solid_pixbuf[num]).height != th)  {
//				// Draw into a surface
//				var surface = new Cairo.ImageSurface (Cairo.Format.ARGB32, tw, th);
//				var c = new Cairo.Context (surface);
//				if (num == 0) {
//					c.set_source_rgb (0.16, 0.16, 0.16);
//				} else {
//					c.set_source_rgb (0.0, 0.0, 0.0);
//				}
//				c.rectangle (0, 0, tw, th);
//			    c.fill();

//				// Convert to a pixbuf and return
//				cached_solid_pixbuf[num] = Gdk.pixbuf_get_from_surface (surface, 0, 0, tw, th);
//			}
//			return new Tile (cached_solid_pixbuf[num], treq.tile_pos, treq.map, tile_mode);
    }

    pub fn push_request(&mut self, treq: TileRequest) {
        // As there is no ordered set available, we have to do the hard work 
        // by ourselves... Time complexity is O(n).
        for i in 0..self.queue.len() {
            if self.queue[i].cmp(&treq) == Ordering::Less {
                self.queue.insert(i, treq);
                self.incr_and_notify();
                return
            }
        }
        self.queue.push_back(treq);
        self.incr_and_notify();
    }

    // Increment request count by one and notify worker threads    
    fn incr_and_notify(&self) {
        let mut mu = self.new_tiles_mutex.lock().unwrap();
        *mu += 1;
        self.new_tiles_condvar.notify_one();
    }
    
    /// Returns the most urgent tile to be loaded and sets it to TileState::Prosessed before that.
    /// Blocks if there are not tiles to process.
    fn pull_request(&mut self) -> TileRequest {
        // Decrease available request count by one
        let mut mu = self.new_tiles_mutex.lock().unwrap();
        *mu -= 1;
        
        // Return the topmost request
        self.queue.pop_front().unwrap()
    }
    
    /// Just a dummy method. Can be deleted later as the project continues.
    pub fn ping(&self) {
    }
}

impl Drop for TileRequestQueue {
    fn drop(&mut self) {
    }
}

// ------------------------------------------------------------------------------------------------

/// Mem & disk cache for the tiles.
/// 
/// Tile procedure:
/// 1. check if a valid tile is available in cache -> Complete, Ready
/// 2. check if an expired tile is available in cache -> Approximated, Loading
/// 3. look for an upper level tile -> Approximated, Loading
/// 4. no tile -> Blank, Loading
struct TileCache {
}

// ------------------------------------------------------------------------------------------------

/// Disk cache for pictures, whether they are map tiles or photos.
struct PictureDiskCache {
}

// ---- TileSource ---------------------------------------------------------------------------------

/// The network source where tiles are loaded.
pub struct TileSource {
    // The current string in urls vector
    index: usize,

    /// An array of mutually optional urls
    ///
    urls: Vec<String>,
}

impl TileSource {
    pub fn new() -> TileSource {
        TileSource {
            index: 0,
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
    let trq = TileRequestQueue::new();
    trq.read().unwrap().ping();
}

