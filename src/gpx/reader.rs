
extern crate time;
extern crate xml;

//use std::fs::File;
use std::io::{Read};
//use std::result;
use std::option::{Option};
use std::collections::linked_list::LinkedList;
use self::time::{Tm, now, strptime, strftime};

use gpx::model::*;

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
                println!("GPXReader: StartDocument");
            }
            Ok(xml::reader::XmlEvent::StartElement { name, attributes, .. }) => {
                println!("GPXReader: StartElement: {}", name);
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
                                Ok(f) => { wpt.lat = f; }
                                Err(e) => { println!("Bad GPX lat: {}" , value); }
                            }
                        },
                        None => { },
                    }
                    
                    // Lon attribute
                    match pick_attr_value("lon", &attributes) {
                        Some(value) => { 
                            match value.parse() {
                                Ok(f) => { wpt.lon = f; }
                                Err(e) => { println!("Bad GPX lon: {}" , value); }
                            }
                        },
                        None => { },
                    }
                    
                    col.tracks.back_mut().unwrap().trkseg.back_mut().unwrap().trkpt.push_back(wpt);
                } else {
                    println!("GPXReader: StartElement: unknown element: {}", name);
                }
            }
            Ok(xml::reader::XmlEvent::Characters(s)) => {
                println!("GPXReader: Characters {}", s);
                elem_characters = s;
            }
            Ok(xml::reader::XmlEvent::EndElement { name, .. }) => {
                println!("GPXReader: EndElement: {}", name);
                let en = name.local_name;
                let een = en_stack.pop_back().unwrap();
                println!("{} ? {}", en, een);
                assert!(een == en);
                if en == "trk" {
                } else if en == "trkseg" {
                } else if en == "trkpt" {
                } else if en == "ele" {
                    match find_waypoint(&mut col, &mut en_stack) {
                        Ok(wpt) => {
                            match elem_characters.parse::<f64>() {
                                Ok(f) => { wpt.elev = Some(f); }
                                Err(e) => { wpt.elev = None; println!("Bad GPX elevation: {}", elem_characters); }
                            }
                        } 
                        Err(e) => {
                            println!("{}", e);
                        }
                    }
                } else if en == "time" {
                    match find_waypoint(&mut col, &mut en_stack) {
                        Ok(wpt) => {
                            match strptime(elem_characters.as_str(), GPX_TIME_FORMAT) {
                                Ok(t) => { wpt.time = Some(t); }
                                Err(e) => { wpt.time = None; }
                            }
                            if wpt.time.is_some() {
                                match strptime(elem_characters.as_str(), GPX_TIME_FORMAT_WITH_TIMEZONE) {
                                Ok(t) => { wpt.time = Some(t); }
                                Err(e) => { wpt.time = None; }
                                }
                            }
                            if wpt.time.is_some() {
                                match strptime(elem_characters.as_str(), GPX_TIME_FORMAT_COMPACT) {
                                Ok(t) => { wpt.time = Some(t); }
                                Err(e) => { wpt.time = None; }
                                }
                            }
                            if wpt.time.is_some() {
                                match strptime(elem_characters.as_str(), GPX_TIME_FORMAT_COMPACT_WITHOUT_FRACTIONS) {
                                Ok(t) => { wpt.time = Some(t); }
                                Err(e) => { wpt.time = None; }
                                }
                            }
                            if wpt.time.is_some() {
                                match strptime(elem_characters.as_str(), GPX_TIME_FORMAT_WITHOUT_FRACTIONS) {
                                Ok(t) => { wpt.time = Some(t); }
                                Err(e) => { wpt.time = None; }
                                }
                            }
                            if wpt.time.is_some() {
                                println!("Bad GPX time: {}", elem_characters);
                            }
                        }
                        Err(e) => {
                            println!("{}", e);
                        }
                    }
                }
            }
            Ok(xml::reader::XmlEvent::EndDocument { }) => {
                println!("GPXReader: EndDocument");
                break;
            }
            Err(e) => {
                println!("GPXReader: Error: {}", e);
                
                // Return error if not successful
                return Err("Something failed".into()); // FIXME
            }
            _ => {
                //println!("GPXReader: Empty");
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

