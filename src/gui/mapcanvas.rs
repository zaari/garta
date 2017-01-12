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
extern crate gdk;
extern crate glib;
extern crate cairo;

use std::rc::{Rc};
use std::cell::{RefCell};
use log::LogLevel::Debug;
use std::time::{Instant, Duration};
use self::gtk::prelude::*;
use std::collections::{BTreeSet};
use std::process;

use super::mainwindow::{MapWindow};
use core::tiles::*;
use geocoord::geo::{PixelPos, Location};
use gui::floatingtext::*;

pub enum MapCanvasMode {
    /// Waiting for user action passively.
    Void,
    
    /// Element being moved on the map.
    Moving,
    
    /// Map being scrolled on x/y space.
    Scrolling,
    
    /// Smooth zooming animation.
    ZoomAnimation,
}

pub struct MapCanvas {
    /// GKT drawing area widget for the canvas.
    pub widget: Option<gtk::DrawingArea>,
    
    /// Floating text elements at southeast corner from bottom to up. 
    /// The pivot points are relative to southeast corner of the window and 
    /// thus always have negative coordinates.
    float_texts_se: RefCell<Vec<FloatingText>>,

    /// Map window.
    map_win: Option<Rc<MapWindow>>,
    
    /// Current mode of the canvas.
    mode: RefCell<MapCanvasMode>,
    
    /// Mouse location of the previous event.
    orig_pos: RefCell<PixelPos>,
    orig_center: RefCell<Location>,
    
    // Accuracy of the view (degrees per pixel)
    accuracy: RefCell<Option<f64>>,
}

impl MapCanvas {
    pub fn new() -> MapCanvas {
        MapCanvas {
            widget: None,
            float_texts_se: RefCell::new(Vec::new()),
            map_win: None,
            mode: RefCell::new(MapCanvasMode::Void),
            orig_pos: RefCell::new(PixelPos::new(0, 0)),
            orig_center: RefCell::new(Location::new(0.0, 0.0)),
            accuracy: RefCell::new(None),
        }
    }

    pub fn init(&mut self, map_win: Rc<MapWindow>) {
        self.map_win = Some(map_win.clone());
    
        // Create the widget
        let canvas = gtk::DrawingArea::new();
        canvas.set_size_request(512, 512);
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

        // Signal handlers
        let mwin = map_win.clone();
        canvas.connect_draw( move |widget, context| { 
            let map_canvas = mwin.map_canvas.borrow();
            map_canvas.draw(context); 
            Inhibit(true) 
        });
        let mwin = map_win.clone();
        canvas.connect_button_press_event( move |widget, event| { 
            let map_canvas = mwin.map_canvas.borrow();
            map_canvas.button_press_event(event); 
            Inhibit(true) 
        } );
        let mwin = map_win.clone();
        canvas.connect_button_release_event( move |widget, event| { 
            let map_canvas = mwin.map_canvas.borrow();
            map_canvas.button_release_event(event); 
            Inhibit(true) 
        } );
        let mwin = map_win.clone();
        canvas.connect_motion_notify_event( move |widget, event| { 
            let map_canvas = mwin.map_canvas.borrow();
            map_canvas.motion_notify_event(event); 
            Inhibit(true) 
        } );
        let mwin = map_win.clone();
        canvas.connect_scroll_event( move |widget, event| { 
            let map_canvas = mwin.map_canvas.borrow();
            map_canvas.scroll_event(event); 
            Inhibit(true) 
        } );
                                        
        self.widget = Some(canvas);
    }

