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

extern crate chrono;
extern crate xml;

//use std::fs::File;
use std::io::{Read};
//use std::result;
use std::option::{Option};
use std::collections::linked_list::LinkedList;

use super::model::*;
use super::super::geo::Location;

pub fn read_gpx<R: Read> (source: R) -> Result<Collection, String> {
    let mut parser = xml::reader::EventReader::new_with_config(
                    source, 
                    xml::reader::ParserConfig {
                        trim_whitespace: true,
                        whitespace_to_characters: false,
                        cdata_to_characters: true,
                        ignore_comments: true,
                        coalesce_characters: true,
                    }
                );
    let mut en_stack: LinkedList<String> = LinkedList::new(); // Element name stack
    let mut elem_characters = "".to_string();
    let mut col = Collection::new();
    
    /// Use XML parser to get data from the source.
    loop {
        match parser.next() {
            Ok(xml::reader::XmlEvent::StartDocument { .. }) => {
                debug!("GPXReader: StartDocument");
            }
            Ok(xml::reader::XmlEvent::StartElement { name, attributes, .. }) => {
                debug!("GPXReader: StartElement: {}", name);
                elem_characters = "".into();
                let en = &name.local_name;
                en_stack.push_back(en.clone());
                if en == "trk" {
                    col.tracks.push_back(Track::new());
                } else if en == "rte" {
                    col.routes.push_back(Route::new());
                } else if en == "trkseg" {
                    col.tracks.back_mut().unwrap().trkseg.push_back(TrackSegment::new());
                } else if en == "trkpt" {
                    let mut wpt = Point::new(0.0, 0.0);
                    
                    // Lat attribute
                    match pick_attr_value("lat", &attributes) {
                        Some(value) => { 
                            match value.parse() {
                                //Ok(f) => { wpt.unwrap().lat = f; }
                                Ok(f) => { wpt.location = Location::new(f, wpt.location.lon); }
                                Err(e) => { debug!("Bad GPX lat: {}" , value); }
                            }
                        },
                        None => { },
                    }
                    
                    // Lon attribute
                    match pick_attr_value("lon", &attributes) {
                        Some(value) => { 
                            match value.parse() {
                                Ok(f) => { wpt.location = Location::new(wpt.location.lat, f); }
                                Err(e) => { debug!("Bad GPX lon: {}" , value); }
                            }
                        },
                        None => { },
                    }
                    
                    col.tracks.back_mut().unwrap().trkseg.back_mut().unwrap().trkpt.push_back(wpt);
                } else {
                    debug!("GPXReader: StartElement: unknown element: {}", name);
                }
            }
            Ok(xml::reader::XmlEvent::Characters(s)) => {
                debug!("GPXReader: Characters {}", s);
                elem_characters = s;
            }
            Ok(xml::reader::XmlEvent::EndElement { name, .. }) => {
                debug!("GPXReader: EndElement: {}", name);
                let en = name.local_name;
                let een = en_stack.pop_back().unwrap();
                debug!("{} ? {}", en, een);
                assert!(een == en);
                if en == "trk" {
                } else if en == "trkseg" {
                } else if en == "trkpt" {
                } else if en == "ele" {
                    match find_waypoint(&mut col, &mut en_stack) {
                        Ok(wpt) => {
                            match elem_characters.parse::<f64>() {
                                Ok(f) => { wpt.elev = Some(f); }
                                Err(e) => { wpt.elev = None; debug!("Bad GPX elevation: {}", elem_characters); }
                            }
                        } 
                        Err(e) => {
                            debug!("{}", e);
                        }
                    }
                } else if en == "time" {
                    match find_waypoint(&mut col, &mut en_stack) {
                        Ok(wpt) => {
                            match elem_characters.as_str().parse::<chrono::DateTime<chrono::UTC>>() {
                                Ok(t) => { wpt.time = Some(t); }
                                Err(e) => { wpt.time = None; }
                            }
                            if !wpt.time.is_some() {
                                debug!("Bad GPX time: {}", elem_characters);
                            }
                        }
                        Err(e) => {
                            debug!("{}", e);
                        }
                    }
                }
            }
            Ok(xml::reader::XmlEvent::EndDocument { }) => {
                debug!("GPXReader: EndDocument");
                break;
            }
            Err(e) => {
                debug!("GPXReader: Error: {}", e);
                
                // Return error if not successful
                return Err("Something failed".into()); // FIXME
            }
            _ => {
                //debug!("GPXReader: Empty");
                return Err("Empty".into());
            }
        }
    }
    
    // Return the collection if successful
    Ok(col)
}

fn find_waypoint<'a>(col: &'a mut Collection, en_stack: &mut LinkedList<String>) -> Result<&'a mut Point, String> {
    let en = en_stack.back_mut().unwrap();
    if en == "trkpt" {
        Ok( col.tracks.back_mut().unwrap().trkseg.back_mut().unwrap().trkpt.back_mut().unwrap() )
    } else if en == "rtept" {
        Ok( col.routes.back_mut().unwrap().rtept.back_mut().unwrap() )
    } else if en == "wpt" {
        Ok( col.waypoints.back_mut().unwrap() )
    } else {
        Err((format!("Unexpected waypoint context {}", en).into()))
    }
}

/// Picks a wanted value for the given name.
fn pick_attr_value<'a>(name: &str, attrs: &'a Vec<xml::attribute::OwnedAttribute>) -> Option<&'a String> {  
    for attr in attrs {
        if attr.name.local_name == name {
            return Some(&attr.value)
        }
    }
    None
}

/// Format for strptime (ISO 8601).
pub const GPX_TIME_FORMAT: &'static str = "%Y-%m-%dT%H:%M:%S.%fZ";
pub const GPX_TIME_FORMAT_WITH_TIMEZONE: &'static str = "%Y-%m-%dT%H:%M:%S.%f%z";
pub const GPX_TIME_FORMAT_COMPACT: &'static str = "%Y%m%dT%H%M%S.%fZ";
pub const GPX_TIME_FORMAT_COMPACT_WITHOUT_FRACTIONS: &'static str = "%Y%m%dT%H%M%SZ";
pub const GPX_TIME_FORMAT_WITHOUT_FRACTIONS: &'static str = "%Y-%m-%dT%H:%M:%SZ";

