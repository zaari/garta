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

//use self::gtk::prelude::*;
use self::cairo::{Format, ImageSurface};

use geocoord::geo::{Vector};

/// Image surface wrapped with some metadata bundled.
pub struct Sprite {
    /// Surface for the pixel data.
    pub surface: ImageSurface,
    
    /// Surface width.
    pub width: i32,
    
    /// Surface height.
    pub height: i32,

    /// Offset of the surface.
    pub offset: Vector,
    
    /// Zoom level.
    pub zoom_level: u8,
}

impl Sprite {
    /// Constructor with most of the parameters.
    pub fn new(width: i32, height: i32, zoom_level: u8, transparent: bool) -> Sprite {
        Sprite::with_offset(width, height, Vector::zero(), zoom_level, transparent)
    }
    
    /// Constructor with mosto of the parameters, including offset.
    pub fn with_offset(width: i32, height: i32, offset: Vector, zoom_level: u8, transparent: bool) -> Sprite {
        Sprite {
            surface: {
                if transparent {
                    ImageSurface::create(Format::ARgb32, width, height)
                } else {
                    ImageSurface::create(Format::Rgb24, width, height)
                }
            },
            width: width,
            height: height,
            offset: offset,
            zoom_level: zoom_level,
        }
    }

    /// Create a context for the surface
    pub fn to_context(&self) -> cairo::Context {
        cairo::Context::new(&self.surface)
    }
}

impl Clone for Sprite {
    /// Deep clone of the data.
    fn clone(&self) -> Sprite {
        // Create a new surface with matching dimensions
        let new_surface = ImageSurface::create(Format::Rgb24, self.width, self.height);

        // Copy surface contents
        let c = cairo::Context::new(&new_surface);
        c.set_source_surface(&self.surface, 0.0, 0.0);
        c.paint();

        // Return a new sprite
        Sprite {
            surface: new_surface,
            width: self.width,
            height: self.height,
            offset: self.offset,
            zoom_level: self.zoom_level,
        }
    }
}