    /// Copies copyright texts from the map of the view.
    pub fn update_map_meta(&mut self) {
        self.float_texts_se.borrow_mut().clear();
        
        if let Some(ref map_win) = self.map_win {
            let atlas = map_win.atlas.borrow();
            let view = map_win.map_view.borrow();
            if atlas.maps.contains_key(&view.map_slug) {
                let map = atlas.maps.get(&view.map_slug).unwrap();
            
                let nil_pos = PixelPos::new(0, 0);
                for copyright in &map.copyrights {
                    let mut ft = FloatingText::new(
                            TextAnchor::SouthEast, 
                            nil_pos, 
                            copyright.text.clone(), 
                            Some(copyright.url.clone()));
                    ft.font_size = 9;
                    ft.margin = 2;
                    if map.dark {
                        ft.fg_rgba = (1.0, 1.0, 1.0, 1.0);
                        ft.bg_rgba = (0.0, 0.0, 0.0, 0.3);
                        ft.highlight_rgba = (0.0, 0.0, 1.0, 1.0);
                    } else {
                        ft.fg_rgba = (0.0, 0.0, 0.0, 1.0);
                        ft.bg_rgba = (1.0, 1.0, 1.0, 0.3);
                        ft.highlight_rgba = (0.0, 0.0, 1.0, 1.0);
                    }
                    
                    self.float_texts_se.borrow_mut().push(ft);
                }
            } else {
                warn!("Map not found for slug {}", &view.map_slug);
            }
        } else {
            warn!("No map_win");
        }
    }

    /// Calls 'matching' function if the pixel pos is in the floating text 
    /// and 'non_matching' if not.
    fn map_floating_text<F, G>(&self, pos: PixelPos, mut matching: F, mut non_matching: G) 
        where F: FnMut(&mut FloatingText), G: FnMut(&mut FloatingText),
    {
        // Iterate southeast texts
        for mut ft in self.float_texts_se.borrow_mut().iter_mut() {
            let contains = {
                if let Some(ref geometry) = ft.geometry {
                    geometry.contains(pos)
                } else {
                    false
                }
            };
            if contains {
                matching(&mut ft);
            } else {
                non_matching(&mut ft);
            }
        }
    }

