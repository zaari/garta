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

#[macro_use]
extern crate lazy_static;

mod core;
mod gui;
mod gpx;

use std::cell::{RefCell};
use std::rc::{Rc};
use std::sync::{Arc};
use std::process::{exit};

use core::settings::{settings_write};
use core::tiles::TileRequestQueue;
use core::root::{Project, Layer, MapView};
use core::map::Map;

fn main() {
    println!("Garta started");
    
    // Load settings
    println!("Loading settings");
    if let Err(e) = settings_write().load() {
        println!("Failed to load settings: {}", e);
        exit(1);
    }
    
    // Start the threads
    println!("Starting worker threads");
    let trq = TileRequestQueue::new();
    trq.read().unwrap().ping();

    // Project with test data
    let project = Rc::new(RefCell::new(Project::new("unnamed".into())));
    project.borrow_mut().layers.push_back(Rc::new(RefCell::new(Layer::new("layer1".into(), "Layer 1".into()))));
    project.borrow_mut().layers.push_back(Rc::new(RefCell::new(Layer::new("layer2".into(), "Layer 2".into()))));
    settings_write().maps.push_back(Arc::new(Map::new("map1".into(), "Map 1".into())));
    settings_write().maps.push_back(Arc::new(Map::new("map2".into(), "Map 2".into())));
    settings_write().maps.push_back(Arc::new(Map::new("map3".into(), "Map 3".into())));
    let map_view = Rc::new(RefCell::new(MapView::new()));

    // Open GUI
    let mut main_window = gui::MapWindow::new(project, map_view);
    match main_window.run() {
        Ok(()) => { },
        Err(e) => { println!("Failed to open the main window: {}", e); },
    }
}

