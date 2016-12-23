
extern crate gtk;
extern crate gio;
extern crate glib;

use self::gtk::prelude::*;

/// Create map canvas widget with all the needed signals connected.
pub fn build_map_widget() -> gtk::DrawingArea {
    // Create the widget
    let canvas = gtk::DrawingArea::new();
    canvas.set_size_request(800, 800);
    canvas.set_visible(true);

    // TODO: connect

    // Return the widget    
    canvas
}

