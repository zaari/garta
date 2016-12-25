
extern crate gtk;
extern crate gio;
extern crate gdk;
extern crate glib;
extern crate cairo;

use self::gtk::prelude::*;

/// Create map canvas widget with all the needed signals connected.
pub fn build_map_widget() -> gtk::DrawingArea {
    // Create the widget
    let canvas = gtk::DrawingArea::new();
    canvas.set_size_request(800, 800);
    canvas.set_visible(true);
    canvas.set_sensitive(true);

    // Signal handler
    canvas.connect_draw( |widget, context| 
                                    { draw(widget, context); Inhibit(true) });                            
    canvas.connect_button_release_event( |widget, event| 
                                    { button_release_event(widget, event); Inhibit(true) } );

    // Return the widget    
    canvas
}

/// Signal handler for draw
fn draw(widget: &gtk::DrawingArea, c: &cairo::Context) {
    let width = 400;
    let height = 400;

    c.save();
    c.move_to(50.0, (width as f64) * 0.5);
    c.set_font_size(18.0);
    c.show_text("This is going to be the map area");
    c.restore();    
}

/// Event handler for button release
fn button_release_event(widget: &gtk::DrawingArea, ev: &gdk::EventButton) {
    let (x, y) = ev.get_position();
    debug!("button_release_event: {}", x);
}

