// Garta - GPX editor and analyser
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

extern crate regex;

use std::f64::consts;
use std::fmt;
use std::cmp::*;
use self::regex::Regex;

// ---- PixelPos -----------------------------------------------------------------------------------

/// View position in pixels.
#[derive(Copy, Clone)]
pub struct PixelPos {
    x: i32,
    y: i32,
}

impl PixelPos {
    /// Constructor.
    fn new(x: i32, y: i32) -> PixelPos {
        PixelPos{x: x, y: y}
    }
}

impl fmt::Display for PixelPos {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({},{})", self.x, self.y)
    }
}

// ---- Location -----------------------------------------------------------------------------------

/// Map coordinates in degrees.
#[derive(Copy, Clone)]
pub struct Location {
    pub lat: f64, // south-north
    pub lon: f64, // west-east
}

impl Location {
    // Constructor
    pub fn new(lat: f64, lon: f64) -> Location {
        Location{lat: lat, lon: lon}
    }

    pub fn from_str(lat_lon_str: &str) -> Result<Location, String> {
        Location::from_string(lat_lon_str.to_string())
    }
    
    pub fn from_string(lat_lon_str: String) -> Result<Location, String> {
        let fre = "[0-9]*\\.[0-9]+|[0-9]+";
        //let sfre = "-?[0-9]*\\.[0-9]+|-?[0-9]+";
        
        // 48.23532N 2.235235W | 48.23532°N 2.235235°W | 48.23532 N 2.235235 W
        let res = format!(r"^(?P<latdeg>{})[ °]?(?P<latside>[NS])\W+(?P<londeg>{})[ °]?(?P<lonside>[EW])$", fre, fre);
        let re = Regex::new(&res).unwrap();
        let caps_wrapped = re.captures(lat_lon_str.as_str());
        if caps_wrapped.is_some() {
            let caps = caps_wrapped.unwrap();
            let lat = caps.name("latdeg");
            let lon = caps.name("londeg");
            let ns = { if caps.name("latside").expect("unexpected") == "N" { 1.0 } else { -1.0 }  };
            let ew = { if caps.name("lonside").expect("unexpected") == "E" { 1.0 } else { -1.0 }  };
            if lat.is_some() && lon.is_some() {
                return Ok(Location{lat: ns * lat.expect("unexpected").parse::<f64>().unwrap(), lon: ew * lon.expect("unexpected").parse::<f64>().unwrap()});
            } 
        }
        Err(format!("bad location: {}", lat_lon_str))
    }

    /// True if this location is east from the other location.
    pub fn east_from(&self, other: Location) -> bool {
        let lon = self.lon;
        let olon = other.lon;
        
        if (lon - olon).abs() < 180.0 {
            lon > olon
        } else {
            if lon > 0.0 {
                lon > olon + 360.0
            } else {
                lon > olon - 360.0
            }
        }
    }
    
    /// True if this location is west from the other location.
    pub fn west_from(&self, other: Location) -> bool {
        ! self.east_from(other)
    }

    /// True if this location is north from the other location.
    pub fn north_from(&self, other: Location) -> bool {
        self.lat > other.lat
    }

    /// True if this location is south from the other location.
    pub fn south_from(&self, other: Location) -> bool {
        self.lat < other.lat
    }
    
    /// Distance to the other location.
    pub fn distance_to(&self, other: Location) -> f64 {
        // See: http://www.movable-type.co.uk/scripts/latlong.html
        const R: f64 = 6371000.0;
        let d_lat = other.lat - self.lat;
        let d_lon = other.lon - self.lon;
        
        let a = deg_sin(d_lat / 2.0) * deg_sin(d_lat / 2.0) +
                deg_cos(self.lat) * deg_cos(other.lat) *
                deg_sin(d_lon / 2.0) * deg_sin(d_lon / 2.0);
        let c = 2.0 * atan2(sqrt(a), sqrt(1.0 - a));
        R * c
    }
    
