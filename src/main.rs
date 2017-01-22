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
#![allow(unused_assignments)] // ...to avoid false warnings

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
extern crate env_logger;

mod core;
mod gui;
mod geocoord;

use std::cell::{RefCell};
use std::process::{exit};

use core::settings::{settings_write, settings_read, APP_NAME, APP_VERSION};
use core::tiles::{create_tile_cache};
use core::atlas::{Atlas, Layer, Map, MapToken, MapView};
use core::persistence::*;

fn main() {
    // Initialize logger
    env_logger::init().unwrap();
    info!("{} {} started", APP_NAME, APP_VERSION);
    
    // Load settings
    info!("Loading settings");
    if let Err(e) = settings_write().load() {
        error!("Failed to load settings: {}", e);
        exit(1);
    }
    
    // Initialize tile cache
    info!("Initialize tile cache");
    let tcache_rrc = create_tile_cache();

    // Create atlas
    let atlas = RefCell::new(Atlas::new("unnamed".into()));
    
    // Load maps from JSON files
    info!("Loading map files");
    for dir_name in vec![settings_read().host_maps_directory(), settings_read().user_maps_directory()] {
        match deserialize_all(dir_name, |map: Map, file_stem: &String| {
            debug!("Loaded map {} ({})", map.name, file_stem);
            atlas.borrow_mut().maps.insert(map.slug.clone(), map);
        }) {
            Ok(()) => { }
            Err(e) => { warn!("Failed to load map: {}", e); }
        }
    }
    
    // Load tokens
    for dir_name in vec![settings_read().host_tokens_directory(), settings_read().user_tokens_directory()] {
        match deserialize_all(dir_name, |token: MapToken, file_stem: &String| {
            debug!("Loaded token {}", file_stem);
            atlas.borrow_mut().tokens.insert(file_stem.clone(), token);
        }) {
            Ok(()) => { }
            Err(e) => { warn!("Failed to load token: {}", e); }
        }
    }

    // Hard-coded sample layers
    debug!("Creating sample layers");
    {    
        let l0 = Layer::new("Backdrop".into(), 0); atlas.borrow_mut().layers.insert(l0.id(), l0);
        let l1 = Layer::new("Layer 1".into(), 1); atlas.borrow_mut().layers.insert(l1.id(), l1);
        let l2 = Layer::new("Layer 2".into(), 2); atlas.borrow_mut().layers.insert(l2.id(), l2);
        let l3 = Layer::new("Layer 3".into(), 3); atlas.borrow_mut().layers.insert(l3.id(), l3);
    }

    // Load map view
    let map_view = RefCell::new(MapView::restore());
    if map_view.borrow().map_slug == "" {
        map_view.borrow_mut().map_slug = "osm-carto".into(); // TODO: better validation
    }

    // Open GUI
    info!("Showing the main window");
    let main_window_r = gui::MapWindow::new_rc(atlas, map_view, tcache_rrc.clone());
    match main_window_r.init(main_window_r.clone()) {
        Ok(()) => { },
        Err(e) => { error!("Failed to open the main window: {}", e); exit(1); },
    }
    tcache_rrc.borrow_mut().observer = Some(main_window_r.clone());

    // Main loop
    info!("Starting the main loop");
    gui::main();

    // Save map view
    main_window_r.map_view.borrow().store();
    
    // Cleanup
    tcache_rrc.borrow_mut().store();
    info!("Exit");
}

