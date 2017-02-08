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
use std::collections::linked_list::LinkedList;
use self::gtk::prelude::*;
use self::glib::variant::{FromVariant};
use geocoord::geo::{Location};
use core::atlas::{Atlas, MapView};
use core::id::{UniqueId};
use core::tiles::{TileCache, TileObserver, TileRequest};
use core::settings::{settings_read};
use gui::mapcanvas::{MapCanvas};
//use core::settings::{settings_read, settings_write};

/// Main window.
pub struct MapWindow {
    /// Core model elements
    pub atlas: RefCell<Atlas>,
    
    /// Meta data about canvas
    pub map_view: RefCell<MapView>,
    
    /// Map canvas meta element.
    pub map_canvas: RefCell<MapCanvas>,
    
    /// Tile cache
    pub tile_cache: Rc<RefCell<TileCache>>,
    
    /// A seprate struct for GTK widgets to reduce borrow calls
    widgets: RefCell<MapWindowWidgets>,
}

/// As gtk-rs elements are just wrappers pointing the native widgets they survive from cloning.
#[derive(Clone)]
struct MapWindowWidgets {
    win:                    Option<gtk::ApplicationWindow>,
    coordinates_label:      Option<gtk::Label>,
    zoom_level_label:       Option<gtk::Label>,
    maps_button:            Option<gtk::MenuButton>,
    maps_button_label:      Option<gtk::Label>,
    layers_button:          Option<gtk::MenuButton>,
    layers_button_label:    Option<gtk::Label>,
    coordinates_button:     Option<gtk::MenuButton>,
}

impl MapWindowWidgets {
    fn new() -> MapWindowWidgets {
        MapWindowWidgets {
            win: None,
            coordinates_label: None,
            zoom_level_label: None,
            maps_button: None,
            maps_button_label: None,
            layers_button: None,
            layers_button_label: None,
            coordinates_button: None,
        }
    }
}

impl MapWindow {
    pub fn new_rc(atlas: RefCell<Atlas>, map_view: RefCell<MapView>, tile_cache: Rc<RefCell<TileCache>>) -> Rc<MapWindow> {
        Rc::new(MapWindow {
            atlas: atlas,
            map_canvas: RefCell::new(MapCanvas::new()),
            map_view: map_view,
            tile_cache: tile_cache,
            widgets: RefCell::new(MapWindowWidgets::new()),
        })
    }
    
    /// Build main window and run GTK main.
    pub fn init(&self, self_rc: Rc<Self>) -> Result<(), &'static str> {
        // Initialize GTK
        if gtk::init().is_err() {
            return Err("Failed to initialize GTK.");
        }

        // Initialize map canvas        
        self.map_canvas.borrow_mut().init(self_rc.clone());

