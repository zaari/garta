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

use core::settings::{settings_write};
use core::tiles::{create_tile_cache};
use core::root::{Atlas, Layer, MapView};
use core::map::{Map};
use core::id::{UniqueId};
use core::persistence::*;

fn main() {
    // Initialize logger
    env_logger::init().unwrap();
    debug!("Garta started");
    
    // Load settings
    debug!("Loading settings");
    if let Err(e) = settings_write().load() {
        error!("Failed to load settings: {}", e);
        exit(1);
    }
    
    // Start the threads
    debug!("Starting worker threads");
    let tcache = create_tile_cache();
    tcache.borrow_mut().init();

    // Create atlas
    let atlas = Rc::new(RefCell::new(Atlas::new("unnamed".into())));
    
    // Load maps from JSON files
    match deserialize_all("map", |map: Map| {
        let mut a = atlas.borrow_mut();
        debug!("Loaded map {}", map.name);
        a.maps.insert(map.id(), map);
    }) {
        Ok(()) => { }
        Err(e) => { warn!("Failed to load map: {}", e); }
    }
    let m2id: UniqueId = 2; // FIXME

    // Hard-coded sample layers
    {    
        let mut a = atlas.borrow_mut();
        let l0 = Layer::new("Backdrop".into(), 0); a.layers.insert(l0.id(), l0);
        let l1 = Layer::new("Layer 1".into(), 1); a.layers.insert(l1.id(), l1);
        let l2 = Layer::new("Layer 2".into(), 2); a.layers.insert(l2.id(), l2);
        let l3 = Layer::new("Layer 3".into(), 3); a.layers.insert(l3.id(), l3);
    }

    // Open GUI
    let map_view = Rc::new(RefCell::new(MapView::new()));
    map_view.borrow_mut().map_id = m2id;
    let main_window = Rc::new(RefCell::new(gui::MapWindow::new(atlas, map_view, tcache.clone())));
    tcache.borrow_mut().observer = Some(main_window.clone());
    match main_window.borrow_mut().init() {
        Ok(()) => { },
        Err(e) => { error!("Failed to open the main window: {}", e); },
    }

    // Main loop
    gui::main();
}