    /// Bearing to the given location.
    pub fn bearing_to(&self, other: Location) -> f64 {
        // See: http://www.movable-type.co.uk/scripts/latlong.html
        let d_lon = other.lon - self.lon;
        let y = deg_sin(d_lon) * deg_cos(other.lat);
        let x = deg_cos(self.lat) * deg_sin(other.lat) - deg_sin(self.lat) * deg_cos(other.lat) * deg_cos(d_lon);
        degrees_between(0.0, deg_atan2(y, x), 360.0)
    }
    
    /// Move to the given location.
    pub fn move_towards(&self, bearing: f64, distance: f64) -> Location {
        // See: http://www.movable-type.co.uk/scripts/latlong.html
        const R: f64 = 6371000.0;
        let dr = distance / R;
        let lat2 = deg_asin(deg_sin(self.lat) * cos(dr) +
                   deg_cos(self.lat) * sin(dr) * deg_cos(bearing));
        let lon2 = self.lon + deg_atan2(deg_sin(bearing) * sin(dr) * deg_cos(self.lat),
                   cos(dr) - deg_sin(self.lat) * deg_sin(lat2));
        Location::new(lat2, lon2)
    }   
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.lat >= 0.0 {
            if self.lon >= 0.0 {
                write!(f, "{}°N {}°E", self.lat, self.lon)
            } else {
                write!(f, "{}°N {}°W", self.lat, -self.lon)
            }
        } else {
            if self.lon >= 0.0 {
                write!(f, "{}°S {}°E", -self.lat, self.lon)
            } else {
                write!(f, "{}°S {}°W", -self.lat, -self.lon)
            }
        }
    }
}

// ---- GeoBox -------------------------------------------------------------------------------------

/// Rectangular geographical area outlined by latitudes and longitudes.
#[derive(Copy, Clone)]
pub struct GeoBox {
    northwest: Location,
    southeast: Location,
}

impl GeoBox {
    /// Constructor.
    pub fn new(nw: Location, se: Location) -> GeoBox {
        GeoBox { northwest: nw, southeast: se }
    }

    // Northwest corner of the box.
    pub fn northwest(&self) -> &Location {
        &self.northwest
    }

    // Southeast corner of the box.
    pub fn southeast(&self) -> &Location {
        &self.southeast
    }

    /// Create northeast corner based on the nw and se corners.
    pub fn northeast(&self) -> Location {
        Location::new(self.northwest.lat, self.southeast.lon)
    }
    
    /// Create southwest corner based on the nw and se corners.
    pub fn southwest(&self) -> Location {
        Location::new(self.southeast.lat, self.northwest.lon)
    }
    
    /// True if the given location is completely inside this box.
    pub fn contains(&self, loc: Location) -> bool {
        if self.northwest.west_from(self.southeast) {
            // Normal "small" areas
            loc.south_from(self.northwest) && loc.north_from(self.southeast) &&
            loc.west_from(self.southeast)  && loc.east_from(self.northwest)
        } else {
            // Bigger than semi-equator areas
            loc.south_from(self.northwest) && loc.north_from(self.southeast) &&
            !(loc.east_from(self.southeast) && loc.west_from(self.northwest))
        }
    }
    
    /// True if the given and this box have common area.
    pub fn intersects(&self, other: GeoBox) -> bool {
        self.contains(other.northwest) ||
        self.contains(other.southeast) ||
        other.contains(self.northwest) ||
        other.contains(self.southeast) ||
        self.contains(other.northeast()) ||
        self.contains(other.southwest()) ||
        other.contains(self.northeast()) ||
        other.contains(self.southwest())
    }
    