        // Create widgets    
        {    
            // Get widgets struct
            let mut widgets = self.widgets.borrow_mut();

            // Load resources from a glade file
            let builder = gtk::Builder::new_from_file(settings_read().ui_directory_for("main-window.ui"));
            let win_o: Option<gtk::ApplicationWindow> = builder.get_object("main_window");
            if let Some(ref win) = win_o {
                // Action for add_attraction
                let add_attraction_action = gio::SimpleAction::new("add_attraction", None);
                add_attraction_action.connect_activate(|_, _| {
                    debug!("add_attraction");
                });
                win.add_action(&add_attraction_action);
                
                // Action for add_waypoint
                let add_waypoint_action = gio::SimpleAction::new("add_waypoint", None);
                add_waypoint_action.connect_activate(|_, _| {
                    debug!("add_waypoint");
                });
                win.add_action(&add_waypoint_action);
                
                // Action for add_track
                let add_track_action = gio::SimpleAction::new("add_track", None);
                add_track_action.connect_activate(|_, _| {
                    debug!("add_track");
                });
                win.add_action(&add_track_action);
                
                // Action for add_route
                let add_route_action = gio::SimpleAction::new("add_route", None);
                add_route_action.connect_activate(|_, _| {
                    debug!("add_route");
                });
                win.add_action(&add_route_action);
                
                // Action for manage_layers
                let add_layers_action = gio::SimpleAction::new("manage_layers", None);
                add_layers_action.connect_activate(|_, _| {
                    debug!("manage_layers");
                });
                win.add_action(&add_layers_action);
                
                // Event for window close button
                {
                    let self_rc2 = self_rc.clone();
                    win.connect_delete_event(move |_, _| {
                        // Save window geometry
                        if let Some(ref win) = self_rc2.widgets.borrow().win {
                            let mut view = self_rc2.map_view.borrow_mut();
                            view.window_size = Some(win.get_size());
                            view.window_position = Some(win.get_position());
                        }
                    
                        // Quit GTK
                        gtk::main_quit();
                        gtk::Inhibit(false)
                    });
                }

                // Add map widget
                let map_box: gtk::Box = builder.get_object("map_box").unwrap();
                map_box.add(self.map_canvas.borrow().widget.as_ref().unwrap());
                map_box.set_child_packing(self.map_canvas.borrow().widget.as_ref().unwrap(), 
                    true, true, 0, gtk::PackType::End);

                // Set window position and size
                {
                    let view = self.map_view.borrow_mut();
                    if let Some(win_pos) = view.window_position {
                        win.move_(win_pos.0, win_pos.1);
                    }
                    if let Some(win_size) = view.window_size {
                        win.set_default_size(win_size.0, win_size.1);
                    }
                }
                
                // Show win and enter GTK main loop
                win.show_all();
            } else {
                return Err("No main_window object found in the resource file.");
            }

            // Assign components to the struct
            widgets.win = win_o;
            widgets.coordinates_label = Some(builder.get_object("coordinates_button_label").unwrap());
            widgets.zoom_level_label = Some(builder.get_object("zoom_level_label").unwrap());
            widgets.maps_button = Some(builder.get_object("maps_button").unwrap());
            widgets.maps_button_label = Some(builder.get_object("maps_button_label").unwrap());
            widgets.layers_button = Some(builder.get_object("layers_button").unwrap());
            widgets.layers_button_label = Some(builder.get_object("layers_button_label").unwrap());
            widgets.coordinates_button = Some(builder.get_object("coordinates_button").unwrap());
            
            // Hide unfinished items
            { let b: gtk::MenuButton = builder.get_object("add_button").unwrap(); b.set_visible(false); }
            { let b: gtk::MenuButton = builder.get_object("list_button").unwrap(); b.set_visible(false); }
            { let b: gtk::MenuButton = builder.get_object("menu_button").unwrap(); b.set_visible(false); }
            { let b: gtk::Widget = builder.get_object("layers_button").unwrap(); b.set_visible(false); }
        }
        
        // Populate popovers and override default values
        let zoom_level = self.map_view.borrow().zoom_level;
        self.populate_maps_button(&self_rc);
        self.populate_layers_button(&self_rc);
        self.populate_coordinates_button(&self_rc);
        self.update_maps_button();
        self.update_layers_button();
        self.update_coordinates_button(None, None);
        self.update_zoom_level_label(zoom_level);
        self.update_map();

