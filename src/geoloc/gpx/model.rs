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

extern crate time;

use std::option::{Option};
use std::collections::linked_list::LinkedList;
use self::time::{Tm};
use std::fmt;

use super::super::geo::Location;

/// GPX File.
pub struct Collection {
    pub version: String,
    pub creator: String,
    
    pub metadata: Metadata,
    
    pub waypoints: LinkedList<Point>,
    pub routes: LinkedList<Route>,
    pub tracks: LinkedList<Track>,
    pub extension: Option<Extension>,
}

impl Collection {
    pub fn new() -> Collection {
        Collection {
            version: "".into(),
            creator: "".into(),
            metadata: Metadata {
                extension: None,
            },
            waypoints: LinkedList::new(),
            routes: LinkedList::new(),
            tracks: LinkedList::new(),
            extension: None,
        }
    }
}

/// Metadata for GPX file.
pub struct Metadata {
    pub extension: Option<Extension>,
}

/// GPX waypoint, route point or track point.
pub struct Point {
    pub location: Location,
    pub elev: Option<f64>, 
    pub time: Option<Tm>,
    pub magvar: Option<f64>,
    pub geoidheight: Option<f64>,
    pub name: Option<String>,
    pub cmt: Option<String>,
    pub desc: Option<String>,
    pub src: Option<String>,
    pub links: Vec<String>,
    pub sym: Option<String>,
    pub type_: Option<String>,
    pub fix: Option<String>,
    pub sat: Option<u8>,
    pub hdop: Option<f64>,
    pub vdop: Option<f64>,
    pub pdop: Option<f64>,
    pub ageofdgpsdata: Option<f64>,
    pub dgpsid: Option<u16>,
    pub extension: Option<Extension>,
    
    pub speed: Option<f64>,
}

impl Point {
    pub fn new(lat: f64, lon: f64) -> Point {
        Point {
            location: Location::new(lat, lon),
            elev: None,
            time: None,
            magvar: None,
            geoidheight: None,
            name: None,
            cmt: None,
            desc: None,
            src: None,
            links: Vec::new(),
            sym: None,
            type_: None,
            fix: None,
            sat: None,
            hdop: None,
            vdop: None,
            pdop: None,
            ageofdgpsdata: None,
            dgpsid: None,
            extension: None,
            
            speed: None,
        }
    }
}

impl fmt::Display for Point {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({})", self.location)
    }
}

/// GPX route.
pub struct Route {
    pub name: Option<String>,
    pub cmt: Option<String>,
    pub desc: Option<String>,
    pub src: Option<String>,
    pub links: LinkedList<String>,
    pub number: Option<u32>,
    pub type_: Option<String>,
    pub extension: Option<Extension>,
    pub rtept: LinkedList<Point>,
}

impl Route {
    pub fn new() -> Route {
        Route {
            name: None,
            cmt: None,
            desc: None,
            src: None,
            links: LinkedList::new(),
            number: None,
            type_: None,
            extension: None,
            rtept: LinkedList::new(),
        }
    }
}

/// GPX track consisting of segments.
pub struct Track {
    pub name: Option<String>,
    pub cmt: Option<String>,
    pub desc: Option<String>,
    pub src: Option<String>,
    pub links: LinkedList<String>,
    pub number: Option<u32>,
    pub type_: Option<String>,
    pub extension: Option<Extension>,
    pub trkseg: LinkedList<TrackSegment>,
}

impl Track {
    pub fn new() -> Track {
        Track {
            name: None,
            cmt: None,
            desc: None,
            src: None,
            links: LinkedList::new(),
            number: None,
            type_: None,
            extension: None,
            trkseg: LinkedList::new(),
        }
    }
}

/// GPX track segments with points.
pub struct TrackSegment {
    pub trkpt: LinkedList<Point>,
    pub extension: Option<Extension>,
}

impl TrackSegment {
    pub fn new() -> TrackSegment {
        TrackSegment {
            trkpt: LinkedList::new(),
            extension: None,
        }
    }
}


/// GPX extension element.
pub enum Extension {
    Elem {name: String, value: String, attrs: LinkedList<ExtensionAttribute>},
    List{name: String, extensions: LinkedList<Extension>},
}

/// Attribute of extension element.
pub struct ExtensionAttribute {
    pub name: String,
    pub value: String,
}