    /// Expand the area, to cover the given location too.
    pub fn expand(&self, loc: Location) -> GeoBox {
        let mut gb = *self;

	    // Longitude
	    if loc.west_from(self.northwest) {
	        gb.northwest = Location::new(gb.northwest.lat, loc.lon);
	    } else if loc.east_from(self.southeast) {
	        gb.southeast = Location::new(gb.southeast.lat, loc.lon);
	    }
	    
	    // Latitude
	    if loc.north_from(self.northwest) {
	        gb.northwest = Location::new(loc.lat, gb.northwest.lon);
	    } else if loc.south_from(self.southeast) {
	        gb.southeast = Location::new(loc.lat, gb.southeast.lon);
	    }
	    
        gb
    }
}

impl fmt::Display for GeoBox {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} - {}", self.northwest.to_string(), self.southeast.to_string())
    }
}

impl Ord for GeoBox {
    /// Ordering based on intersecting. If the boxes intersect they are considered to be equal.
    fn cmp(&self, other: &Self) -> Ordering {
        if self.southeast.lat < other.northwest.lat {
            Ordering::Less
        } else if self.northwest.lat > other.southeast.lat {
            Ordering::Greater
        } else {
            if self.southeast.lon < other.northwest.lon {
                Ordering::Less
            } else if self.northwest.lon > other.southeast.lon {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
    }
}

impl PartialOrd for GeoBox {
    /// Ordering based on intersecting.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for GeoBox {
    /// Ordering based on intersecting.
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for GeoBox {}

// ---- Projection ---------------------------------------------------------------------------------

/// Trait for projections, to map position between map coordinates and view pixels.
pub trait Projection {
    /// Converts coordinates to pixel position (with origin at 0°N 0°E).
    /// Parameter 'ppdoe' is pixels per degree on equator.
    fn location_to_global_pixel_pos(&self, loc: Location, ppdoe: f64) -> PixelPos;
    
    /// Converts pixel position (with origin at 0°N 0°E) to coordinates.
    /// Parameter 'ppdoe' is pixels per degree on equator.
    fn global_pixel_pos_to_location(&self, pp: PixelPos, ppdoe: f64) -> Location;
    
    // Returns gobal pixel position of the "top left" corner of the projection.
    fn northwest_global_pixel(&mut self, ppdoe: f64) -> PixelPos;
}

// ---- MercatorProjection -------------------------------------------------------------------------

pub struct MercatorProjection { 
    current_ppdoe: f64,
    current_northwest_global_pixel: PixelPos,
}

impl MercatorProjection {
    fn new() -> MercatorProjection {
        MercatorProjection{current_ppdoe: -1.0, current_northwest_global_pixel: PixelPos::new(0, 0)}
    }
}

///
/// Mercator projection coordinate conversions. 
///
/// https://en.wikipedia.org/wiki/Mercator_projection#Inverse_transformations
///
impl Projection for MercatorProjection {
    fn location_to_global_pixel_pos(&self, loc: Location, ppdoe: f64) -> PixelPos {
        const R: f64 = 360.0 / (2.0 * consts::PI);
        let phi = loc.lat * consts::PI / 180.0;
        let y = R * asinh(tan(phi));
        PixelPos::new((loc.lon * ppdoe) as i32, (-y * ppdoe) as i32)
    }
    
    fn global_pixel_pos_to_location(&self, pos: PixelPos, ppdoe: f64) -> Location {
        const R: f64 = 360.0 / (2.0 * consts::PI);
        let y = pos.y as f64 / ppdoe;
        let phi = asinh(tanh(y / R));
        Location::new(-phi * 180.0 / consts::PI, pos.x as f64 / ppdoe)
    }

    fn northwest_global_pixel(&mut self, ppdoe: f64) -> PixelPos {
        if self.current_ppdoe != ppdoe {
            // The northwest corner based on Mercator projection definition
            let nw_loc = Location::new(consts::PI.sinh().atan() * 180.0 / consts::PI, -180.0);
            self.current_ppdoe = ppdoe;
            self.current_northwest_global_pixel = self.location_to_global_pixel_pos(nw_loc, ppdoe);
        }
        
        self.current_northwest_global_pixel
    }
}

// ---- traditional math functions -----------------------------------------------------------------

fn sin(r: f64) -> f64 { r.sin() }
fn cos(r: f64) -> f64 { r.cos() }
fn tan(r: f64) -> f64 { r.tan() }
fn sinh(r: f64) -> f64 { r.sinh() }
fn cosh(r: f64) -> f64 { r.cosh() }
fn tanh(r: f64) -> f64 { r.tanh() }
fn sqrt(r: f64) -> f64 { r.sqrt() }
fn asinh(r: f64) -> f64 { r.asinh() }
fn acosh(r: f64) -> f64 { r.acosh() }
fn atan2(y: f64, x: f64) -> f64 { y.atan2(x) }
fn abs(v: f64) -> f64 { v.abs() }

// ---- degrees-based trigonometry -----------------------------------------------------------------

fn deg_sin(d: f64) -> f64 { (d * consts::PI / 180.0).sin() }
fn deg_cos(d: f64) -> f64 { (d * consts::PI / 180.0).cos() }
fn deg_tan(d: f64) -> f64 { (d * consts::PI / 180.0).tan() }
fn deg_atan2(y: f64, x: f64) -> f64 { y.atan2(x) * 180.0 / consts::PI }
fn deg_asin(d: f64) -> f64 { d.asin() * consts::PI / 180.0 }
fn deg_acos(d: f64) -> f64 { d.acos() * consts::PI / 180.0 }

/// Makes the degrees to be between the minimum and maximum.
fn degrees_between(minimum: f64, mut degrees: f64, maximum: f64) -> f64 {
    assert!(maximum > minimum);
    assert!(maximum - minimum >= 360.0);
    
    while degrees < minimum { degrees += 360.0; }
    while degrees >= maximum { degrees -= 360.0; }
    assert!(minimum <= degrees && degrees < maximum);
    degrees
}

// ---- tests --------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// True if val is between min and max values.
    fn close_enough_to(val: f64, expected: f64, max_error: f64) -> bool { (val - expected).abs() <= max_error }

    #[test]
    fn test_locations() {
        // Locations
        let paris             = Location::new(48.8567, 2.3508);
        let visby             = Location::new(57.634722, 18.299167);
        let melbourne         = Location::new(-37.813611, 144.963056);
        let port_aux_francais = Location::new(-49.35, 70.216667);
        let ushuaia           = Location::new(-54.8, -68.3);
        let willemstad        = Location::new(12.116667, -68.933333);
        let unalaska          = Location::new(53.880857, 166.542134);
        let wrangel_west      = Location::from_str("71°N 179°E").unwrap();
        let wrangel_east      = Location::from_str("71°N 178°W").unwrap();

        // Direction tests
        assert!( visby.east_from(paris) );
        assert!( paris.south_from(visby) );
        assert!( visby.north_from(paris) );
        assert!( melbourne.west_from(willemstad) );
        assert!( melbourne.west_from(unalaska) );
        assert!( unalaska.west_from(willemstad) );
        assert!( willemstad.west_from(paris) );
        assert!( paris.west_from(port_aux_francais) );
        assert!( port_aux_francais.west_from(melbourne) );
        assert!( melbourne.south_from(visby) );
        
        assert!( ushuaia.to_string() == "54.8°S 68.3°W" );
        
        let ushuaia2 = Location::from_string(ushuaia.to_string()).unwrap();
        assert!( ushuaia2.to_string() == "54.8°S 68.3°W" );
        
        assert!( Location::from_string("nonsense".to_string()).is_err() );
        
        // Distance, bearing and direction tests
        assert!( wrangel_west.west_from(wrangel_east) );
        assert!( wrangel_east.east_from(wrangel_west) );

        assert!( close_enough_to(wrangel_west.distance_to(wrangel_east), 108000.0, 1000.0) );
        assert!( close_enough_to(wrangel_east.distance_to(wrangel_west), 108000.0, 1000.0) );

        assert!( close_enough_to(wrangel_west.bearing_to(wrangel_east), 90.0, 2.0) );
        assert!( close_enough_to(wrangel_east.bearing_to(wrangel_west), 270.0, 2.0) );

        // Distance tests based on simple Google searches, like "distance from visby to melbourne"
        assert!( close_enough_to(visby.distance_to(melbourne), 15601000.0, 10000.0) );
        assert!( close_enough_to(melbourne.distance_to(ushuaia), 9234000.0, 10000.0) );
        assert!( close_enough_to(ushuaia.distance_to(melbourne), melbourne.distance_to(ushuaia), 1.0) );
    }

    #[test]
    fn test_geoboxes() {
        let globe179 = GeoBox::new(
            Location::from_str("90°N 179°W").unwrap(), 
            Location::from_str("90°S 179°E").unwrap());

        let mediterranean = GeoBox::new(
            Location::from_str("46°N 5°E").unwrap(), 
            Location::from_str("30°N 37°E").unwrap());

        let sardinia = GeoBox::new(
            Location::from_str("41.25°N 8°E").unwrap(), 
            Location::from_str("39°N 10°E").unwrap());
            
        let gotland = GeoBox::new(
            Location::from_str("58°N 18°E").unwrap(), 
            Location::from_str("57°N 19.5°E").unwrap());

        let azores = GeoBox::new(
            Location::from_str("40°N 32°W").unwrap(), 
            Location::from_str("36°S 24°W").unwrap());

        let pacific_ocean = GeoBox::new(
            Location::from_str("61°N 142°E").unwrap(), 
            Location::from_str("75°S 69°W").unwrap());

        let new_zealand = GeoBox::new(
            Location::from_str("34°S 166°E").unwrap(), 
            Location::from_str("47°S 179°W").unwrap());
            
        let easter_island = GeoBox::new(
            Location::from_str("27°S 110°W").unwrap(), 
            Location::from_str("28°S 109°W").unwrap());

        let taveuni = Location::from_str("16.8°S 179.5°W").unwrap();
        
        assert!( mediterranean.northwest.west_from(mediterranean.southeast) );
        assert!( pacific_ocean.northwest.west_from(pacific_ocean.southeast) );

        assert!( mediterranean.contains(sardinia.northwest) );
        assert!( ! mediterranean.contains(gotland.northwest) );
            
        assert!( mediterranean.intersects(sardinia) );
        assert!( ! mediterranean.intersects(gotland) );
        assert!( ! mediterranean.intersects(azores) );
        assert!( ! mediterranean.intersects(easter_island) );
        assert!( ! mediterranean.intersects(new_zealand) );
        
        assert!( pacific_ocean.contains(taveuni) );

        assert!( ! pacific_ocean.intersects(sardinia) );
        assert!( ! pacific_ocean.intersects(gotland) );
        assert!( ! pacific_ocean.intersects(azores) );
        assert!( pacific_ocean.intersects(easter_island) );
        assert!( pacific_ocean.intersects(new_zealand) );
        assert!( ! pacific_ocean.intersects(mediterranean) );

        assert!( globe179.northwest.east_from(globe179.southeast) );
        assert!( ! globe179.contains(taveuni) );
        assert!( globe179.contains(mediterranean.northwest) );
        assert!( globe179.intersects(mediterranean) );
        assert!( globe179.intersects(pacific_ocean) );  
    }

    #[test]
    fn test_geobox_cmp() {
        let mut bset: BTreeSet<GeoBox> = BTreeSet::new();
        let view = GeoBox::new(
            Location::from_str("10°N 0°E").unwrap(),
            Location::from_str("20°N 10°E").unwrap(),
        );
        bset.insert(view);
        // TODO
    }

    #[test]
    fn test_projection() {
        let mut mer = MercatorProjection::new();
        let pp = mer.northwest_global_pixel(1.0);
        debug!("mer: {}", mer.northwest_global_pixel(1.0));
        assert!(pp.x == -180 && pp.y == -179);
    }
}