        Ok(())
    }

    /// Populate (or re-populate) maps button popover.
    pub fn populate_maps_button(&self, self_rc: &Rc<Self>) {
        let widgets = self.widgets.borrow_mut();
        
        if let Some(ref button) = widgets.maps_button {
            if let Some(ref win) = widgets.win {
                // TODO: clean the old map actions from win
            
                let menu_model = gio::Menu::new();
                
                // Get backdrop map id
                let backdrop_map_slug = {
                    let view = self.map_view.borrow_mut();
                    view.map_slug.clone()
                };

                // Simple Action for map menu button
                let action = gio::SimpleAction::new_stateful(
                                "choose_map", 
                                Some(&glib::VariantType::new("s").unwrap()),
                                &backdrop_map_slug.to_string().to_variant()
                                );
                {
                    let self_rc = self_rc.clone();
                    action.connect_activate( move |action, map_slug_variant| {
                        if let Some(ref var) = *map_slug_variant {
                            if let Some(map_slug) = var.get_str() {
                                debug!("choose_map action invoked {}!", map_slug);
                                action.set_state(var);
                                
                                // Change map id on the view
                                {
                                    let mut view = self_rc.map_view.borrow_mut();
                                    view.map_slug = map_slug.to_string();
                                    
                                    // We don't want to focus on the lower left corner on redraw.
                                    view.focus = None;
                                }
                                
                                // Refresh the map element and the button
                                {
                                    self_rc.map_canvas.borrow_mut().update_map_meta();
                                    self_rc.update_map();
                                    self_rc.update_maps_button();
                                }
                            }
                        }
                    });
                }
                win.add_action(&action);

                // Fill in and add the maps section
                let atlas = self.atlas.borrow();
                for (_, map) in &atlas.maps {
                    if !map.transparent {
                        let item = gio::MenuItem::new(
                            Some(map.name.as_str()), 
                            Some(format!("win.choose_map('{}')", map.slug).as_str()));
                        menu_model.append_item(&item);
                    }
                }

                // Set menu model                
                button.set_menu_model(Some(&menu_model));

                // Update canvas copyrights
                self_rc.map_canvas.borrow_mut().update_map_meta();
            }
        }        
    }

    /// Populate (or re-populate) layers button popover.
    pub fn populate_layers_button(&self, self_rc: &Rc<Self>) {
        let widgets = self.widgets.borrow_mut();
        
        if let Some(ref button) = widgets.layers_button { if let Some(ref win) = widgets.win {
            // TODO: clean the old layer actions from win
            
            let menu_model = gio::Menu::new();
            
            // Layers section
            let atlas = self.atlas.borrow();
            let layers_section = gio::Menu::new();
            for layer in atlas.layers.values().rev() {
                let layer_id = &layer.id();
                let initial_state = self.map_view.borrow().visible_layer_ids
                    .iter()
                    .filter(|&la_id| la_id == layer_id)
                    .count() > 0;
                
                // Only transparent layers are toggleable
                if layer.backdrop() { continue; }
                
                // Choose map action
                let action = gio::SimpleAction::new_stateful(
                                format!("toggle_layer_{}", layer_id).as_ref(), 
                                None,
                                &initial_state.to_variant()
                                );

                // Action closure
                {
                    let self_rc = self_rc.clone();
                    action.connect_change_state( move |action, value| {
                        let selected_layer_id_str = &action.get_name().unwrap()["toggle_layer_".len()..];
                        let selected_layer_id = selected_layer_id_str.parse::<UniqueId>().unwrap();
                        if let Some(ref var) = *value {
                            debug!("toggle_layer({}) action invoked {}!", selected_layer_id, var);
                            if let Some(var_bool) = bool::from_variant(var) {
                                action.set_state(var);

                                // Change layers list of map view
                                {
                                    let atlas = self_rc.atlas.borrow_mut();
                                    let mut view = self_rc.map_view.borrow_mut();
                                    
                                    for (layer_id, layer) in &atlas.layers {
                                        if *layer_id == selected_layer_id {
                                            if var_bool {
                                                // Add layer
                                                view.visible_layer_ids.push_back(*layer_id);
                                            } else {
                                                // Drop layer
                                                view.visible_layer_ids = view.visible_layer_ids
                                                    .iter()
                                                    .filter(|&la_id| *la_id != *layer_id)
                                                    .cloned()
                                                    .collect::<LinkedList<UniqueId>>();
                                            }
                                            break;
                                        }
                                    }
                                }
                                
                                // Change layer label
                                self_rc.update_layers_button();
                                
                                // Refresh the map element
                                self_rc.update_map();
                            }
                        }
                    });
                }
                win.add_action(&action);

                // Menu item
                let item = gio::MenuItem::new(
                    Some(layer.name.as_str()), 
                    Some(format!("win.toggle_layer_{}", layer.id()).as_str()));
                layers_section.append_item(&item);
            }
            menu_model.append_section(None, &layers_section);

            // Add manage item to ops section
            let ops_section = gio::Menu::new(); 
            ops_section.append_item(&gio::MenuItem::new(
                Some("Manage..."), 
                Some("win.manage_layers")));
            menu_model.append_section(None, &ops_section);

            // Set menu model                
            button.set_menu_model(Some(&menu_model));
        } }   
    }

    /// Populate coordinates button popover.
    pub fn populate_coordinates_button(&self, self_rc: &Rc<Self>) {
        let widgets = self.widgets.borrow_mut();
        
        if let Some(ref button) = widgets.coordinates_button {
            if let Some(ref win) = widgets.win {
                let menu_model = gio::Menu::new();

                // Get backdrop map id
                let coordinates_format = {
                    let view = self.map_view.borrow_mut();
                    view.coordinates_format.clone()
                };
                
                // Choose coordinates action
                let action = gio::SimpleAction::new_stateful(
                                "choose_coordinates", 
                                Some(&glib::VariantType::new("s").unwrap()),
                                &coordinates_format.to_variant(),
                                );
                                
                {
                    let self_rc = self_rc.clone();
                    action.connect_activate( move |action, coordinates_slug_variant| {
                        if let Some(ref var) = *coordinates_slug_variant {
                            if let Some(coordinates_format) = var.get_str() {
                                debug!("choose_cordinates action invoked {}!", coordinates_format);
                                action.set_state(var);
                                
                                // Change coordinates format on the view
                                {
                                    let mut view = self_rc.map_view.borrow_mut();
                                    view.coordinates_format = coordinates_format.into();
                                }
                                
                                // Refresh the button
                                let focus = self_rc.map_view.borrow().focus;
                                self_rc.update_coordinates_button(focus, None); // TODO: accuracy
                            }
                        }
                    });
                }
                win.add_action(&action);

                // Fill in and add the coordinates section
                menu_model.append_item(&gio::MenuItem::new(
                    Some("DDD°mm'SS.ss\""), Some("win.choose_coordinates('dms')")));
                menu_model.append_item(&gio::MenuItem::new(
                    Some("DDD°mm.mmm°"), Some("win.choose_coordinates('dm')")));
                menu_model.append_item(&gio::MenuItem::new(
                    Some("DDD.ddddd°"), Some("win.choose_coordinates('d')")));
                menu_model.append_item(&gio::MenuItem::new(
                    Some("-DDD.ddddd"), Some("win.choose_coordinates('-d')")));

                // Set menu model                
                button.set_menu_model(Some(&menu_model));
            }
        }
    }

    /// Update maps button label.
    pub fn update_maps_button(&self) {
        let widgets = self.widgets.borrow_mut();
        
        if let Some(ref label) = widgets.maps_button_label {
            // Get map name for the backdrop layer
            let backdrop_map_name = {
                let atlas = self.atlas.borrow();
                let view = self.map_view.borrow();
                debug!("update_maps_button {}", view.map_slug);
                if !view.map_slug.is_empty() {
                    let map = atlas.maps.get(&view.map_slug).unwrap();
                    map.name.clone()
                } else {
                    "Maps".into()
                }
            };

            // Set button label
            label.set_text(backdrop_map_name.as_str());
        }
    }

    /// Update layers button label.
    pub fn update_layers_button(&self) {
        let widgets = self.widgets.borrow_mut();
        
        if let Some(ref label) = widgets.layers_button_label {
            let atlas = self.atlas.borrow();
            let map_view = self.map_view.borrow();
            let m = map_view.visible_layer_ids.len();
            let n = atlas.layers.len();
            let b = { if atlas.backdrop_layer_id().is_some() { 1 } else { 0 } };
            label.set_text(format!("Layers {}/{}", m, n - b).as_str());
        }
    }

    /// Update coordinates in the coordinates button.
    pub fn update_coordinates_button(&self, location: Option<Location>, accuracy: Option<f64>) {
        let widgets = self.widgets.borrow_mut();
        
        if let Some(ref label) = widgets.coordinates_label {
            if let Some(ref loc) = location {
                let map_view = self.map_view.borrow();
                let format = map_view.coordinates_format.clone();
                label.set_text(loc.format(&format, accuracy).as_ref());
            } else {
                label.set_text("--°N --°E");
            }
        }
    }

    /// Update zoom level label.
    pub fn update_zoom_level_label(&self, zoom_level: u8) {
        let widgets = self.widgets.borrow_mut();
        
        if let Some(ref label) = widgets.zoom_level_label {
            if zoom_level > 0 {
                label.set_text(format!("L{}", zoom_level).as_ref());
            } else {
                label.set_text("--");
            }
        }
    }
    
    /// Full refresh of the map canvas.
    pub fn update_map(&self) {
        let widgets = self.widgets.borrow_mut();
        
        if let Some(ref mapcanvas) = self.map_canvas.borrow().widget {
            debug!("queue_draw");
            mapcanvas.queue_draw();
        } else {
            warn!("No canvas, no queue_draw");
        }
    }
}

impl TileObserver for MapWindow {
    fn tile_loaded(&self, treq: &TileRequest) {
        //debug!("tile_loaded: {:?}", treq);        
        let widgets = self.widgets.borrow_mut();
        
        if self.map_view.borrow().zoom_level == treq.z {
            if let Some(ref mapcanvas) = self.map_canvas.borrow().widget {
                mapcanvas.queue_draw(); // TODO: only partial redraw
            } else {
                warn!("No canvas, no redraw");
            }
        }
    }
}

