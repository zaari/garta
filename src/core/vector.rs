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

use std::fmt;
use std::ops::{Add, Sub, Mul, Div};
use std::convert::{From};

use geocoord::geo::{PixelPos};

/// A simple floating point number pair.
#[derive(Copy, Serialize, Deserialize, Clone)]
pub struct Vector {
    pub x: f64,
    pub y: f64,
}

impl Vector {
    /// Constructor from f64 pair.
    #[inline] 
    pub fn new(x: f64, y: f64) -> Vector {
        Vector{x: x, y: y}
    }

    /// Constructor from PixelPos.
    #[inline] 
    pub fn with_pixelpos(pos: PixelPos) -> Vector {
        Vector{x: pos.x as f64, y: pos.y as f64}
    }

    /// Constructor from a f64 tuple.
    #[inline] 
    pub fn with_tuple(xy: (f64, f64)) -> Vector {
        Vector{x: xy.0, y: xy.1}
    }
    
    /// Constructor a zero vector.
    #[inline] 
    pub fn zero() -> Vector {
        Vector{x: 0.0, y: 0.0}
    }

    /// True if exactly {0.0, 0.0}.
    #[inline] 
    pub fn is_zero(&self) -> bool {
        self.x == 0.0 && self.y == 0.0
    }
    
    /// Return an inverted vector.
    #[inline] 
    pub fn invert(&self) -> Vector {
        Vector{x: -self.x, y: -self.y}
    }

    /// Power 2 of length of the cathetus.
    #[inline] 
    pub fn cathetus2(&self) -> f64 {
        self.x * self.x + self.y * self.y
    }
    
    /// Length of the cathetus.
    pub fn cathetus(&self) -> f64 {
        (self.cathetus2() as f64).sqrt()
    }
    
    /// Converts vector to pixel pos by rounding the f64 pair.
    pub fn to_pixelpos(&self) -> PixelPos {
        PixelPos::new(self.x.round() as i64, self.y.round() as i64)
    }
}

impl Sub for Vector {
    type Output = Vector;

    fn sub(self, other: Vector) -> Vector {
        Vector {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl Add for Vector {
    type Output = Vector;

    fn add(self, other: Vector) -> Vector {
        Vector {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl Mul<f64> for Vector {
    type Output = Vector;

    fn mul(self, rhs: f64) -> Vector {
        Vector {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Div<f64> for Vector {
    type Output = Vector;

    fn div(self, rhs: f64) -> Vector {
        Vector {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl PartialEq for Vector {
    fn eq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }
}

impl From<PixelPos> for Vector {
    fn from(pos: PixelPos) -> Vector {
        Vector::with_pixelpos(pos)
    }
}

impl fmt::Debug for Vector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

// ---- tests --------------------------------------------------------------------------------------

#[test]
fn test_vector() {
    // Zero vector
    let v0 = Vector::zero();
    assert!(v0.is_zero());
    
    // Cathetus
    let v1a = Vector::new(4.0, 3.0);
    assert_eq!(v1a.cathetus(), 5.0);
    
    // Equality
    let v1b = Vector::new(4.0, 3.0);
    assert_eq!(v1a, v1b);
    
    // From and to PixelPos
    let pp = PixelPos::new(4, 3);
    let v2 = Vector::with_pixelpos(pp);
    assert_eq!(v2, v1b);
    assert_eq!(v2.to_pixelpos(), pp);
    
    // Multiply
    let v3 = v2 * 2.0;
    assert_eq!(v3, Vector::new(8.0, 6.0));
    
    // Sum
    let v4 = v3 + v2;
    assert_eq!(v4, Vector::new(12.0, 9.0));
    
    // Substract
    let v5 = v2 - v3;
    assert_eq!(v5, Vector::new(-4.0, -3.0));
    
    // Invert
    let v6 = v5.invert();
    assert_eq!(v6, Vector::new(4.0, 3.0));
}

