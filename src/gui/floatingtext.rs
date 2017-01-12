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

//use log::LogLevel::Debug;
use std::fmt;
//use self::gtk::prelude::*;

use geocoord::geo::{PixelPos, PixelBox};

// ---- FloatingText -------------------------------------------------------------------------------

/// Alighment of text box relative to pivot point.
#[derive(Copy, Clone)]
pub enum TextAnchor {
    NorthWest,
    NorthEast,
    SouthEast,
    South,
    SouthWest,
}
    
/// Text on canvas which can be a clickable too.
#[derive(Clone)]
pub struct FloatingText {
    /// Alignment of the text relative to the pivot point.
    pub anchor: TextAnchor,
    
    /// Text pivot point on canvas.
    pub pivot: PixelPos,

    /// The visible text.
    pub text: String,

    /// Optional url.   
    pub url: Option<String>,
    
    /// Normal text color.
    pub fg_rgba: (f64, f64, f64, f64),
    
    /// Background box color.
    pub bg_rgba: (f64, f64, f64, f64),
    
    /// Text highlight color.
    pub highlight_rgba: (f64, f64, f64, f64),
    
    /// Font size.
    pub font_size: i64,
    
    /// Margin between the text and the background rectangle.
    pub margin: i64,

    /// Set by the draw method.
    pub geometry: Option<PixelBox>,
    
    /// Baseline offset from the top of the area.
    pub baseline_offset: Option<i64>,
}

impl FloatingText {
    /// Constructor.
    pub fn new(anchor: TextAnchor, pivot: PixelPos, text: String, url: Option<String>) -> FloatingText {
        FloatingText {
            anchor: anchor,
            pivot: pivot,
            text: text,
            url: url,
            fg_rgba: (0.0, 0.0, 0.0, 1.0),
            bg_rgba: (1.0, 1.0, 1.0, 0.3),
            highlight_rgba: (0.6, 0.8, 1.0, 1.0),
            font_size: 12,
            margin: 3,
            geometry: None,
            baseline_offset: None,
        }
    }

    /// True if the given pos is inside the geometry.
    pub fn contains(&self, pos: PixelPos) -> bool {
        if let Some(geometry) = self.geometry {
            geometry.contains(pos)
        } else {
            false
        }
    }
    
    /// Called by canvas draw method.
    pub fn draw(&mut self, c: &cairo::Context, offset: PixelPos, highlight: bool) {
        // Choose font
        c.select_font_face("sans-serif", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        c.set_font_size(self.font_size as f64);
        
        // Calculate geometry
        let origin = self.pivot + offset;
        let margin = self.margin;
        let font_ext = c.font_extents();
        let ext = c.text_extents(self.text.as_str());
        let (bx, by, tx, ty) = match self.anchor {
            TextAnchor::NorthWest => {
                (0, 0, 0, 0)
            }
            TextAnchor::NorthEast => {
                (0, 0, 0, 0)
            }
            TextAnchor::SouthEast => {
                (origin.x - ext.width as i64 - margin, 
                origin.y - ext.height as i64 - 2 * margin, 
                origin.x - ext.width as i64, 
                origin.y - font_ext.descent as i64 - margin)
            }
            TextAnchor::South => {
                (0, 0, 0, 0)
            }
            TextAnchor::SouthWest => {
                (0, 0, 0, 0)
            }
        };
        let geometry = PixelBox::new(
            PixelPos::new(bx, by), 
            PixelPos::new(bx + ext.width as i64 + 2 * margin, by + ext.height as i64 + 2 * margin));
        self.geometry = Some(geometry - offset);
        self.baseline_offset = Some(margin + font_ext.height as i64);
        
        // Draw a background box
        c.set_source_rgba(self.bg_rgba.0, self.bg_rgba.1, self.bg_rgba.2, self.bg_rgba.3);
        c.rectangle(geometry.x() as f64, geometry.y() as f64, geometry.width() as f64, geometry.height() as f64);
        c.fill();
/* TODO: rounded borders
	    c.new_sub_path ();
	    c.arc (bx + bw - radius, by + radius, radius, -90 * degrees, 0 * degrees);
	    c.arc (bx + bw - radius, by + bh - radius, radius, 0 * degrees, 90 * degrees);
	    c.arc (bx + radius, by + bh - radius, radius, 90 * degrees, 180 * degrees);
	    c.arc (bx + radius, by + radius, radius, 180 * degrees, 270 * degrees);
	    c.close_path ();
*/
        
        // Draw text
        c.set_source_rgba(self.fg_rgba.0, self.fg_rgba.1, self.fg_rgba.2, self.fg_rgba.3);
        c.move_to(tx as f64, ty as f64);
        c.show_text(self.text.as_str());
    }
    
}

impl fmt::Debug for FloatingText {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

