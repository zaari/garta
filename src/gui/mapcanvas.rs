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
extern crate chrono;

use std::rc::{Rc};
use std::cell::{RefCell};
use log::LogLevel::Debug;
use std::time::{Instant, Duration};
use std::collections::{BTreeSet, VecDeque};
use std::process;
use self::gtk::prelude::*;
use self::cairo::{Format, ImageSurface};

use super::mainwindow::{MapWindow};
use self::chrono::{UTC};
use core::tiles::*;
use geocoord::geo::{Vector, Location, Projection};
use gui::floatingtext::*;
use core::settings::{settings_read};

// Animation frames per second
const ANIMATION_FPS: f64 = 60.0;

// Scroll history minimum length in elements 
const ANIMATION_SCROLL_HISTORY_LENGTH: usize = 1000;

// Scroll history minimum age in seconds (to start scroll animation)
const ANIMATION_SCROLL_HISTORY_MIN_AGE: f64 = 0.05;

// Minimum pixels per second (to start scroll animation)
const ANIMATION_SCROLL_SPEED_THRESHOLD: f64 = 50.0;

// Maximum pixels per second
const ANIMATION_SCROLL_SPEED_LIMIT: f64 = 2000.0;

// Scroll speed decay ratio per second
const ANIMATION_SCROLL_DECAY: f64 = 0.046;

#[derive(Debug, PartialEq)]
pub enum MapCanvasMode {
    /// Waiting for user action passively.
    Void,
    
    /// Element being moved on the map.
    Moving,
    
    /// Map being scrolled on x/y space manually.
    Scrolling,

    /// Inertia scrolling animation.
    ScrollAnimation,
    
    /// Smooth zooming animation.
    ZoomAnimation,
}

pub struct MapCanvas {
    /// GKT drawing area widget for the canvas.
    pub widget: Option<gtk::DrawingArea>,
    
    // Floating text elements at southeast corner from bottom to up. 
    // The pivot points are relative to southeast corner of the window and 
    // thus always have negative coordinates.
    float_texts_se: RefCell<Vec<FloatingText>>,

    // Map window.
    map_win: Option<Rc<MapWindow>>,
    
    // Current mode of the canvas.
    mode: RefCell<MapCanvasMode>,
    
    // Temporary surface for tile grid
    tile_surface: RefCell<Option<ImageSurface>>,
    tile_surface_size: RefCell<(i32, i32)>,
    
    // Mouse location of the previous event.
    orig_pos: RefCell<Vector>,
    orig_center: RefCell<Location>,
    
    // Accuracy of the view (degrees per pixel)
    accuracy: RefCell<Option<f64>>,
    
    // Scroll position and time history. The positions are mouse positions relative to the 
    // canvas corner.
    scroll_history: RefCell<VecDeque<(Vector, Instant)>>,
    
    // In ZoomAnimation mode, the speed vector of scrolling.
    scroll_speed_vec: RefCell<Vector>,
    scroll_prev_time: RefCell<Instant>,
    scroll_center_fpos: RefCell<Vector>,
}

