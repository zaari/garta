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
use self::hyper::{Client};
use self::hyper::status::{StatusCode};
use self::rand::{Rng};
use self::cairo::{/*Context, */Format, ImageSurface};

use core::settings::{settings_read};

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
    tcache_rr.borrow_mut().init();
    
    // Return tcache
    tcache_rr
}

impl TileCache {
    fn new() -> TileCache {
        let tcache = TileCache {
            tiles: BTreeMap::new(),
            tile_request_queue: TileRequestQueue::new(),
            observer: None,
        };
        tcache
    }

    /// Initialize the cache
    pub fn init(&self) {
        // TODO
    }

    /// Return tile for the given request. The result may be an approximation.    
    pub fn get_tile(&mut self, treq: &TileRequest) -> Option<&mut Tile> {
        let tile_key = treq.to_key();
        if self.tiles.contains_key(&tile_key) {
            // Check tile state
            match self.tiles.get(&tile_key).unwrap().state {
                TileState::Void => {
                    self.tiles.insert(tile_key.clone(), Tile::with_request(treq));
                    self.tile_request_queue.write().unwrap().push_request(treq);
                }
                TileState::Pending => {
                }
                TileState::Fetching => {
                }
                TileState::Ready => {
                    // TODO: check expire time
                }
                TileState::Error => {
                }
                TileState::Flushed => {
                    self.tile_request_queue.write().unwrap().push_request(treq);
                }
            }

            // Update access time
            self.tiles.get_mut(&tile_key).unwrap().access_time = chrono::UTC::now();
            
            // Return
            Some(self.tiles.get_mut(&tile_key).unwrap())
        } else {
            // Enqueue the request and create a new empty tile
            self.tile_request_queue.write().unwrap().push_request(treq);
            let mut tile = Tile::with_request(treq);
            
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
        if let Some(mut tile) = self.tiles.get_mut(&treq_result.to_key()) {
            // Assign tile data
            tile.data = Some(treq_result.data.clone());
            tile.state = TileState::Ready;
            tile.width = treq_result.tile_width;
            tile.height = treq_result.tile_height;
        } else {
            warn!("Received image data fetch for tile {} but tile isn't in cache!", treq_result.to_key());
        }
    }
}

// ---- Tile ---------------------------------------------------------------------------------------

/// Tile state.
#[derive(Clone, Debug)]
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
    pub access_time: chrono::DateTime<chrono::UTC>,
    
    /// Time when this tile expires.
    pub expire_time: chrono::DateTime<chrono::UTC>,
    
    /// Tile data as a byte array.
    data: Option<Box<[u8]>>,
    
    /// Tile data converted to a surface
    surface: Option<ImageSurface>,
    surface_none: Option<ImageSurface>,
    
    /// True if the tile image exists in disk cache
    saved: bool,
}

impl Tile {
    /// Constructor from TileRequest.
    pub fn with_request(treq: &TileRequest) -> Tile {
        Tile{ state: TileState::Pending, 
              x: treq.x, y: treq.y, z: treq.z, mult: treq.mult, 
              width: 256, height: 256,
              access_time: chrono::UTC::now(),
              expire_time: chrono::UTC::now(), // TODO: future
              data: None,
              surface: None,
              surface_none: None,
              saved: false,
        }
    }

    /// Constructor a black tile for TileRequest.
    fn new(treq: &TileRequest, r: f64, g: f64, b: f64) -> Tile {
        // Create black isurface
        let isurface = ImageSurface::create(Format::ARgb32, 256, 256);
        let c = cairo::Context::new(&isurface);
        c.set_source_rgb(r, g, b);
        c.paint();

        // Return tile        
        Tile{ state: TileState::Pending, 
              x: treq.x, y: treq.y, z: treq.z, mult: treq.mult, 
              width: 256, height: 256,
              access_time: chrono::UTC::now(),
              expire_time: chrono::UTC::now(), // TODO: future
              data: None,
              surface: None,
              surface_none: None,
              saved: false,
        }
    }

    // Getters   
    pub fn x(&self) -> u32 { self.x }
    pub fn y(&self) -> u32 { self.y }
    pub fn z(&self) -> u8 { self.z }
    pub fn mult(&self) -> u8 { self.mult }
    pub fn width(&self) -> i32 { self.width }
    pub fn height(&self) -> i32 { self.height }
    
