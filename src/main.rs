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

#![allow(dead_code)]
#![allow(unused_variables)]

#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
extern crate env_logger;

mod core;
mod gui;
mod gpx;

use std::cell::{RefCell};
use std::rc::{Rc};
use std::process::{exit};

use core::settings::{settings_write};
use core::tiles::TileRequestQueue;
use core::root::{Atlas, Layer, MapView};
use core::map::Map;

fn main() {
    env_logger::init().unwrap();
    info!("Garta started");
    
    // Load settings
    debug!("Loading settings");
    if let Err(e) = settings_write().load() {
        error!("Failed to load settings: {}", e);
        exit(1);
    }
    
    // Start the threads
    debug!("Starting worker threads");
    let trq = TileRequestQueue::new();
    trq.read().unwrap().ping();

    // Generated model for testing
    let atlas = Rc::new(RefCell::new(Atlas::new("unnamed".into())));
    let m2id = {
        let mut p = atlas.borrow_mut();
        let m1 = Map::new("Map 1".into()); p.maps.insert(m1.id(), m1);
        let m2 = Map::new("Map 2".into()); let m2id = m2.id(); p.maps.insert(m2.id(), m2);
        let m3 = Map::new("Map 3".into()); p.maps.insert(m3.id(), m3);
        let l0 = Layer::new("Backdrop".into(), 0); p.layers.insert(l0.id(), l0);
        let l1 = Layer::new("Layer 1".into(), 1); p.layers.insert(l1.id(), l1);
        let l2 = Layer::new("Layer 2".into(), 2); p.layers.insert(l2.id(), l2);
        let l3 = Layer::new("Layer 3".into(), 3); p.layers.insert(l3.id(), l3);
        m2id
    };

    // Open GUI
    let map_view = Rc::new(RefCell::new(MapView::new()));
    map_view.borrow_mut().map_id = m2id;
    let mut main_window = gui::MapWindow::new(atlas, map_view);
    match main_window.run() {
        Ok(()) => { },
        Err(e) => { error!("Failed to open the main window: {}", e); },
    }
}