impl MapCanvas {
    pub fn new() -> MapCanvas {
        MapCanvas {
            widget: None,
            float_texts_se: RefCell::new(Vec::new()),
            map_win: None,
            mode: RefCell::new(MapCanvasMode::Void),
            tile_surface: RefCell::new(None),
            tile_surface_size: RefCell::new((0, 0)),
            orig_pos: RefCell::new(Vector::zero()),
            orig_center: RefCell::new(Location::new(0.0, 0.0)),
            accuracy: RefCell::new(None),
            scroll_history: RefCell::new(VecDeque::with_capacity(ANIMATION_SCROLL_HISTORY_LENGTH)),
            scroll_speed_vec: RefCell::new(Vector::zero()),
            scroll_prev_time: RefCell::new(Instant::now()),
            scroll_center_fpos: RefCell::new(Vector::zero()),
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
            
                let nil_pos = Vector::zero();
                for copyright in &map.copyrights {
                    let mut ft = FloatingText::new(
                            TextAnchor::SouthEast, 
                            nil_pos, 
                            copyright.text.clone(), 
                            Some(copyright.url.clone()));
                    ft.font_size = 9.0;
                    ft.margin = 2.0;
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
    fn map_floating_text<F, G>(&self, pos: Vector, mut matching: F, mut non_matching: G) 
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
            let vw = widget.get_allocated_width() as f64;
            let vh = widget.get_allocated_height() as f64;

            // Default background color
            let background_color = (0.2f64, 0.2f64, 0.2f64);
        /* TODO: get_background_color is not available on API yet    
            if let Some(style_context) = widget.get_style_context() {
                style_context.get_background_color(gtk::StateFlags::STATE_FLAG_NORMAL);
            }
        */    
            let round_all_coordinates = { *self.mode.borrow() == MapCanvasMode::Void };

            // Condition round function
            let cr = |a: f64| { if round_all_coordinates { a.round() } else { a } };

            // Map
            if let Some(ref map_win) = self.map_win {
                let map_view = map_win.map_view.borrow();
                let atlas = map_win.atlas.borrow();
                if let Some(ref map) = atlas.maps.get(&map_view.map_slug) {
                    // Map projection
                    let mut projection = map.as_projection();
                    
                    // Draw tiles
                    if let Some(tile_source) = map.to_tile_source(&atlas.tokens) {
                        // Compute tile grid dimensions at the current zoom level
                        let tw = tile_source.tile_width as f64;
                        let th = tile_source.tile_height as f64;
                        let zoom_level = map_view.zoom_level;
                        let mult = 1;
                        let center = map_view.center;
                        let ppdoe = ((tw as u64) << (zoom_level as u64)) as f64 / 360.0;
                        let global_nw_pos = projection.northwest_global_pixel(ppdoe);
                        let center_pos = projection.location_to_global_pixel_pos(center, ppdoe);
                        let view_nw_pos = center_pos - Vector::new(vw / 2.0, vh / 2.0);
                        let offset_pos = Vector::new(
                                (view_nw_pos.x - global_nw_pos.x) % tw, 
                                (view_nw_pos.y - global_nw_pos.y) % th);
                        //debug!("{:?} - {:?} = {:?}", center_pos, Vector::new(vw / 2, vh / 2), view_nw_pos);
                        let gx = ((view_nw_pos.x - global_nw_pos.x) / tw) as i32;
                        let gy = ((view_nw_pos.y - global_nw_pos.y) / th) as i32;
                        let gw = ((vw + tw - 1.0) / tw + 1.0) as i32;
                        let gh = ((vh + th - 1.0) / th + 1.0) as i32;
                    
                        // Tile cache    
                        let mut tcache = map_win.tile_cache.borrow_mut();

                        // Background color
                        c.set_source_rgb(background_color.0, background_color.1, background_color.2);
                        c.paint();

                        //debug!("ppdoe={} zoom_level={} gx={} gy={} gw={} gh={} tw={} th={}", ppdoe, zoom_level, gx, gy, gw, gh, tw, th);

                        // Create an ordered list of tile requests
                        let mut treqs: BTreeSet<TileRequest> = BTreeSet::new();
                        let focus_pos = projection.location_to_global_pixel_pos(map_view.focus.unwrap_or(center), ppdoe) - view_nw_pos;
                        let gen = UTC::now().timestamp() as u64;
                        for ly in 0..gh {
                            for lx in 0..gw {
                                // Priority depends on tile's center's distance from the center and time
                                let pri_xy = Vector::new(lx as f64 * tw + tw/2.0, 
                                                         ly as f64 * th + th/2.0) 
                                                         - focus_pos - offset_pos;
                                let pri = -pri_xy.cathetus2();
                                
                                // Add to the ordered set
                                treqs.insert(TileRequest::new(gen, pri as i64,
                                    gx + lx as i32, gy + ly as i32, zoom_level, mult, tile_source.clone()));
                            }
                        }

                        // Use a separate image surface for tiles to avoid seams when not rounding
                        let mut tile_surface_size = self.tile_surface_size.borrow_mut();
                        let mut tile_surface = self.tile_surface.borrow_mut();
                        let tssz = ((gw as f64 * tw) as i32, (gh as f64 * th) as i32);
                        if tile_surface.is_none() || *tile_surface_size != tssz {
                            *tile_surface = Some(ImageSurface::create(Format::Rgb24, tssz.0, tssz.1));
                            *tile_surface_size = tssz;
                        }
                        if let Some(ref tile_surface) = *tile_surface {
                            let tc = cairo::Context::new(&tile_surface);
                            
                            // Clear surface
                            c.set_source_rgb(0.8, 0.8, 0.8);
                            tc.paint();
                            
                            // Request tiles
                            for treq in treqs.iter().rev() {
                                // Handle the response
                                if let Some(tile) = tcache.get_tile(&treq) {
                                    // Draw tile
                                    if let Some(ref tile_surface) = tile.get_surface() {
                                        let lx = treq.x - gx;
                                        let ly = treq.y - gy;
                                        let vx = lx as f64 * tw;
                                        let vy = ly as f64 * th;
                                        tc.set_source_surface(tile_surface, cr(vx), cr(vy));
                                        tc.paint();
                                    }
                                }
                            }
                            
                            // Paint tile surface onto canvas context
                            c.set_source_surface(&tile_surface, cr(-offset_pos.x), cr(-offset_pos.y));
                            c.paint();
                        }

                        // Update accuracy as it's relatively cheap to compute it here
                        let view_se_pos = center_pos + Vector::new(vw / 2.0, vh / 2.0);
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
                    let margin = 2.0;
                    let mut ty = -margin;
                    for float_text in self.float_texts_se.borrow_mut().iter_mut() {
                        // Draw the text
                        float_text.pivot = Vector::new(-float_text.margin - margin, ty);
                        float_text.draw(c, Vector::new(vw, vh), |a| { cr(a) });
                        ty -= float_text.font_size + 2.0 * float_text.margin + margin;
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
                debug!("draw time: {:.3} ms", ms); 
            }
        }
    }

    /// Event handler for mouse button press. Either start dragging a map element or scrolling the 
    /// map. This doesn't select map element (to avoid accidental drag instead of scroll).
    fn button_press_event(&self, ev: &gdk::EventButton) {
        let pos = Vector::with_tuple(ev.get_position());
        debug!("button_press_event: {:?}", pos);

        // Check whether the click is on a map element hotspot or not
        if let Some(ref map_win) = self.map_win {
            // The default mode is scrolling
            let mut new_mode = MapCanvasMode::Scrolling;
            self.scroll_history.borrow_mut().clear();
            *self.orig_pos.borrow_mut() = pos;
            *self.orig_center.borrow_mut() = map_win.map_view.borrow().center;
            
            // Match mode
            match *self.mode.borrow() {
                MapCanvasMode::Void => {
                    if false {
                        // Select the map element
                        new_mode = MapCanvasMode::Moving;
                    } else {
                        // Start scrolling
                        new_mode = MapCanvasMode::Scrolling;
                    } 
                }
                _ => {
                    new_mode = MapCanvasMode::Scrolling;
                }
            }
            *self.mode.borrow_mut() = new_mode;
        }
    }

    /// Event handler for mouse button release. Either stop drag or scroll, or select a new 
    /// map element.
    fn button_release_event(&self, ev: &gdk::EventButton) {
        if let Some(ref map_win) = self.map_win {
            let pos = Vector::with_tuple(ev.get_position());
            debug!("button_release_event: {:?} mode={:?}", pos, *self.mode.borrow());
            
            // Either end the drag, scrolling or just keep the selection
            let mut new_mode = MapCanvasMode::Void;
            match *self.mode.borrow() {
                MapCanvasMode::Scrolling => {
                    let mut scroll_history = self.scroll_history.borrow_mut();
                    let history_size = scroll_history.len();
                    
                    // Reference point 0 from the current measurements
                    let pos1 = pos;
                    let time1 = Instant::now();
                    
                    // Reference point 1 from far enough in the scroll history
                    let (mut pos0, mut time0) = (pos1, time1);
                    while duration_to_seconds(&(time1 - time0)) < ANIMATION_SCROLL_HISTORY_MIN_AGE {
                        if let Some((pos, time)) = scroll_history.pop_back() {
                            pos0 = pos; time0 = time; 
                        } else {
                            break;
                        }
                    }
                    
                    if duration_to_seconds(&(time1 - time0)) >= ANIMATION_SCROLL_HISTORY_MIN_AGE {
                        // Calculate a speed vector
                        let mut cc = CoordinateContext::new(map_win.clone(), self);
                        let delta_pos = pos1 - pos0;
                        let delta_time = duration_to_seconds(&(time1 - time0));
                        assert!(delta_time > 0.0);
                        let mut speed_vec = Vector::new(-delta_pos.x as f64 / delta_time, 
                                                    -delta_pos.y as f64 / delta_time );
                        // Speed limit
                        if speed_vec.cathetus() > ANIMATION_SCROLL_SPEED_LIMIT {
                            speed_vec = speed_vec * (ANIMATION_SCROLL_SPEED_LIMIT / speed_vec.cathetus());
                        }

                        // Start animation if speed threshold is reached
                        if speed_vec.cathetus() >= ANIMATION_SCROLL_SPEED_THRESHOLD {
                            new_mode = MapCanvasMode::ScrollAnimation;
                            {
                                let center_pos = cc.loc_to_gpos(map_win.map_view.borrow().center);
                                *self.scroll_speed_vec.borrow_mut() = speed_vec;
                                *self.scroll_prev_time.borrow_mut() = time1;
                                *self.scroll_center_fpos.borrow_mut() = center_pos.into();
                            }

                            // GTK timeout closure
                            let map_win_r = map_win.clone();
                            let decay = ANIMATION_SCROLL_DECAY.powf(1.0 / ANIMATION_FPS);
                            timeout_add((1000.0 / ANIMATION_FPS) as u32, move || {
                                let map_canvas = map_win_r.map_canvas.borrow();
                                let mut cc = CoordinateContext::new(map_win_r.clone(), &map_canvas);
                        
                                // If mode has changed, stop scrolling
                                if *map_canvas.mode.borrow() != MapCanvasMode::ScrollAnimation {
                                    map_win_r.update_map();
                                    return Continue(false);
                                }
                            
                                // Get needed parameters
                                let now = Instant::now();
                                let mut prev_time = map_canvas.scroll_prev_time.borrow_mut();
                                let mut speed_vec = map_canvas.scroll_speed_vec.borrow_mut();
                                let delta_time = duration_to_seconds(&(now - *prev_time));
                                let mut center_fpos = map_canvas.scroll_center_fpos.borrow_mut();
                                
                                // Reduce speed vector
                                let f: f64 = decay;
                                assert!(f < 1.0);
                                *speed_vec = *speed_vec * f.max(0.0);
                                let speed_vec_step = *speed_vec * delta_time;
                                
                                // Stop if the speed is too low
                                if speed_vec_step.cathetus2() < 0.1 {
                                    *map_canvas.mode.borrow_mut() = MapCanvasMode::Void;
                                    map_win_r.update_map();
                                    return Continue(false);
                                }

                                // Move view center
                                *center_fpos = *center_fpos + speed_vec_step;
                                {
                                    let new_center_loc = cc.gpos_to_loc(*center_fpos);
                                    let mut view = map_win_r.map_view.borrow_mut();
                                    view.center = new_center_loc;
                                    view.focus = Some(new_center_loc);
                                }
                                
                                // Request a map update    
                                map_win_r.update_map();
                                
                                // Save time
                                *prev_time = now;
                                
                                Continue(true)
                            });
                        }
                    }
                },
                _ => {
                }
            }
            *self.mode.borrow_mut() = new_mode;
                
            // Open a url if one of the floating texts is clicked.
            let url: RefCell<Option<String>> = RefCell::new(None);
            self.map_floating_text(pos, 
                { |ft| { *url.borrow_mut() = ft.url.clone(); } }, 
                { |ft| { } }) ;
            if let Some(ref url) = url.into_inner() {
                info!("Opening url: {}", url);
                match process::Command::new(&settings_read().browser_command).arg(url).spawn() {
                    Ok(child) => { }
                    Err(e) => {
                        error!("Failed to open the url: {}", e);
                    }
                }
            }
            
            // Request map refresh to get coordinate rounding on
            map_win.update_map();
        } else {
            *self.mode.borrow_mut() = MapCanvasMode::Void;
        }
    }

    /// Event handler for mouse motion. Either drag or scroll.
    fn motion_notify_event(&self, ev: &gdk::EventMotion) {
        //debug!("motion_notify_event: mode={:?}", *self.mode.borrow());
        if let Some(ref map_win) = self.map_win {
            let mut cc = CoordinateContext::new(map_win.clone(), self);
            let update_map = RefCell::new(false);
            let pos = Vector::with_tuple(ev.get_position());
            match *self.mode.borrow() {
                MapCanvasMode::Void => {
                    // Check for possible hover highlight
                    self.map_floating_text(pos, 
                        { |ft| {
                            if !ft.highlight && ft.url.is_some() {
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
                    
                    if !delta_pos.is_zero() {
                        // Move center of the view
                        let orig_center_pos = cc.loc_to_wpos(*self.orig_center.borrow());
                        let new_center = cc.wpos_to_loc(orig_center_pos - delta_pos);
                        map_win.map_view.borrow_mut().center = new_center;

                        // Request a map update                        
                        *update_map.borrow_mut() = true;
                        
                        // Add pos and time to history for inertia
                        let mut scroll_history = self.scroll_history.borrow_mut();
                        if scroll_history.len() >= ANIMATION_SCROLL_HISTORY_LENGTH {
                            scroll_history.pop_front();
                        }
                        scroll_history.push_back((pos, Instant::now()));
                    }
                }
                _ => { }
            }
            
            // Update coordinates label
            {
                let focus = cc.wpos_to_loc(pos);
                map_win.map_view.borrow_mut().focus = Some(focus);
                map_win.update_coordinates_button(Some(focus), *self.accuracy.borrow());
            }
                    

            // Request map update if needed.
            if *update_map.borrow() == true {
                map_win.update_map();
            }
        }
    }

    /// Event handler for mouse wheel.
    fn scroll_event(&self, ev: &gdk::EventScroll) {
        if let Some(ref map_win) = self.map_win {
            let mut cc = CoordinateContext::new(map_win.clone(), self);
            if {
                let mut r = false;
                if let Some(ref widget) = self.widget {
                    // Convert mouse position to focus location
                    let (x, y) = ev.get_position();
                    let mouse_location = cc.wpos_to_loc(Vector::new(x, y));
                
                    let mut map_view = map_win.map_view.borrow_mut();
                    let (dx, dy) = ev.get_delta();
                    //debug!("scroll_event: {},{} delta={},{} mouse_location={:?}", x as i32, y as i32, dx, dy, mouse_location);
                    
                    // Zoom direction
                    if dy < 0.0 {
                        // Max zoom level
                        let max_zoom_level = {
                            if let Some(ref map) = map_win.atlas.borrow().maps.get(&map_view.map_slug) {
                                map.max_zoom_level
                            } else {
                                16
                            }
                        };
                    
                        // Zoom in
                        if map_view.zoom_level < max_zoom_level {
                            map_view.focus = Some(mouse_location);
                            map_view.center = mouse_location.weighted_average(&map_view.center, 0.5);
                            map_view.zoom_level += 1;
                            r = true;
                        }
                    } else if dy > 0.0 {
                        // Zoom out
                        if map_view.zoom_level >= 3 {
                            map_view.focus = Some(mouse_location);
                            map_view.center = mouse_location.weighted_average(&map_view.center, 2.0);
                            map_view.zoom_level -= 1;                
                            r = true;
                        }
                    }
                }
                r
            } {
                // Change state
                *self.mode.borrow_mut() = MapCanvasMode::Void; // TODO: Zoom animation mode
            
                // Let cache know that we changed the level.
                {
                    let map_view = map_win.map_view.borrow();
                    let mut tcache = map_win.tile_cache.borrow_mut();
                    tcache.focus_on_zoom_level(map_view.zoom_level);
                }
            
                // Request map update
                map_win.update_map();
                map_win.update_zoom_level_label(map_win.map_view.borrow().zoom_level);
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------

/// A helper class for coordinate transformations. This class is meant to be in effect 
/// temporarily only as it takes a snapshot of needed fields.
struct CoordinateContext {
    projection: Projection,
    center: Location,
    ppdoe: f64,
    tile_width: i64,
    canvas_width: f64,
    canvas_height: f64,
}

impl CoordinateContext {
    /// Construct a new context using information from map window and drawing area widget.
    pub fn new(map_win: Rc<MapWindow>, map_canvas: &MapCanvas) -> CoordinateContext {
        let map_view = map_win.map_view.borrow();
        if let Some(ref map) = map_win.atlas.borrow().maps.get(&map_view.map_slug) {
            if let Some(tw) = map.tile_width {
                if let Some(ref widget) = map_canvas.widget {
                    return CoordinateContext {
                        projection: map.as_projection(),
                        center: map_view.center,
                        ppdoe: ((tw as u64) << (map_view.zoom_level as u64)) as f64 / 360.0,
                        tile_width: tw as i64,
                        canvas_width: widget.get_allocated_width() as f64,
                        canvas_height: widget.get_allocated_height() as f64,
                    };
                }
            }
        }
        panic!("CoordinateContext creation failed!");
    }
    
    /// Convert location to local pixel position.
    pub fn loc_to_wpos(&mut self, loc: Location) -> Vector {
        let global_nw_pos = self.projection.northwest_global_pixel(self.ppdoe);            
        let center_pos = self.projection.location_to_global_pixel_pos(self.center, self.ppdoe);
        let view_nw_pos = center_pos - Vector::new(self.canvas_width / 2.0, self.canvas_height / 2.0);
        
        let global_pos = self.projection.location_to_global_pixel_pos(loc, self.ppdoe);
        global_pos - view_nw_pos
    }

    /// Convert local pixel position to location.    
    pub fn wpos_to_loc(&mut self, local_pos: Vector) -> Location {
        let global_nw_pos = self.projection.northwest_global_pixel(self.ppdoe);            
        let center_pos = self.projection.location_to_global_pixel_pos(self.center, self.ppdoe);
        let view_nw_pos = center_pos - Vector::new(self.canvas_width / 2.0, self.canvas_height / 2.0);
        
        let global_pos = view_nw_pos + local_pos;            
        self.projection.global_pixel_pos_to_location(global_pos, self.ppdoe)
    }
    
    /// Convert location to global pixel position.
    pub fn loc_to_gpos(&mut self, loc: Location) -> Vector {
        self.projection.location_to_global_pixel_pos(loc, self.ppdoe)
    }
    
    /// Convert global pixel position to location.
    pub fn gpos_to_loc(&mut self, global_pos: Vector) -> Location {
        self.projection.global_pixel_pos_to_location(global_pos, self.ppdoe)
    }
}

// -------------------------------------------------------------------------------------------------

/// Convert duration to milliseconds
fn duration_to_seconds(i: &Duration) -> f64 {
    let secs = i.as_secs() as f64;
    let nsecs = i.subsec_nanos() as f64;
    (secs + 0.000000001 * nsecs)
}

