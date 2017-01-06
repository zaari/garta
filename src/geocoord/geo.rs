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

extern crate regex;
extern crate chrono;
extern crate assert;

use std::f64::consts;
use std::fmt;
use std::time;
//use std::iter;
use self::regex::{Regex};

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

/// A struct representing geographic location and point of time. 
/// The coordinates are expected to use WGS84 reference ellipsoid.
/// The altitude is in metres above sea leve.
/// The distance calculations rely on Haversine formula which is accurate-enough for modern
/// location systems (GPS, GLONASS, Galileo) and is relatively fast.
#[derive(Copy, Clone, Debug)]
pub struct Location {
    pub lat: f64, // south-north
    pub lon: f64, // west-east
    pub elevation: Option<f64>, // altitude in metres
    pub time: Option<chrono::DateTime<chrono::UTC>>, // time related to the location
}

impl Location {
    // Constructor with coordinates only.
    pub fn new(lat: f64, lon: f64) -> Location {
        Location{lat: lat, lon: lon, elevation: None, time: None}
    }
    
    // Constructor with coordinates only.
    pub fn new_with_elevation(lat: f64, lon: f64, elevation: f64) -> Location {
        Location{lat: lat, lon: lon, elevation: Some(elevation), time: None}
    }
    
    // Constructor with coordinates only.
    pub fn new_with_elevation_and_time(lat: f64, lon: f64, elevation: f64, time: chrono::DateTime<chrono::UTC>) -> Location {
        Location{lat: lat, lon: lon, elevation: Some(elevation), time: Some(time)}
    }
    
    // Constructor with all fields.
    pub fn new_with_all(lat: f64, lon: f64, elevation: Option<f64>, time: Option<chrono::DateTime<chrono::UTC>>) -> Location {
        Location{lat: lat, lon: lon, elevation: elevation, time: time}
    }

    pub fn new_with_str(lat_lon_str: &str) -> Result<Location, String> {
        Location::new_with_string(lat_lon_str.to_string())
    }
    
