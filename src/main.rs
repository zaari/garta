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
#![allow(unused_assignments)] // ...to avoid false warnings (I should file a bug report about this)

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate log;
extern crate env_logger;

mod geocoord;
mod core;
mod gui;

use std::cell::{RefCell};
use std::process::{exit};
use std::time::{Instant};

use core::settings::{settings_write, settings_read, APP_NAME, APP_VERSION};
use core::tiles::{create_tile_cache};
use core::atlas::{Atlas, Layer, Map, MapToken, MapView};
use core::persistence::*;
use core::misc::{duration_to_seconds};

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
    let tcache_time0 = Instant::now();
    info!("Initializing tile cache");
    let tcache_rc = create_tile_cache();
    debug!("Cache initialized in {:.3} seconds", duration_to_seconds(&tcache_time0.elapsed()));

    // Create atlas
    let atlas = RefCell::new(Atlas::new("unnamed".into()));
    
    // Load maps from JSON files
    info!("Loading map files");
    for dir_name in settings_read().map_directories() {
        match deserialize_all(dir_name, |map: Map, file_stem: &String| {
            debug!("Loaded map {} ({})", map.name, file_stem);
            if map.url_templates.len() > 0 {
                atlas.borrow_mut().maps.insert(map.slug.clone(), map);
            } else {
                warn!("Map {} doesn't have any urls defined!", map.name);
            }
        }) {
            Ok(()) => { }
            Err(e) => { warn!("Failed to load map: {}", e); }
        }
    }
    if atlas.borrow().maps.len() == 0 {
        error!("No maps found.");
        exit(1);
    }
    
    // Load tokens (if exist)
    for dir_name in settings_read().token_directories() {
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

    // TODO: Load atlas...

    // Load map view
    let map_view = RefCell::new(MapView::restore());
    if map_view.borrow().map_slug == "" {
        map_view.borrow_mut().map_slug = "osm-carto".into(); // TODO: better validation
    }

    // Create GUI and run GTK main
    info!("Run {} with GUI", APP_NAME);
    match gui::run_app(atlas, map_view, tcache_rc.clone()) {
        Ok(map_win_r) => {
            // Persist map view state
            map_win_r.map_view.borrow().store();
        },
        Err(e) => {
            error!("Failed to run the app: {}", e);
            exit(1);
        }
    }

    // Persist tile cache state
    tcache_rc.borrow_mut().store();
    debug!("Exit");
}

