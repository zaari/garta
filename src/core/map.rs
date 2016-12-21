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

use std::cmp::*;
use core::id::*;

// ---- Map ----------------------------------------------------------------------------------------

/// A slippy map.
pub struct Map {
    id: UniqueId,
    pub name: String,
    pub transparent: bool,
    pub urls: Vec<String>,
}

impl Map {
    /// Constructor.
    pub fn new(name: String) -> Map {
        Map {
            id: super::id::next_id(),
            name: name,
            transparent: false,
            urls: Vec::new(),
        }
    }

    /// Id getter.    
    pub fn id(&self) -> UniqueId { self.id }
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


