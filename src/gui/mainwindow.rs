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
extern crate glib;

use std::cell::{RefCell};
use std::rc::{Rc};
use std::result::*;
use std::collections::linked_list::LinkedList;
use self::gtk::prelude::*;
use self::glib::variant::{FromVariant};
use geocoord::geo::{Location};
use core::root::{Atlas, MapView};
use core::id::{UniqueId, NONE};
use core::tiles::{TileCache};
use gui::mapcanvas::{build_map_canvas};
//use core::settings::{settings_read, settings_write};

/// Main window.
pub struct MapWindow {
    pub atlas: Rc<RefCell<Atlas>>,
    pub map_view: Rc<RefCell<MapView>>,
    pub tile_cache: Rc<RefCell<TileCache>>,

    // the following ones "survive" from cloning without reference counting
    win:                    Option<gtk::ApplicationWindow>,
    coordinates_label:      Option<gtk::Label>,
    zoom_level_label:       Option<gtk::Label>,
    maps_button:            Option<gtk::MenuButton>,
    maps_button_label:      Option<gtk::Label>,
    layers_button:          Option<gtk::MenuButton>,
    layers_button_label:    Option<gtk::Label>,
    coordinates_button:     Option<gtk::MenuButton>,    
}

impl Clone for MapWindow {
    fn clone(&self) -> MapWindow {
        MapWindow {
            atlas: self.atlas.clone(),
            map_view: self.map_view.clone(),
            tile_cache : self.tile_cache.clone(),
            win: self.win.clone(),
            coordinates_label: self.coordinates_label.clone(),
            zoom_level_label: self.zoom_level_label.clone(),
            maps_button: self.maps_button.clone(),
            maps_button_label: self.maps_button_label.clone(),
            layers_button: self.layers_button.clone(),
            layers_button_label: self.layers_button_label.clone(),
            coordinates_button: self.coordinates_button.clone(),
        }
    }
}