    /// Signal handler for draw
    fn draw(&self, c: &cairo::Context) {
        let start_time = Instant::now();
        if let Some(ref widget) = self.widget {
            let vw = widget.get_allocated_width() as i64;
            let vh = widget.get_allocated_height() as i64;

            // Default background color
            let background_color = (0.2f64, 0.2f64, 0.2f64);
        /* TODO: get_background_color is not available on API yet    
            if let Some(style_context) = widget.get_style_context() {
                style_context.get_background_color(gtk::StateFlags::STATE_FLAG_NORMAL);
            }
        */    

            // Map
            if let Some(ref map_win) = self.map_win {
                let mut map_view = map_win.map_view.borrow_mut();
                let atlas = map_win.atlas.borrow();
                if let Some(ref map) = atlas.maps.get(&map_view.map_slug) {
                    // Map projection
                    let mut projection = map.as_projection();
                    
                    // Draw tiles
                    if let Some(tile_source) = map.to_tile_source(&atlas.tokens) {
                        // Compute tile grid dimensions at the current zoom level
                        let tw = tile_source.tile_width as i64;
                        let th = tile_source.tile_height as i64;
                        let zoom_level = map_view.zoom_level;
                        let mult = 1;
                        let center = map_view.center;
                        let ppdoe = ((tw as u64) << (zoom_level as u64)) as f64 / 360.0;
                        let global_nw_pos = projection.northwest_global_pixel(ppdoe);
                        let center_pos = projection.location_to_global_pixel_pos(center, ppdoe);
                        let view_nw_pos = center_pos - PixelPos::new(vw / 2, vh / 2);
                        let offset_pos = PixelPos::new(
                                (view_nw_pos.x - global_nw_pos.x) % tw, 
                                (view_nw_pos.y - global_nw_pos.y) % th);
                        //debug!("{:?} - {:?} = {:?}", center_pos, PixelPos::new(vw / 2, vh / 2), view_nw_pos);
                        let gx = ((view_nw_pos.x - global_nw_pos.x) / tw) as i64;
                        let gy = ((view_nw_pos.y - global_nw_pos.y) / th) as i64;
                        let gw = ((vw + tw - 1) / tw + 1) as i64;
                        let gh = ((vh + th - 1) / th + 1) as i64;
                    
                        // Tile cache    
                        let mut tcache = map_win.tile_cache.borrow_mut();

                        // Background color
                        c.set_source_rgb(background_color.0, background_color.1, background_color.2);
                        c.paint();

                        //debug!("ppdoe={} zoom_level={} gx={} gy={} gw={} gh={} tw={} th={}", ppdoe, zoom_level, gx, gy, gw, gh, tw, th);

                        // Create an ordered list of tile requests
                        let mut treqs: BTreeSet<TileRequest> = BTreeSet::new();
                        let focus_pos = projection.location_to_global_pixel_pos(map_view.focus.unwrap_or(center), ppdoe) - view_nw_pos;
                        for ly in 0..gh {
                            for lx in 0..gw {
                                // Priority depends on tile's center's distance from the center
                                let pri_xy = PixelPos::new(lx * tw + tw/2, ly * th + th/2) - focus_pos - offset_pos;
                                let pri = -pri_xy.cathetus2();
                                
                                // Add to the ordered set
                                treqs.insert(TileRequest::new(map_view.request_generation, pri,
                                    gx + lx, gy + ly, zoom_level, mult, tile_source.clone()));
                            }
                        }
                                    
                        // Request tile
                        for treq in treqs.iter().rev() {
                            // Handle the response
                            if let Some(tile) = tcache.get_tile(&treq) {
                                // Draw tile
                                if let Some(ref tile_surface) = tile.get_surface() {
                                    let lx = treq.x - gx;
                                    let ly = treq.y - gy;
                                    let vx = (lx * tw - offset_pos.x) as f64;
                                    let vy = (ly * th - offset_pos.y) as f64;
                                    c.set_source_surface(tile_surface, vx, vy);
                                    c.paint();
                                }
                            }
                        }
                        
                        // Increase the generation
                        if map_view.request_generation == u64::max_value() {
                            map_view.request_generation = 1;
                        } else {
                            map_view.request_generation += 1;
                        }
                        
                        // Update accuracy as it's relatively cheap to compute it here
                        let view_se_pos = center_pos + PixelPos::new(vw / 2, vh / 2);
                        let view_nw_loc = projection.global_pixel_pos_to_location(view_nw_pos, ppdoe);
                        let view_se_loc = projection.global_pixel_pos_to_location(view_se_pos, ppdoe);
                        let accuracy = (view_nw_loc.lat - view_se_loc.lat) / (vh as f64);
                        if accuracy > 0.0 {
                            *self.accuracy.borrow_mut() = Some(accuracy);
                        } else {
                            *self.accuracy.borrow_mut() = None;
                        }
                    } else {
                        warn!("No tile source for map {}", &map_view.map_slug);
                    }
                    
                    // Draw copyright texts
                    let margin = 2;
                    let mut ty = -margin;
                    for float_text in self.float_texts_se.borrow_mut().iter_mut() {
                        // Draw the text
                        float_text.pivot = PixelPos::new(-float_text.margin - margin, ty);
                        float_text.draw(c, PixelPos::new(vw, vh));
                        ty -= float_text.font_size + 2 * float_text.margin + margin;
                    }
                } else {
                    warn!("No map for slug {}", &map_view.map_slug);
                }
            }
        }

        // Debug draw time    
        if log_enabled!(Debug) { 
            let ms = 1000.0 * duration_to_seconds(&start_time.elapsed());
            if ms >= 15.000 {
                debug!("draw time: {:.3}", ms); 
            }
        }
    }

