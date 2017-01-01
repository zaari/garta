// Garta - GPX viewer and editor
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

use std::cmp::*;

extern crate serde_json;

use core::id::*;
use core::tiles::{TileSource};

// ---- Map ----------------------------------------------------------------------------------------

/// A slippy map.
#[derive(Serialize, Deserialize, Debug)]
pub struct Map {
    id: UniqueId, // TODO: id -> slug
    pub name: String,
    pub transparent: bool,
    pub urls: Vec<String>,
    pub token: String,
}

impl Map {
    /// Constructor.
    pub fn new(name: String) -> Map {
        Map {
            id: super::id::next_id(),
            name: name,
            transparent: false,
            urls: Vec::new(),
            token: "".into(),
        }
    }
    
//    /// Construct a new Map by reading it from a JSON file.
//    pub fn with_file(filename: &String) -> Result<Map, io::Error> {
//        // Read file to string
//        let mut f = File::open(filename.as_str())?;
//        let mut buf = String::new();
//        f.read_to_string(&mut buf)?;
//        
//        // Decode to JSON
//        match json::decode(buf.as_str()) {
//            Ok(map) => { Ok(map) }
//            Err(e) => { 
//                Err(io::Error::new(io::ErrorKind::Other, format!("json parse failed for {}", filename))) 
//            }
//        }
//    } // TODO: JSON loading and saving don't need to be here

//    // Save Map to a JSON file.
//    pub fn save(&self, filename: &String) -> Result<(), io::Error> {
//        let encoded_map = format!("{}", json::as_pretty_json(self));
//        let mut buf = File::create(filename.as_str())?;
//        buf.write(encoded_map.as_bytes())?;
//        Ok(()) 
//    }

    /// Id getter.    
    pub fn id(&self) -> UniqueId { self.id }

    /// Convert Map into a TileSource.    
    pub fn to_tile_source(&self) -> TileSource {
        TileSource {
            slug: format!("map-{}", self.id),
            name: self.name.clone(),
            urls: self.urls.clone(),
            token: self.token.clone(),
        }
    }
}

impl Ord for Map {
    // Name-based sorting.
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for Map {
    // Name-based sorting.
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl PartialEq for Map {
    // Name-based sorting.
    fn eq(&self, other: &Self) -> bool {
        self.name.eq(&other.name)
    }
}

impl Eq for Map {}

