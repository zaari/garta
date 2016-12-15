extern crate gtk;
extern crate gio;
extern crate glib;

use std::cell::{RefCell};
use std::rc::{Rc};
use std::result::*;
use self::gtk::prelude::*;
use self::glib::variant::{FromVariant};
use core::geo::{Location};
use core::root::{Project, MapView};
use core::settings::{settings_read};

/// Main window..
pub struct MapWindow {
    project: Box<Project>,
    win: Option<gtk::ApplicationWindow>,
    coordinates_label: Option<gtk::Label>,
    zoom_level_label: Option<gtk::Label>,
    maps_button: Option<gtk::MenuButton>,
    layers_button: Option<gtk::MenuButton>,
    coordinates_button: Option<gtk::MenuButton>,
    
    map_view: Rc<RefCell<MapView>>,
}

impl MapWindow {
    pub fn new(project: Box<Project>, map_view: Rc<RefCell<MapView>>) -> MapWindow {
        MapWindow {
            project: project,
            win: None,
            coordinates_label: None,
            zoom_level_label: None,
            maps_button: None,
            layers_button: None,
            coordinates_button: None,
            map_view: map_view,
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
        if let Some(ref mut win) = self.win {
            // Action for add_attraction
            let add_attraction_action = gio::SimpleAction::new("add_attraction", None);
            add_attraction_action.connect_activate(|_, _| {
                println!("add_attraction");
            });
            win.add_action(&add_attraction_action);
            
            // Action for add_waypoint
            let add_waypoint_action = gio::SimpleAction::new("add_waypoint", None);
            add_waypoint_action.connect_activate(|_, _| {
                println!("add_waypoint");
            });
            win.add_action(&add_waypoint_action);
            
            // Action for add_track
            let add_track_action = gio::SimpleAction::new("add_track", None);
            add_track_action.connect_activate(|_, _| {
                println!("add_track");
            });
            win.add_action(&add_track_action);
            
            // Action for add_route
            let add_route_action = gio::SimpleAction::new("add_route", None);
            add_route_action.connect_activate(|_, _| {
                println!("add_route");
            });
            win.add_action(&add_route_action);
            
            // Action for manage_layers
            let add_layers_action = gio::SimpleAction::new("manage_layers", None);
            add_layers_action.connect_activate(|_, _| {
                println!("manage_layers");
            });
            win.add_action(&add_layers_action);
            
            // Assign components to the struct
            self.coordinates_label = Some(builder.get_object("coordinates_button_label").unwrap());
            self.zoom_level_label = Some(builder.get_object("zoom_level_label").unwrap());
            self.maps_button = Some(builder.get_object("maps_button").unwrap());
            self.layers_button = Some(builder.get_object("layers_button").unwrap());
            self.coordinates_button = Some(builder.get_object("coordinates_button").unwrap());
            
            // Event for window close button
            win.connect_delete_event(|_, _| {
                gtk::main_quit();
                gtk::Inhibit(false)
            });
            
            // Show win and enter GTK main loop
            win.show_all();
        } else {
            return Err("No main_window object found in the resource file.");
        }
        
        // Populate popovers and override default values
        self.populate_maps_button();
        self.populate_layers_button();
        self.populate_coordinates_button();
        self.update_coordinates_button(None);
        self.update_zoom_level_label(0); // TODO
        
        // Start main
        gtk::main();
        Ok(())
    }

    /// Populate (or re-populate) maps button popover.
    pub fn populate_maps_button(&mut self) {
        if let Some(ref button) = self.maps_button {
            if let Some(ref win) = self.win {
                let menu_model = gio::Menu::new();
                
                // Choose map action
                let action = gio::SimpleAction::new_stateful(
                                "choose_map", 
                                Some(&glib::VariantType::new("s").unwrap()),
                                &"default".to_string().to_variant()
                                );
                action.connect_activate( |action, map_slug_variant| {
                    if let Some(ref var) = *map_slug_variant {
                        if let Some(map_slug) = var.get_str() {
                            println!("choose_map action invoked {}!", map_slug);
                            action.set_state(var);
                            
                            // TODO: change map on the view
                        }
                    }
                });
                win.add_action(&action);

                // Fill in and add the maps section
                for map in &settings_read().maps {
                    if !map.transparent {
                        let item = gio::MenuItem::new(
                            Some(map.name.as_str()), 
                            Some(format!("win.choose_map('{}')", map.slug).as_str()));
                        menu_model.append_item(&item);
                    }
                }
                action.set_state(&"map1".to_string().to_variant()); // TODO

                // Set menu model                
                button.set_menu_model(Some(&menu_model));
            }
        }        
    }

    /// Populate (or re-populate) layers button popover.
    pub fn populate_layers_button(&mut self) {
        if let Some(ref button) = self.layers_button {
            if let Some(ref win) = self.win {
                let menu_model = gio::Menu::new();
                
                // Layers secion
                {
                    let section = gio::Menu::new();
                    
                    // Iterate layers                
                    for layer_rr in &self.project.layers {
                        let initial_state = true; //FIXME: self.map_view.visible_layers.contains(layer_rr);
                        let layer = layer_rr.borrow();
                        
                        // Only transparent layers are toggleable
                        if !layer.backdrop {
                            // Choose map action
                            let action = gio::SimpleAction::new_stateful(
                                            format!("toggle_layer_{}", layer.slug).as_ref(), 
                                            None,
                                            &initial_state.to_variant()
                                            );
                            action.connect_change_state( |action, value| {
                                let layer_slug = &action.get_name().unwrap()["toggle_layer_".len()..];
                                if let Some(ref var) = *value {
                                    println!("toggle_layer({}) action invoked {}!", layer_slug, var);
                                    if let Some(var_bool) = bool::from_variant(var) {
                                        action.set_state(var);
                                        
                                        // TODO: change layer on the view
                                    }
                                }
                            });
                            win.add_action(&action);

                            let item = gio::MenuItem::new(
                                Some(layer.name.as_str()), 
                                Some(format!("win.toggle_layer_{}", layer.slug).as_str()));
                            section.append_item(&item);
                        }
                    }
                    menu_model.append_section(None, &section);
                }

                // Ops section
                {
                    let section = gio::Menu::new(); 
                
                    // Add manage item
                    section.append_item(&gio::MenuItem::new(
                        Some("Manage..."), 
                        Some("win.manage_layers")));
                    menu_model.append_section(None, &section);
                }

                // Set menu model                
                button.set_menu_model(Some(&menu_model));
            }
        }        
    }

    /// Populate coordinates button popover.
    pub fn populate_coordinates_button(&mut self) {
        if let Some(ref button) = self.coordinates_button {
            if let Some(ref win) = self.win {
                let menu_model = gio::Menu::new();
                
                // Choose coordinates action
                let action = gio::SimpleAction::new_stateful(
                                "choose_coordinates", 
                                Some(&glib::VariantType::new("s").unwrap()),
                                &"default".to_string().to_variant(),
                                );
                action.connect_activate( |action, coordinates_slug_variant| {
                    if let Some(ref var) = *coordinates_slug_variant {
                        if let Some(coordinates_slug) = var.get_str() {
                            println!("choose_cordinates action invoked {}!", coordinates_slug);
                            action.set_state(var);
                            
                            // TODO: change coordinates on the view
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
                action.set_state(&"dm".to_string().to_variant()); // TODO

                // Set menu model                
                button.set_menu_model(Some(&menu_model));
            }
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
}

