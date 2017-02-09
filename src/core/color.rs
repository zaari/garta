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

extern crate serde_json;

/// RGBA color.
#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Color {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

impl Color {
    /// Construct a new color with alpha.
    pub fn new(r: f64, g: f64, b: f64, a: f64) -> Color {
        Color {
            red: r,
            green: g,
            blue: b,
            alpha: a,
        }
    }
    
    /// Construct a new color without alpha.
    pub fn opaque(r: f64, g: f64, b: f64) -> Color {
        Color {
            red: r,
            green: g,
            blue: b,
            alpha: 0.0,
        }
    }

    /// Constructor for RGB tuple.
    pub fn with_tuple(rgb: (f64, f64, f64)) -> Color {
        Color {
            red: rgb.0,
            green: rgb.1,
            blue: rgb.2,
            alpha: 0.0,
        }
    }
    
    /// Construct a black color.
    pub fn black() -> Color {
        Color::opaque(0.0, 0.0, 0.0)
    }
    
    /// Construct a black color.
    pub fn white() -> Color {
        Color::opaque(1.0, 1.0, 1.0)
    }
    
    /// Return color with alpha set to given value.
    pub fn alpha(&self, alpha: f64) -> Color {
        Color {
            red: self.red,
            green: self.green,
            blue: self.blue,
            alpha: alpha,
        }
    }
    
    /// Weighted average of two colors. 0.0 results self, 1.0 results the other and 0.5 average.
    #[inline]
    pub fn blend_with(&self, other: Color, weight: f64) -> Color {
        let w1 = 1.0 - weight;
        Color {
            red:   w1 * self.red   + weight * other.red,
            green: w1 * self.green + weight * other.green,
            blue:  w1 * self.blue  + weight * other.blue,
            alpha: w1 * self.alpha + weight * other.alpha,
        }
    }
    
    /// Square of distance to the other color. Alpha is ignored.
    #[inline]
    pub fn distance_to(&self, other: Color) -> f64 {
        let sq = |x| { x * x};
        
        (sq(self.red - other.red) + sq(self.green - other.green) + sq(self.blue - other.blue)).sqrt()
    }
}

// ---- tests --------------------------------------------------------------------------------------

#[test]
fn test_color() {
    let black = Color::black();
    let white = Color::white();
    
    // Distance between black and white
    assert_eq!(black.distance_to(white), 3.0f64.sqrt());
    
    // Mix black and white to get gray    
    let gray = black.blend_with(white, 0.5);
    assert_eq!(gray.red, 0.5);
    assert_eq!(gray.green, 0.5);
    assert_eq!(gray.blue, 0.5);
    
    // Mix black and with with weights 0.0 and 1.0
    assert_eq!(black.blend_with(white, 0.0), black);
    assert_eq!(black.blend_with(white, 1.0), white);
    
    // Alpha
    let orange = Color::opaque(1.0, 0.6, 0.0);
    let orange_a = orange.alpha(0.8);
    assert_eq!(orange_a.red, 1.0);
    assert_eq!(orange_a.green, 0.6);
    assert_eq!(orange_a.blue, 0.0);
    assert_eq!(orange_a.alpha, 0.8);
    
    // PartialEq
    assert_eq!(orange, orange_a.alpha(0.0));
}

