
extern crate time;

pub mod model;
pub mod reader;
pub mod writer;

use self::model::*;
use self::reader::*;
use self::writer::*;

use std::result::Result;
use std::fs::File;
use std::path::Path;
use std::io::{BufReader, Read};
use self::time::{strftime};

#[test]
fn test_gpx_reader() {
    let filename = "/var/tmp/2016-08-20_Kaunissaari_ORIG.gpx";
    let file = File::open(&Path::new(filename)).unwrap();
    let reader = BufReader::new(file);

    let mut collection = read_gpx(reader);
    match collection {
        Ok(col) => {
            println!("ok");
            for track in col.tracks {
                println!("track");
                for seg in track.trkseg {
                    println!("seg");
                    for mut pt in seg.trkpt {
                        println!("{}", pt);
                    }
                }
            }
        }
        Err(e) => {
            println!("Error: {}", e);
        }
    }
/*
    let filename = "/var/tmp/2016-08-20_Kaunissaari_ORIG.gpx";
    let file = File::open(&Path::new(filename)).unwrap();
    let reader = BufReader::new(file);
    
    println!("Opening {}", filename);
    let parser = GPXReader::new(reader);
    for ev_res in parser {
        match ev_res {
            Result::Ok(ev) => {
                match ev {
                    GPXEvent::StartCollection { } => {
                    }
                    GPXEvent::Point { lat, lon, elev, time } => {
                        println!("Point: {} {} {} {} {}", lat, lon, elev.unwrap_or(-1.0), 
                            match time {
                                Some(v) => strftime(GPX_TIME_FORMAT, &v).unwrap_or("???".into()),
                                None => { "-".into() }
                            }.as_str(),
                            match time {
                                Some(v) => (v.tm_utcoff / 3600).to_string(),
                                None => { "-".into() }
                            }.as_str(),
                        );
                    }
                    GPXEvent::EndCollection { } => {
                        break;
                    }
                    _ => {}
                }
            }
            Result::Err(err) => {
                panic!("GPX parsing failed: {}", err);
            }
        }
    }
    assert!(false);
*/
}