    /// Event handler for mouse button press. Either start dragging a map element or scrolling the 
    /// map. This doesn't select map element (to avoid accidental drag instead of scroll).
    fn button_press_event(&self, ev: &gdk::EventButton) {
        let pos = PixelPos::new_with_f64_tuple(ev.get_position());
        debug!("button_press_event: {:?}", pos);

        // Check whether the click is on a map element hotspot or not
        if let Some(ref map_win) = self.map_win {
            if false {
                // Select the map element
            } else {
                // Start scrolling
                *self.mode.borrow_mut() = MapCanvasMode::Scrolling;
                *self.orig_pos.borrow_mut() = pos;
                *self.orig_center.borrow_mut() = map_win.map_view.borrow().center;
            }    
        }
    }

    /// Event handler for mouse button release. Either stop drag or scroll, or select a new 
    /// map element.
    fn button_release_event(&self, ev: &gdk::EventButton) {
        if let Some(ref map_win) = self.map_win {
            let pos = PixelPos::new_with_f64_tuple(ev.get_position());
            debug!("button_release_event: {:?}", pos);
            
            // Either end the drag, scrolling or just keep the selection
            debug!("map_window: {}", map_win.map_view.borrow().zoom_level);

            // Open a url if one of the floating texts is clicked.
            let url: RefCell<Option<String>> = RefCell::new(None);
            self.map_floating_text(pos, 
                { |ft| { *url.borrow_mut() = ft.url.clone(); } }, 
                { |ft| { } }) ;
            if let Some(ref url) = url.into_inner() {
                info!("Opening url: {}", url);
                match process::Command::new("xdg-open").arg(url).spawn() {
                    Ok(child) => { }
                    Err(e) => {
                        error!("Failed to open the url: {}", e);
                    }
                }
            }
        }
        *self.mode.borrow_mut() = MapCanvasMode::Void;
    }

    /// Event handler for mouse motion. Either drag or scroll.
    fn motion_notify_event(&self, ev: &gdk::EventMotion) {
        if let Some(ref map_win) = self.map_win {
            let update_map = RefCell::new(false);
            let pos = PixelPos::new_with_f64_tuple(ev.get_position());
            match *self.mode.borrow() {
                MapCanvasMode::Void => {
                    // Mouse moving over the map
                    let focus = self.position_to_location(pos);
                    map_win.map_view.borrow_mut().focus = focus;
                    map_win.update_coordinates_button(focus, *self.accuracy.borrow());
                    
                    // Check for possible hover highlight
                    self.map_floating_text(pos, 
                        { |ft| {
                            if !ft.highlight && ft.url.is_some() {
                                debug!("Highlight: {}", ft.text);
                                ft.highlight = true;
                                *update_map.borrow_mut() = true;
                            }
                        } }, 
                        { |ft| {
                            if ft.highlight {
                                ft.highlight = false;
                                *update_map.borrow_mut() = true;
                            }
                        } }) ;
                },
                MapCanvasMode::Moving => {
                }
                MapCanvasMode::Scrolling => {
                    // Compute delta
                    let orig_pos = *self.orig_pos.borrow();
                    let delta_pos = pos - orig_pos;
                    
                    if !delta_pos.is_origin() {
                        // Move center of the view
                        let orig_center_pos = self.location_to_position(*self.orig_center.borrow()).unwrap();
                        let new_center = self.position_to_location(orig_center_pos - delta_pos).unwrap();
                        map_win.map_view.borrow_mut().center = new_center;

                        // Request a map update                        
                        *update_map.borrow_mut() = true;
                    }
                    
                    //let self.location_to_position();
                    debug!("motion_notify_event: pos={:?} delta={:?} center={}", pos, delta_pos, map_win.map_view.borrow().center);
                }
                _ => { }
            }

            // Request map update if needed.
            if *update_map.borrow() == true {
                map_win.update_map();
            }
        }
    }

