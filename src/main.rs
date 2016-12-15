#[macro_use]
extern crate lazy_static;

mod core;
mod gui;
mod gpx;

use std::cell::{RefCell};
use std::rc::{Rc};
use std::result::*;
use std::sync::{Arc};

use core::settings::{settings_read, settings_write};
use core::tiles::TileRequestQueue;
use core::root::{Project, Layer, MapView};
use core::map::Map;

fn main() {
    println!("Garta started");
    
    // Load settings
    println!("Loading settings");
    settings_write().load();
    
    // Start the threads
    println!("Starting worker threads");
    let trq = TileRequestQueue::new();
    trq.read().unwrap().ping();

    // Project with test data
    let mut project = Project::new("unnamed".into());
    project.layers.push_back(Rc::new(RefCell::new(Layer::new("layer1".into(), "Layer 1".into()))));
    project.layers.push_back(Rc::new(RefCell::new(Layer::new("layer2".into(), "Layer 2".into()))));
    settings_write().maps.push_back(Arc::new(Map::new("map1".into(), "Map 1".into())));
    settings_write().maps.push_back(Arc::new(Map::new("map2".into(), "Map 2".into())));
    settings_write().maps.push_back(Arc::new(Map::new("map3".into(), "Map 3".into())));
    let map_view = Rc::new(RefCell::new(MapView::new()));

    // Open GUI
    let mut main_window = gui::MapWindow::new(Box::new(project), map_view);
    match main_window.run() {
        Ok(()) => { },
        Err(e) => { println!("{}", e); },
    }
}

