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

#![allow(dead_code)]
#![allow(unused_variables)]

#![feature(proc_macro)]
#[macro_use] extern crate serde_derive;

#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
extern crate env_logger;

mod core;
mod gui;
mod geocoord;

use std::cell::{RefCell};
use std::rc::{Rc};
use std::process::{exit};

use core::settings::{settings_write, settings_read};
use core::tiles::{create_tile_cache};
use core::root::{Atlas, Layer, MapView};
use core::map::{Map};
use core::persistence::*;

fn main() {
    // Initialize logger
    env_logger::init().unwrap();
    info!("Garta started");
    
    // Load settings
    info!("Loading settings");
    if let Err(e) = settings_write().load() {
        error!("Failed to load settings: {}", e);
        exit(1);
    }
    
    // Initialize tile cache
    info!("Initialize tile cache");
    let tcache = create_tile_cache();

    // Create atlas
    let atlas = Rc::new(RefCell::new(Atlas::new("unnamed".into())));
    
    // Load maps from JSON files
    info!("Loading map files");
    for dir_name in vec![settings_read().host_maps_directory(), settings_read().user_maps_directory()] {
        match deserialize_all(dir_name, |map: Map, file_stem: &String| {
            let mut a = atlas.borrow_mut();
            debug!("Loaded map {} ({})", map.name, file_stem);
            a.maps.insert(map.slug.clone(), map);
        }) {
            Ok(()) => { }
            Err(e) => { warn!("Failed to load map: {}", e); }
        }
    }

    // Hard-coded sample layers
    debug!("Creating sample layers");
    {    
        let mut a = atlas.borrow_mut();
        let l0 = Layer::new("Backdrop".into(), 0); a.layers.insert(l0.id(), l0);
        let l1 = Layer::new("Layer 1".into(), 1); a.layers.insert(l1.id(), l1);
        let l2 = Layer::new("Layer 2".into(), 2); a.layers.insert(l2.id(), l2);
        let l3 = Layer::new("Layer 3".into(), 3); a.layers.insert(l3.id(), l3);
    }

    // Open GUI
    info!("Showing the main window");
    let map_view = Rc::new(RefCell::new(MapView::new()));
    map_view.borrow_mut().map_slug = "osm-carto".into();
    let main_window = Rc::new(RefCell::new(gui::MapWindow::new(atlas, map_view, tcache.clone())));
    tcache.borrow_mut().observer = Some(main_window.clone());
    match main_window.borrow_mut().init() {
        Ok(()) => { },
        Err(e) => { error!("Failed to open the main window: {}", e); },
    }

    // Main loop
    info!("Starting the main loop");
    gui::main();
    
    // Cleanup
    tcache.borrow_mut().store();
    info!("Exit");
}

