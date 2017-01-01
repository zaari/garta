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

use core::tiles::{TileSource};

// ---- Map ----------------------------------------------------------------------------------------

/// A slippy map.
#[derive(Serialize, Deserialize, Debug)]
pub struct Map {
    pub slug: String,
    
    #[serde(default = "map_default_string")]
    pub name: String,
    
    #[serde(default = "map_default_transparent")]
    pub transparent: bool,

    #[serde(default = "map_default_urls")]
    pub urls: Vec<String>,
    
    #[serde(default = "map_default_string")]
    pub token: String,
    
    #[serde(default = "map_default_string")]
    pub copyright_text: String,
    
    #[serde(default = "map_default_string")]
    pub copyright_url: String,
}

// Defaults for serde
fn map_default_transparent() -> bool { false }
fn map_default_urls() -> Vec<String> { Vec::new() }
fn map_default_string() -> String { "".into() }

impl Map {
    /// Constructor.
    pub fn new(name: String) -> Map {
        Map {
            slug: format!("map-{}", super::id::next_id()),
            name: map_default_string(),
            transparent: map_default_transparent(),
            urls: map_default_urls(),
            token: map_default_string(),
            copyright_text: map_default_string(),
            copyright_url: map_default_string(),
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