    pub fn new_with_string(lat_lon_str: String) -> Result<Location, String> {
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
                return Ok(Location::new(ns * lat.expect("unexpected").parse::<f64>().unwrap(), ew * lon.expect("unexpected").parse::<f64>().unwrap()));
            } 
        }
        Err(format!("bad location: {}", lat_lon_str))
    }

    /// True if this location is east from the other location.
    pub fn east_from(&self, other: &Location) -> bool {
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
    pub fn west_from(&self, other: &Location) -> bool {
        ! self.east_from(other)
    }

    /// True if this location is north from the other location.
    pub fn north_from(&self, other: &Location) -> bool {
        self.lat > other.lat
    }

    /// True if this location is south from the other location.
    pub fn south_from(&self, other: &Location) -> bool {
        self.lat < other.lat
    }
    
    /// Haversine distance between the locations on a sphere.
    pub fn distance_to(&self, other: &Location) -> f64 {
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
    pub fn bearing_to(&self, other: &Location) -> f64 {
        // See: http://www.movable-type.co.uk/scripts/latlong.html
        let d_lon = other.lon - self.lon;
        let y = deg_sin(d_lon) * deg_cos(other.lat);
        let x = deg_cos(self.lat) * deg_sin(other.lat) - deg_sin(self.lat) * deg_cos(other.lat) * deg_cos(d_lon);
        degrees_between(0.0, deg_atan2(y, x), 360.0)
    }
    
    /// Considering this point as a starting point move to the given direction for given distance.
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
    
    /// Returns the average speed between the points in metres per second (m/s).
    /// None is returned if time is missing from either of the points.
    pub fn average_speed(&self, other: &Location) -> Option<f64> {
        let s = self.distance_to(other);
        if let Some(t) = self.delta_time(other) {
            return Some(s / t);
        }
        None
    }
    
    /// Returns delta time in seconds. If either of the points doesn't have time defined
    /// return None.
    pub fn delta_time(&self, other: &Location) -> Option<f64> {
        if let Some(time0) = self.time {
            if let Some(time1) = other.time {
                let duration = (time1 - time0).to_std().unwrap_or(time::Duration::from_secs(0));
                return Some(duration.as_secs() as f64 + 0.000000001 * duration.subsec_nanos() as f64);
            }
        }
        None
    }
    
    /// Computes the approximate distance to the horizon at sea-level if elevation is defined.
    /// The distance is in metres along the curved surface.
    /// Te effect of atmospheric refraction is ignored and the earth is assumed to be a sphere.
    pub fn distance_to_horizon(&self) -> Option<f64> {
        const R: f64 = 6371000.0;
        if let Some(h) = self.elevation {
            let d = sqrt(h * (2.0 * R + h));
            let s = R * atan(d / R);
            Some(s)
        } else {
            None
        }
    }
    
    /// Finds the closest location on the multiline object. The resulting location can be either
    /// at one of the multiline points or on the lines connecting them.
    pub fn closest_to_multiline_location<L: LocationSequence>(&self, multiline: &L) -> Location {
        Location::new(0.0, 0.0) // TODO
    }
    
    /// Finds the closest point of the multiline object.
    pub fn closest_to_multiline_point<L: LocationSequence>(&self, multiline: &L) -> Location {
        Location::new(0.0, 0.0) // TODO
    }
    
    /// Analyses the given area and returns true if the location is covered by the polygon.
    pub fn is_inside_polygon<G: LocationSequence>(&self, polygon: &G) -> bool {
        false // TODO
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

// ---- LocationSequence ---------------------------------------------------------------------------

/// Abstraction of location sequence, also known as multi line, route, track, path, etc.
/// Can be used to outline a polygon too.
pub trait LocationSequence {
    fn bounding_box(&self) -> GeoBox;

//    fn iterator(&self) -> I where I: Iterator<Item = Location>;
    // TODO: https://shadowmint.gitbooks.io/rust/content/howto/iterator.html
    
    /// Length from the first to the last location.
    fn distance(&self) -> f64 { 0.0 }
    
    /// Duration from the first to the last location.
    fn delta_time(&self) -> Option<f64> { None }

    /// Average speed from the first to the last location.
    fn average_speed(&self) -> Option<f64> { None }
    
    /// Altitude difference from the first to the last location.
    fn delta_elevation(&self) -> Option<f64> { None }
    
    /// Returns a tuple of cumulative elevation gain.
    fn cumulative_elevation_gain(&self) -> Option<f64> { None }
    
    /// Returns a tuple of cumulative elevation loss.
    fn cumulative_elevation_loss(&self) -> Option<f64> { None }

    /// Computes a time/speed histogram.
    fn compute_time_speed_histogram(&self, speed_unit: f64) -> Option<Vec<f64>> { None }

    /// Computes a distance/speed histogram.
    fn compute_distance_speed_histogram(&self, speed_unit: f64) -> Option<Vec<f64>> { None }

    /// Computes a speed/time histogram.
    fn compute_speed_time_histogram(&self, time_unit: f64) -> Option<Vec<f64>> { None }

    /// Computes a speed/distance histogram.
    fn compute_speed_distance_histogram(&self, distance_unit: f64) -> Option<Vec<f64>> { None }

    /// Length of the path including vertical gains and losses.
    fn distance_pythagorean(&self) -> Option<f64> { None }

    /// Return a new sequence without points that cause acceleration values higher than the given threshold (m/s^2). 
    fn filter_by_acceleration(&self, max_acceleration: f64) { }     
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
    pub fn contains(&self, loc: &Location) -> bool {
        if self.northwest.west_from(&self.southeast) {
            // Normal "small" areas
            loc.south_from(&self.northwest) && loc.north_from(&self.southeast) &&
            loc.west_from(&self.southeast)  && loc.east_from(&self.northwest)
        } else {
            // Bigger than semi-equator areas
            loc.south_from(&self.northwest) && loc.north_from(&self.southeast) &&
            !(loc.east_from(&self.southeast) && loc.west_from(&self.northwest))
        }
    }
    
    /// True if the given and this box have common area.
    pub fn intersects(&self, other: &GeoBox) -> bool {
        self.contains(&other.northwest) ||
        self.contains(&other.southeast) ||
        other.contains(&self.northwest) ||
        other.contains(&self.southeast) ||
        self.contains(&other.northeast()) ||
        self.contains(&other.southwest()) ||
        other.contains(&self.northeast()) ||
        other.contains(&self.southwest())
    }
    
    /// Expand the area, to cover the given location too.
    pub fn expand(&self, loc: &Location) -> GeoBox {
        let mut gb = *self;

	    // Longitude
	    if loc.west_from(&self.northwest) {
	        gb.northwest = Location::new(gb.northwest.lat, loc.lon);
	    } else if loc.east_from(&self.southeast) {
	        gb.southeast = Location::new(gb.southeast.lat, loc.lon);
	    }
	    
	    // Latitude
	    if loc.north_from(&self.northwest) {
	        gb.northwest = Location::new(loc.lat, gb.northwest.lon);
	    } else if loc.south_from(&self.southeast) {
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

// ---- Projection ---------------------------------------------------------------------------------

/// Collection of needed projections.
pub enum Projection {
    Mercator(MercatorProjection),
}

impl Projection {
    pub fn new_mercator_projection() -> Projection {
        Projection::Mercator(MercatorProjection::new())
    }

    /// Converts coordinates to pixel position (with origin at 0°N 0°E).
    /// Parameter 'ppdoe' is pixels per degree on equator.
    pub fn location_to_global_pixel_pos(&self, loc: Location, ppdoe: f64) -> PixelPos {
        match *self {
            Projection::Mercator(ref p) => { p.location_to_global_pixel_pos(loc, ppdoe) }
        }
    }
    
    /// Converts pixel position (with origin at 0°N 0°E) to coordinates.
    /// Parameter 'ppdoe' is pixels per degree on equator.
    pub fn global_pixel_pos_to_location(&self, pp: PixelPos, ppdoe: f64) -> Location {
        match *self {
            Projection::Mercator(ref p) => { p.global_pixel_pos_to_location(pp, ppdoe) }
        }
    }
    
    // Returns gobal pixel position of the "top left" corner of the projection.
    pub fn northwest_global_pixel(&mut self, ppdoe: f64) -> PixelPos {
        match *self {
            Projection::Mercator(ref mut p) => { p.northwest_global_pixel(ppdoe) }
        }
    }
}

// ---- MercatorProjection -------------------------------------------------------------------------

/// Mercator projection-realted position conversion math.
pub struct MercatorProjection { 
    current_ppdoe: f64,
    current_northwest_global_pixel: PixelPos,
}

/// Mercator projection coordinate conversions. 
/// https://en.wikipedia.org/wiki/Mercator_projection#Inverse_transformations
impl MercatorProjection {
    pub fn new() -> MercatorProjection {
        MercatorProjection{current_ppdoe: -1.0, current_northwest_global_pixel: PixelPos::new(0, 0)}
    }

    #[inline] 
    pub fn location_to_global_pixel_pos(&self, loc: Location, ppdoe: f64) -> PixelPos {
        const R: f64 = 360.0 / (2.0 * consts::PI);
        let phi = loc.lat * consts::PI / 180.0;
        let y = R * asinh(tan(phi));
        PixelPos::new((loc.lon * ppdoe) as i32, (-y * ppdoe) as i32)
    }
    
    #[inline] 
    pub fn global_pixel_pos_to_location(&self, pos: PixelPos, ppdoe: f64) -> Location {
        const R: f64 = 360.0 / (2.0 * consts::PI);
        let y = pos.y as f64 / ppdoe;
        let phi = asinh(tanh(y / R));
        Location::new(-phi * 180.0 / consts::PI, pos.x as f64 / ppdoe)
    }

    #[inline] 
    pub fn northwest_global_pixel(&mut self, ppdoe: f64) -> PixelPos {
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

#[inline] fn sin(r: f64) -> f64 { r.sin() }
#[inline] fn cos(r: f64) -> f64 { r.cos() }
#[inline] fn tan(r: f64) -> f64 { r.tan() }
#[inline] fn sinh(r: f64) -> f64 { r.sinh() }
#[inline] fn cosh(r: f64) -> f64 { r.cosh() }
#[inline] fn tanh(r: f64) -> f64 { r.tanh() }
#[inline] fn sqrt(r: f64) -> f64 { r.sqrt() }
#[inline] fn asinh(r: f64) -> f64 { r.asinh() }
#[inline] fn acosh(r: f64) -> f64 { r.acosh() }
#[inline] fn atan2(y: f64, x: f64) -> f64 { y.atan2(x) }
#[inline] fn atan(a: f64) -> f64 { a.atan() }
#[inline] fn abs(v: f64) -> f64 { v.abs() }

// ---- degrees-based trigonometry -----------------------------------------------------------------

#[inline] fn deg_sin(d: f64) -> f64 { (d * consts::PI / 180.0).sin() }
#[inline] fn deg_cos(d: f64) -> f64 { (d * consts::PI / 180.0).cos() }
#[inline] fn deg_tan(d: f64) -> f64 { (d * consts::PI / 180.0).tan() }
#[inline] fn deg_atan2(y: f64, x: f64) -> f64 { y.atan2(x) * 180.0 / consts::PI }
#[inline] fn deg_asin(d: f64) -> f64 { d.asin() * consts::PI / 180.0 }
#[inline] fn deg_acos(d: f64) -> f64 { d.acos() * consts::PI / 180.0 }

/// Makes the degrees to be between the minimum and maximum.
#[inline] 
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
//    use std::collections::BTreeSet;

    /// True if val is between min and max values.
    fn close_enough_to(val: f64, expected: f64, max_error: f64) -> bool { (val - expected).abs() <= max_error }

    #[test]
    fn test_location_directions() {
        // Locations
        let paris             = Location::new(48.8567, 2.3508);
        let visby             = Location::new(57.634722, 18.299167);
        let melbourne         = Location::new(-37.813611, 144.963056);
        let port_aux_francais = Location::new(-49.35, 70.216667);
        let ushuaia           = Location::new(-54.8, -68.3);
        let willemstad        = Location::new(12.116667, -68.933333);
        let unalaska          = Location::new(53.880857, 166.542134);
        let wrangel_west      = Location::new_with_str("71°N 179°E").unwrap();
        let wrangel_east      = Location::new_with_str("71°N 178°W").unwrap();

        // Direction tests
        assert!( visby.east_from(&paris) );
        assert!( paris.south_from(&visby) );
        assert!( visby.north_from(&paris) );
        assert!( melbourne.west_from(&willemstad) );
        assert!( melbourne.west_from(&unalaska) );
        assert!( unalaska.west_from(&willemstad) );
        assert!( willemstad.west_from(&paris) );
        assert!( paris.west_from(&port_aux_francais) );
        assert!( port_aux_francais.west_from(&melbourne) );
        assert!( melbourne.south_from(&visby) );
        
        assert!( ushuaia.to_string() == "54.8°S 68.3°W" );
        
        let ushuaia2 = Location::new_with_string(ushuaia.to_string()).unwrap();
        assert!( ushuaia2.to_string() == "54.8°S 68.3°W" );
        
        assert!( Location::new_with_string("nonsense".to_string()).is_err() );
        
        // Distance, bearing and direction tests
        assert!( wrangel_west.west_from(&wrangel_east) );
        assert!( wrangel_east.east_from(&wrangel_west) );

        assert::close(wrangel_west.distance_to(&wrangel_east), 108000.0, 1000.0);
        assert::close(wrangel_east.distance_to(&wrangel_west), 108000.0, 1000.0);

        assert::close(wrangel_west.bearing_to(&wrangel_east), 90.0, 2.0);
        assert::close(wrangel_east.bearing_to(&wrangel_west), 270.0, 2.0);

        // Distance tests based on simple Google searches, like "distance from visby to melbourne"
        assert::close(visby.distance_to(&melbourne), 15601000.0, 10000.0);
        assert::close(melbourne.distance_to(&ushuaia), 9234000.0, 10000.0);
        assert::close(ushuaia.distance_to(&melbourne), melbourne.distance_to(&ushuaia), 1.0);
    }

    #[test]
    fn test_location_times() {
        let utc = chrono::offset::utc::UTC::now();

        //
        // Porkkala - Naissaar
        //
        let porkkala_dt_fixed = chrono::DateTime::parse_from_rfc3339("2017-07-15T06:00:00+02:00").unwrap();
        let porkkala = Location::new_with_all(59.983333, 24.433333, None, 
            Some(porkkala_dt_fixed.with_timezone(&utc.timezone())));
        
        let naissaar_dt_fixed = chrono::DateTime::parse_from_rfc3339("2017-07-15T14:00:00+02:00").unwrap();
        let naissaar = Location::new_with_all(59.566667, 24.516667, None, 
            Some(naissaar_dt_fixed.with_timezone(&utc.timezone())));
        
        assert::close(porkkala.distance_to(&naissaar), 47000.0, 1000.0);
        assert::close(porkkala.delta_time(&naissaar).unwrap(), 8.0 * 3600.0, 0.1);
        assert::close(porkkala.average_speed(&naissaar).unwrap(), 1.63, 1.0);
        
        //
        // Naissaar - Tallin
        //
        let tallinn = Location::new(59.437222, 24.745278);
        assert!( naissaar.delta_time(&tallinn).is_none() );
        assert!( naissaar.average_speed(&tallinn).is_none() );
    }

    #[test]
    fn test_location_distance_to_horizon() {
        //
        // Test distance to horizon in any place on the earth (Gotska Sandön here)
        //
        let h = 20.0;
        let gotska_sandon = Location::new_with_elevation(58.366667, 19.25, h);
        let s = gotska_sandon.distance_to_horizon();
        assert::close( s.unwrap(), 3.57 * sqrt(h) * 1000.0, 10.0 );
    }

    #[test]
    fn test_geoboxes() {
        let globe179 = GeoBox::new(
            Location::new_with_str("90°N 179°W").unwrap(), 
            Location::new_with_str("90°S 179°E").unwrap());

        let mediterranean = GeoBox::new(
            Location::new_with_str("46°N 5°E").unwrap(), 
            Location::new_with_str("30°N 37°E").unwrap());

        let sardinia = GeoBox::new(
            Location::new_with_str("41.25°N 8°E").unwrap(), 
            Location::new_with_str("39°N 10°E").unwrap());
            
        let gotland = GeoBox::new(
            Location::new_with_str("58°N 18°E").unwrap(), 
            Location::new_with_str("57°N 19.5°E").unwrap());

        let azores = GeoBox::new(
            Location::new_with_str("40°N 32°W").unwrap(), 
            Location::new_with_str("36°S 24°W").unwrap());

        let pacific_ocean = GeoBox::new(
            Location::new_with_str("61°N 142°E").unwrap(), 
            Location::new_with_str("75°S 69°W").unwrap());

        let new_zealand = GeoBox::new(
            Location::new_with_str("34°S 166°E").unwrap(), 
            Location::new_with_str("47°S 179°W").unwrap());
            
        let easter_island = GeoBox::new(
            Location::new_with_str("27°S 110°W").unwrap(), 
            Location::new_with_str("28°S 109°W").unwrap());

        let taveuni = Location::new_with_str("16.8°S 179.5°W").unwrap();
        
        assert!( mediterranean.northwest.west_from(&mediterranean.southeast) );
        assert!( pacific_ocean.northwest.west_from(&pacific_ocean.southeast) );

        assert!( mediterranean.contains(&sardinia.northwest) );
        assert!( ! mediterranean.contains(&gotland.northwest) );
            
        assert!( mediterranean.intersects(&sardinia) );
        assert!( ! mediterranean.intersects(&gotland) );
        assert!( ! mediterranean.intersects(&azores) );
        assert!( ! mediterranean.intersects(&easter_island) );
        assert!( ! mediterranean.intersects(&new_zealand) );
        
        assert!( pacific_ocean.contains(&taveuni) );

        assert!( ! pacific_ocean.intersects(&sardinia) );
        assert!( ! pacific_ocean.intersects(&gotland) );
        assert!( ! pacific_ocean.intersects(&azores) );
        assert!( pacific_ocean.intersects(&easter_island) );
        assert!( pacific_ocean.intersects(&new_zealand) );
        assert!( ! pacific_ocean.intersects(&mediterranean) );

        assert!( globe179.northwest.east_from(&globe179.southeast) );
        assert!( ! globe179.contains(&taveuni) );
        assert!( globe179.contains(&mediterranean.northwest) );
        assert!( globe179.intersects(&mediterranean) );
        assert!( globe179.intersects(&pacific_ocean) );  
    }

    #[test]
    fn test_projection() {
        let mut mer = MercatorProjection::new();
        let pp = mer.northwest_global_pixel(1.0);
        debug!("mer: {}", mer.northwest_global_pixel(1.0));
        assert!(pp.x == -180 && pp.y == -179);
    }
}

