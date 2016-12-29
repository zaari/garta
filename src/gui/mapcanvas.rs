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

extern crate gtk;
extern crate gio;
extern crate gdk;
extern crate glib;
extern crate cairo;

use std::cell::{RefCell};
use std::rc::{Rc};
use log::LogLevel::Debug;
use std::time::{Instant, Duration};
use self::gtk::prelude::*;

use super::mainwindow::MapWindow;
use core::tiles::*;

/// Create map canvas widget with all the needed signals connected.
pub fn build_map_canvas(map_win: &MapWindow) -> gtk::DrawingArea {
    // Create the widget
    let canvas = gtk::DrawingArea::new();
    canvas.set_size_request(800, 800);
    canvas.set_visible(true);
    canvas.set_sensitive(true);

    // Enable the events you wish to get notified about.
    // The 'draw' event is already enabled by the DrawingArea.
    canvas.add_events( 
        // TODO: symbolic values instead of numberic ones (gdk/gdktypes.h) when
        // gtk-rs gets fixed; http://gtk-rs.org/docs/gdk/struct.EventMask.html
                    (1 << 8) // gdk::EventMask::BUTTON_PRESS_MASK
	              | (1 << 9) // gdk::EventMask::BUTTON_RELEASE_MASK
	              | (1 << 2) // gdk::EventMask::POINTER_MOTION_MASK
	              | (1 << 23) // gdk::EventMask::SMOOTH_SCROLL_MASK
	              | (1 << 10) // gdk::EventMask::KEY_PRESS_MASK
	              | (1 << 11) // gdk::EventMask::KEY_RELEASE_MASK
    );

    // Signal handler
    let map_win_rr = Rc::new(RefCell::new(map_win.clone()));
    canvas.connect_draw( move |widget, context| 
                                    { draw(widget, context, map_win_rr.clone()); Inhibit(true) });                            
    let map_win_rr = Rc::new(RefCell::new(map_win.clone()));
    canvas.connect_button_press_event( move |widget, event| 
                                    { button_press_event(widget, event, map_win_rr.clone()); Inhibit(true) } );
    let map_win_rr = Rc::new(RefCell::new(map_win.clone()));
    canvas.connect_button_release_event( move |widget, event| 
                                    { button_release_event(widget, event, map_win_rr.clone()); Inhibit(true) } );
    let map_win_rr = Rc::new(RefCell::new(map_win.clone()));
    canvas.connect_motion_notify_event( move |widget, event| 
                                    { motion_notify_event(widget, event, map_win_rr.clone()); Inhibit(true) } );

    // Return the widget    
    canvas
}

/// Signal handler for draw
fn draw(widget: &gtk::DrawingArea, c: &cairo::Context, map_win_rr: Rc<RefCell<MapWindow>>) {
    let start_time = Instant::now();
    let (width, height) = widget.get_size_request();

    // Tile source
    let mut tile_source = TileSource::new();
    tile_source.slug = "osm-carto".into();
    tile_source.name = "OpenStreetMap Carto".into();
    tile_source.urls.push("http://a.tile.openstreetmap.org/${z}/${x}/${y}.png".into());
    tile_source.urls.push("http://b.tile.openstreetmap.org/${z}/${x}/${y}.png".into());
    tile_source.urls.push("http://c.tile.openstreetmap.org/${z}/${x}/${y}.png".into());

    // Tile dimensions
    let tw = tile_source.tile_width;
    let th = tile_source.tile_height;

    // Tile cache    
    let tcache_rr = map_win_rr.borrow().tile_cache.clone();
    let mut tcache = tcache_rr.borrow_mut();
    let treq = TileRequest::new(1, 1, 0, 0, 1, 1, tile_source);
    if let Some(tile) = tcache.get_tile(&treq) {
        tile.get_surface();

        // Draw tile
        if let Some(ref tile_surface) = tile.get_surface() {
            c.set_source_surface(tile_surface, ((width - tw) / 2) as f64, ((height - th) / 2) as f64);
            c.paint();
        }
    }
    
    if log_enabled!(Debug) { debug!("draw time: {} ms width={}", duration_to_millisconds(&start_time.elapsed()), width); }
}

/// Event handler for mouse button press. Either start dragging a map element or scrolling the map.
/// This doesn't select map element (to avoid accidental drag instead of scroll).
fn button_press_event(widget: &gtk::DrawingArea, ev: &gdk::EventButton, map_win_rr: Rc<RefCell<MapWindow>>) {
    let (x, y) = ev.get_position();
    debug!("button_press_event: {},{}", x as i32, y as i32);

    // Check whether the click is on a map element hot spot or not
    if false {
        // Select the map element
    } else {
        // Start scrolling
    }    
}

/// Event handler for mouse button release. Either stop drag or scroll, or select a new map element.
fn button_release_event(widget: &gtk::DrawingArea, ev: &gdk::EventButton, map_win_rr: Rc<RefCell<MapWindow>>) {
    let (x, y) = ev.get_position();
    debug!("button_release_event: {},{}", x as i32, y as i32);
    
    // Either end the drag, scrolling or just keep the selection
    let map_win = map_win_rr.borrow();
    debug!("map_window: {}", map_win.map_view.borrow().zoom_level);
}

/// Event handler for mouse motion. Either drag or scroll.
fn motion_notify_event(widget: &gtk::DrawingArea, ev: &gdk::EventMotion, map_win_rr: Rc<RefCell<MapWindow>>) {
    let (x, y) = ev.get_position();
    //debug!("motion_notify_event: {},{}", x as i32, y as i32);
}

/// Convert duration to milliseconds
fn duration_to_millisconds(i: &Duration) -> u64 {
    let secs = i.as_secs() as f64;
    let nsecs = i.subsec_nanos() as f64;
    ((secs + 0.000000001 * nsecs) * 1000.0 + 0.5) as u64
}


