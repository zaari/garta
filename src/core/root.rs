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

use std::cell::{RefCell};
use std::rc::{Rc};
//use std::option::*;
use std::collections::linked_list::LinkedList;
use std::collections::{HashMap, BTreeSet};

use core::elements::*;
use core::map::{Map};
use core::id::{UniqueId, NONE};

// ---- Atlas --------------------------------------------------------------------------------------

/// The root object in the domain model.
pub struct Atlas {
    pub slug: String,
    pub name: String,
    
    /// All the layers. The first layer is rendered first.
    pub layers: HashMap<UniqueId, Layer>,
    
    /// Attractions
    pub attractions: HashMap<UniqueId, Attraction>,

    /// GPX waypoints
    pub waypoints: HashMap<UniqueId, Waypoint>,
    
    /// GPX routes
    pub routes: HashMap<UniqueId, Path>,
    
    /// GPX tracks
    pub tracks: HashMap<UniqueId, Path>,
    
    /// Areas.
    pub areas: HashMap<UniqueId, Area>,
    
    /// Collection of maps.
    pub maps: HashMap<UniqueId, Map>,
}

impl Atlas {
    /// Constructor.
    pub fn new(slug: String) -> Atlas {
        Atlas{
            slug: slug,
            name: "unnamed".into(),
            layers: HashMap::new(),
            attractions: HashMap::new(),
            waypoints: HashMap::new(),
            tracks: HashMap::new(),
            routes: HashMap::new(),
            areas: HashMap::new(),
            maps: HashMap::new(),
        }    
    }

    /// Load atlas
    pub fn load(&mut self, status: &mut AtlasLoadSaveStatus) {
        status.total = 0;
        status.loaded = 0;
        status.ready = false;
        // TODO
    }
    
    /// Save atlas
    pub fn save(&self, status: &mut AtlasLoadSaveStatus) -> bool {
        status.total = 0;
        status.loaded = 0;
        status.ready = false;
        // TODO
        false
    }

    /// Returns the backdrop layer id.
    pub fn backdrop_layer_id(&self) -> Option<UniqueId> {
        for (layer_id, layer) in &self.layers {
            if layer.backdrop {
                Some(layer_id);
            }
        }
        None
    }
}

// ---- AtlasLoadSaveStatus ----------------------------------------------------------------------
pub struct AtlasLoadSaveStatus {
    pub total: i64,
    pub loaded: i64,
    pub ready: bool,
}

impl AtlasLoadSaveStatus {
    pub fn new() -> AtlasLoadSaveStatus {
        AtlasLoadSaveStatus {
            total: 0,
            loaded: 0,
            ready: false,
        }
    }
}

// ---- Layer --------------------------------------------------------------------------------------

/// Layer in a atlas containing map elements.
pub struct Layer {
    // Unique id.
    id: UniqueId,
    
    // Map name.
    pub name: String,
    
    /// True if this layer has an opaque map.
    pub backdrop: bool,
    
    /// In case of backdrop Layers this is set to Some.
    pub map_id: UniqueId,

    /// Map elements on the layer.
    pub elements: BTreeSet<Rc<RefCell<MapElement>>>,
}

impl Layer {
    /// Create a new empty layer.
    pub fn new(name: String) -> Layer {
        Layer{
            id: super::id::next_id(),
            name: name,
            backdrop: false,
            map_id: NONE,
            elements: BTreeSet::new(),
        }    
    }

    /// Id getter.    
    pub fn id(&self) -> UniqueId { self.id }
}

// ---- MapView ------------------------------------------------------------------------------------

/// Map window
pub struct MapView {
    pub zoom_level: u8,
    pub visible_layer_ids: LinkedList<UniqueId>,
}

impl MapView {
    pub fn new() -> MapView {
        MapView {
            zoom_level: 3,
            visible_layer_ids: LinkedList::new(),
        }
    }
}

impl Clone for MapView {
    fn clone(&self) -> MapView {
        MapView {
            zoom_level: self.zoom_level.clone(),
            visible_layer_ids: self.visible_layer_ids.clone(),
        }
    }
}

// ---- tests --------------------------------------------------------------------------------------

#[test]
fn test_atlas() {
    // Create a atlas and layer
    let la = Layer::new("Nimi".into());
    let mut p = Atlas::new("proj".into());
    
    // Add the layer to the atlas
    p.layers.insert(la.id(), la);
}