impl MapWindow {
    pub fn new(atlas: Rc<RefCell<Atlas>>, map_view: Rc<RefCell<MapView>>, tile_cache: Rc<RefCell<TileCache>>) -> MapWindow {
        MapWindow {
            atlas: atlas,
            map_view: map_view,
            tile_cache: tile_cache,
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

    /// Build main window and run GTK main.
    pub fn run(&mut self) -> Result<(), &'static str> {
        // Initialize GTK
        if gtk::init().is_err() {
            return Err("Failed to initialize GTK.");
        }

        // Load resources from a glade file
        let builder = gtk::Builder::new_from_file("ui/main-window.ui");
        self.win = builder.get_object("main_window");
        let self_clone = self.clone();
        if let Some(ref mut win) = self.win {
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
            
            // Assign components to the struct
            self.coordinates_label = Some(builder.get_object("coordinates_button_label").unwrap());
            self.zoom_level_label = Some(builder.get_object("zoom_level_label").unwrap());
            self.maps_button = Some(builder.get_object("maps_button").unwrap());
            self.maps_button_label = Some(builder.get_object("maps_button_label").unwrap());
            self.layers_button = Some(builder.get_object("layers_button").unwrap());
            self.layers_button_label = Some(builder.get_object("layers_button_label").unwrap());
            self.coordinates_button = Some(builder.get_object("coordinates_button").unwrap());
            
            // Event for window close button
            win.connect_delete_event(|_, _| {
                gtk::main_quit();
                gtk::Inhibit(false)
            });

            // Add map widget
            let map_box: gtk::Container = builder.get_object("map_box").unwrap();
            map_box.add(&build_map_canvas(&self_clone));
            
            // Show win and enter GTK main loop
            win.show_all();
        } else {
            return Err("No main_window object found in the resource file.");
        }
        
        // Populate popovers and override default values
        let zoom_level = self.map_view.borrow().zoom_level;
        self.populate_maps_button();
        self.populate_layers_button();
        self.populate_coordinates_button();
        self.update_maps_button();
        self.update_layers_button();
        self.update_coordinates_button(None);
        self.update_zoom_level_label(zoom_level);
        self.update_map();
        
        // Start main
        gtk::main();
        Ok(())
    }

    /// Populate (or re-populate) maps button popover.
    pub fn populate_maps_button(&mut self) {
        if let Some(ref button) = self.maps_button {
            if let Some(ref win) = self.win {
                // TODO: clean the old map actions from win
            
                let menu_model = gio::Menu::new();
                
                // Get backdrop map id
                let backdrop_map_id = {
                    let view = self.map_view.borrow_mut();
                    view.map_id
                };

                // Simple Action for map menu button
                let action = gio::SimpleAction::new_stateful(
                                "choose_map", 
                                Some(&glib::VariantType::new("s").unwrap()),
                                &backdrop_map_id.to_string().to_variant()
                                );
                let map_win = RefCell::new(self.clone());
                action.connect_activate( move |action, map_id_variant| {
                    if let Some(ref var) = *map_id_variant {
                        if let Some(map_id_str) = var.get_str() {
                            let map_id = map_id_str.parse::<UniqueId>().unwrap();
                            debug!("choose_map action invoked {}!", map_id);
                            action.set_state(var);
                            
                            // Change map id on the view
                            let mut win = map_win.borrow_mut();
                            {
                                let mut view = win.map_view.borrow_mut();
                                view.map_id = map_id;
                            }
                            
                            // Refresh the map element and the button
                            win.update_map();
                            win.update_maps_button();
                        }
                    }
                });
                win.add_action(&action);

                // Fill in and add the maps section
                let atlas = self.atlas.borrow();
                for (_, map) in &atlas.maps {
                    if !map.transparent {
                        let item = gio::MenuItem::new(
                            Some(map.name.as_str()), 
                            Some(format!("win.choose_map('{}')", map.id()).as_str()));
                        menu_model.append_item(&item);
                    }
                }

                // Set menu model                
                button.set_menu_model(Some(&menu_model));
            }
        }        
    }

    /// Populate (or re-populate) layers button popover.
    pub fn populate_layers_button(&mut self) {
        if let Some(ref button) = self.layers_button { if let Some(ref win) = self.win {
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
                let map_win_r = RefCell::new(self.clone());
                action.connect_change_state( move |action, value| {
                    let selected_layer_id_str = &action.get_name().unwrap()["toggle_layer_".len()..];
                    let selected_layer_id = selected_layer_id_str.parse::<UniqueId>().unwrap();
                    if let Some(ref var) = *value {
                        debug!("toggle_layer({}) action invoked {}!", selected_layer_id, var);
                        if let Some(var_bool) = bool::from_variant(var) {
                            action.set_state(var);

                            // Change layers list of map view
                            let mut win = map_win_r.borrow_mut();
                            {
                                let atlas_rr = win.atlas.clone();
                                let atlas = atlas_rr.borrow_mut();
                                let mut view = win.map_view.borrow_mut();
                                
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
                            win.update_layers_button();
                            
                            // Refresh the map element
                            win.update_map();
                        }
                    }
                });
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
    pub fn populate_coordinates_button(&mut self) {
        if let Some(ref button) = self.coordinates_button {
            if let Some(ref win) = self.win {
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
                let map_win_r = RefCell::new(self.clone());
                action.connect_activate( move |action, coordinates_slug_variant| {
                    if let Some(ref var) = *coordinates_slug_variant {
                        if let Some(coordinates_format) = var.get_str() {
                            debug!("choose_cordinates action invoked {}!", coordinates_format);
                            action.set_state(var);
                            
                            // Change coordinates format on the view
                            let mut win = map_win_r.borrow_mut();
                            {
                                let mut view = win.map_view.borrow_mut();
                                view.coordinates_format = coordinates_format.into();
                            }
                            
                            // Refresh the button
                            win.update_coordinates_button(None);
                        }
                    }
                });
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
    pub fn update_maps_button(&mut self) {
        if let Some(ref label) = self.maps_button_label {
            // Get map name for the backdrop layer
            let backdrop_map_name = {
                let atlas = self.atlas.borrow();
                let view = self.map_view.borrow();
                debug!("update_maps_button_ {}", view.map_id);
                if view.map_id != NONE {
                    let map = atlas.maps.get(&view.map_id).unwrap();
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
    pub fn update_layers_button(&mut self) {
        if let Some(ref label) = self.layers_button_label {
            let atlas = self.atlas.borrow();
            let map_view = self.map_view.borrow();
            let m = map_view.visible_layer_ids.len();
            let n = atlas.layers.len();
            let b = { if atlas.backdrop_layer_id().is_some() { 1 } else { 0 } };
            label.set_text(format!("Layers {}/{}", m, n - b).as_str());
        }
    }

    /// Update coordinates in the coordinates button.
    pub fn update_coordinates_button(&mut self, location: Option<Location>) {
        if let Some(ref label) = self.coordinates_label {
            if let Some(ref loc) = location {
                label.set_text(loc.to_string().as_ref());
            } else {
                label.set_text("--°N --°E");
            }
        }
    }

    /// Update zoom level label.
    pub fn update_zoom_level_label(&mut self, zoom_level: u8) {
        if let Some(ref label) = self.zoom_level_label {
            if zoom_level > 0 {
                label.set_text(format!("L{}", zoom_level).as_ref());
            } else {
                label.set_text("--");
            }
        }
    }
    
    /// Refresh the map element.
    pub fn update_map(&mut self) {
        // TODO
    }
}

