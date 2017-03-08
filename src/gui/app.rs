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

extern crate gtk;
extern crate gio;
extern crate glib;

use std::rc::{Rc};
use std::cell::{RefCell};
use std::result::*;
use std::env;
use self::gtk::prelude::*;
use core::atlas::{Atlas, MapView};
use core::tiles::{TileCache};
use core::settings::{APP_ID};
use gui::mapwindow::{MapWindow};

/// Run GTK application.
pub fn run_app(atlas: RefCell<Atlas>, map_view: RefCell<MapView>, tcache_rrc: Rc<RefCell<TileCache>>) -> Result<Rc<MapWindow>, String> {
    // Create map window and set it as tile cache observer
    let map_win_r = MapWindow::new_r(atlas, map_view, tcache_rrc.clone());
    tcache_rrc.borrow_mut().observer = Some(map_win_r.clone());

    // Create and run GTK app
    let app = match gtk::Application::new(Some(APP_ID), gio::APPLICATION_FLAGS_NONE) {
        Ok(app) => {
            // Handle 'active' signal sent by gtk::Application::run method
            let map_win_r = map_win_r.clone();
            app.connect_activate(move |app| {
                // Call MapWindow::init where the GUI is created
                match map_win_r.init(map_win_r.clone(), app) {
                    Ok(()) => {
                    },
                    Err(e) => {
                        error!("Failed to create user interface: {}", e);
                    }
                }
            });

            // Run GTK application with command line args
            let args: Vec<String> = env::args().collect();
            let argv: Vec<&str> = args.iter().map(|x| x.as_str()).collect();
            app.run(argv.len() as i32, argv.as_slice());
        },
        Err(e) => {
            return Err(format!("Failed to create gtk app: {:?}", e));
        }
    };
    
    Ok(map_win_r)
}