    /// Return image surface. May involve an in-memory data conversion.
    pub fn get_surface(&mut self) -> Option<&ImageSurface> {
        if self.surface.is_none() {
            if let Some(data) = self.data.take() {
                let stride = cairo_format_stride_for_width(Format::ARgb32, self.width);
                debug!("stride={} (width={})", stride, self.width);
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
            x: treq.x, y: treq.y, z: treq.z, mult: treq.mult, 
            width: self.width, height: self.height,
            access_time: chrono::UTC::now(),
            expire_time: chrono::UTC::now(), // TODO: future
            data: None,
            surface: Some(isurface),
            surface_none: None,
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

impl fmt::Debug for Tile {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        let data_state = {
            if self.surface.is_some() { "surface" }
            else if self.data.is_some() { "data" }
            else if self.saved { "disk" }
            else { "empty" }
        };
        write!(f, "{{{},{} L{} {}x{} {} [{:?}]}}", 
            self.x, self.y, self.z, self.width, self.height, data_state, self.state)
    }
}

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

#[derive(Clone)]
struct TileRequestResult {
    pub request: TileRequest,

    /// Image raw bitmap data
    pub data: Box<[u8]>,
    
    /// Tile width in pixels.
    pub tile_width: i32,
    
    /// Tile height in pixels.
    pub tile_height: i32,
    
    /// Error message
    pub error: String,
}

impl TileRequestResult {
    /// Non-error constructor.
    fn new(treq: &TileRequest, img_data: &mut Vec<u8>) -> TileRequestResult {
        let mut tile_width: i32 = 0;
        let mut tile_height: i32 = 0;
        match convert_image_to_buffer(img_data, &mut tile_width, &mut tile_height) {
            Ok(raw_data) => {
                TileRequestResult {
                    request: treq.clone(),
                    data: raw_data,
                    tile_width: tile_width,
                    tile_height: tile_height,
                    error: "".into(),
                }
            },
            Err(e) => {
                TileRequestResult {
                    request: treq.clone(),
                    data: Box::new([0u8]),
                    tile_width: 0,
                    tile_height: 0,
                    error: e.to_string(),
                }
            }
        }
    }

    /// Error constructor.
    fn with_error(treq: &TileRequest, err: String) -> TileRequestResult {
        TileRequestResult {
            request: treq.clone(),
            data: Box::new([0u8]),
            tile_width: 0,
            tile_height: 0,
            error: err,
        }
    }
    
    /// Return TileRequest key.
    pub fn to_key(&self) -> String {
        self.request.to_key()
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
                        debug!("treq_result received successfully");
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
    fn new() -> Arc<RwLock<TileRequestQueue>> {
        // Create a new tile grid
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
        let http_client_a = Arc::new(Client::new());
        for i in 1..(n + 1) {

            // Put self in thread local storage
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
                //*global.borrow_mut() = Some((tcache_t, rx)) // https://github.com/gtk-rs/examples/blob/master/src/multithreading_context.rs
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
                            
                            // Download the requested tile
                            let res = treq.source.fetch_tile_data(&treq, &http_client_t);
                            
                            // Notify TileCache
                            glib::idle_add(receive_treq_result);
                            match tx.send(res) {
                                Ok(()) => {
                                }, 
                                Err(e) => {
                                    panic!("Send to TileCache failed: {}", e);
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
#[derive(Clone)]
pub struct TileSource {
    // File system friendly name
    pub slug: String,

    // A unique name of the tile source.
    pub name: String,

    /// An array of mutually optional urls
    pub urls: Vec<String>,
    
    /// Token required by the service provider
    pub token: String,
}

impl TileSource {
    pub fn new() -> TileSource {
        TileSource {
            slug: "unnamed".into(),
            name: "Unnamed".into(),
            urls: Vec::new(),
            token: "".into(),
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
            
            if url.starts_with("file:") {
                // Load data from local disk
                // TODO
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
            TileRequestResult::new(treq, &mut data)
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
            Err(format!("No urls defined for the tile source {}", self.name))
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
            for i in 0..bu8.len() { // TODO: in the future .step_by(4)
                if i % 4 == 0 {
                    bu8.swap(i + 0, i + 2); // RGBA -> BGRA (Cairo expects this; ARGB32 in big-endian)
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
        let mut tile_source = TileSource::new();
        tile_source.slug = "osm-carto".into();
        tile_source.name = "OpenStreetMap Carto".into();
        tile_source.urls.push("http://a.tile.openstreetmap.org/${z}/${x}/${y}.png".into());
        tile_source.urls.push("http://b.tile.openstreetmap.org/${z}/${x}/${y}.png".into());
        
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

