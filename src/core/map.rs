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

use std::cmp::*;

use core::tiles::{TileSource};

// ---- Map ----------------------------------------------------------------------------------------

/// A slippy map.
#[derive(Serialize, Deserialize, Debug)]
pub struct Map {
    #[serde(default)]
    pub slug: String,
    
    #[serde(default)]
    pub name: String,
    
    #[serde(default)]
    pub transparent: bool,

    #[serde(default)]
    pub urls: Vec<String>,
    
    #[serde(default)]
    pub token: String,
    
    #[serde(default)]
    pub copyright_text: String,
    
    #[serde(default)]
    pub copyright_url: String,
}

impl Map {
    /// Constructor.
    pub fn new(name: String) -> Map {
        Map {
            slug: format!("map-{}", super::id::next_id()),
            name: "".into(),
            transparent: false,
            urls: Vec::new(),
            token: "".into(),
            copyright_text: "".into(),
            copyright_url: "".into(),
        }
    }
    
    /// Convert Map into a TileSource.    
    pub fn to_tile_source(&self) -> TileSource {
        TileSource {
            slug: self.slug.clone(),
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