    /// Event handler for mouse motion. Either drag or scroll.
    fn scroll_event(&self, ev: &gdk::EventScroll) {
        if let Some(ref map_win) = self.map_win {
            if {
                let mut r = false;
                if let Some(ref widget) = self.widget {
                    // Convert mouse position to focus location
                    let (x, y) = ev.get_position();
                    let mouse_location = self.position_to_location(PixelPos::new_with_f64(x, y));
                
                    let mut map_view = map_win.map_view.borrow_mut();
                    let (dx, dy) = ev.get_delta();
                    debug!("scroll_event: {},{} delta={},{}", x as i32, y as i32, dx, dy);
                    
                    // Zoom direction
                    if dy < 0.0 {
                        if map_view.zoom_level < 17 { // TODO
                            map_view.zoom_level += 1;
                            map_view.focus = mouse_location;
                            map_view.center = map_view.center.weighted_average(
                                    &map_view.focus.unwrap_or(map_view.center), 0.5);
                            r = true;
                        }
                    } else if dy > 0.0 {
                        if map_view.zoom_level > 0 {
                            map_view.zoom_level -= 1;                
                            map_view.focus = mouse_location;
                            map_view.center = map_view.center.weighted_average(
                                    &map_view.focus.unwrap_or(map_view.center), 0.5);
                            r = true;
                        }
                    }
                }
                r
            } {
                // Request map update
                map_win.update_map();
                map_win.update_zoom_level_label(map_win.map_view.borrow().zoom_level);
            }
        }
    }

    /// Convert window position to location. Returns None if map is not found.
    fn position_to_location(&self, local_pos: PixelPos) -> Option<Location> {
        if let Some(ref map_win) = self.map_win {
            if let Some(ref widget) = self.widget {
                let vw = widget.get_allocated_width() as i64;
                let vh = widget.get_allocated_height() as i64;

                // Map
                let map_view = map_win.map_view.borrow();
                if let Some(ref map) = map_win.atlas.borrow().maps.get(&map_view.map_slug) {
                    // Map projection
                    let mut projection = map.as_projection();
                    
                    if let Some(tw) = map.tile_width {
                        let ppdoe = ((tw as u64) << (map_view.zoom_level as u64)) as f64 / 360.0;
                        let global_nw_pos = projection.northwest_global_pixel(ppdoe);            
                        let center_pos = projection.location_to_global_pixel_pos(map_view.center, ppdoe);
                        let view_nw_pos = center_pos - PixelPos::new(vw / 2, vh / 2);
                        
                        let global_pos = view_nw_pos + local_pos;            
                        return Some(projection.global_pixel_pos_to_location(global_pos, ppdoe));
                    }
                }
            }
        }
        return None;
    }

    /// Convert location to a local window position.
    fn location_to_position(&self, loc: Location) -> Option<PixelPos> {
        if let Some(ref map_win) = self.map_win {
            if let Some(ref widget) = self.widget {
                let vw = widget.get_allocated_width() as i64;
                let vh = widget.get_allocated_height() as i64;

                // Map
                let map_view = map_win.map_view.borrow();
                if let Some(ref map) = map_win.atlas.borrow().maps.get(&map_view.map_slug) {
                    // Map projection
                    let mut projection = map.as_projection();
                    
                    if let Some(tw) = map.tile_width {
                        let ppdoe = ((tw as u64) << (map_view.zoom_level as u64)) as f64 / 360.0;
                        let global_nw_pos = projection.northwest_global_pixel(ppdoe);            
                        let center_pos = projection.location_to_global_pixel_pos(map_view.center, ppdoe);
                        let view_nw_pos = center_pos - PixelPos::new(vw / 2, vh / 2);
                        
                        let global_pos = projection.location_to_global_pixel_pos(loc, ppdoe);
                        return Some(global_pos - view_nw_pos);
                    }
                }
            }
        }
        return None;
    }
}

// -------------------------------------------------------------------------------------------------

/// Convert duration to milliseconds
fn duration_to_seconds(i: &Duration) -> f64 {
    let secs = i.as_secs() as f64;
    let nsecs = i.subsec_nanos() as f64;
    (secs + 0.000000001 * nsecs)
}


