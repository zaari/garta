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

use std::collections::linked_list::LinkedList;
use std::collections::{HashMap, BTreeSet, BTreeMap};
use std::cmp::*;

use core::elements::*;
use core::map::{Map};
use core::id::{UniqueId, NONE};

// ---- Atlas --------------------------------------------------------------------------------------

/// The root object in the domain model.
pub struct Atlas {
    pub slug: String,
    pub name: String,
    
    /// Layers
    pub layers: BTreeMap<UniqueId, Layer>,
    
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
    pub maps: BTreeMap<UniqueId, Map>,
}

impl Atlas {
    /// Constructor.
    pub fn new(slug: String) -> Atlas {
        Atlas{
            slug: slug,
            name: "unnamed".into(),
            layers: BTreeMap::new(),
            attractions: HashMap::new(),
            waypoints: HashMap::new(),
            tracks: HashMap::new(),
            routes: HashMap::new(),
            areas: HashMap::new(),
            maps: BTreeMap::new(),
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
            if layer.backdrop() {
                return Some(*layer_id);
            }
        }
        None
    }
    
    /// Set layer order value and ensure that the BTree is valid after the change.
    pub fn set_layer_order(&mut self, layer_id: UniqueId, order: u16) {
        if let Some(mut layer) = self.layers.remove(&layer_id) {
            layer.order = order;
            self.layers.insert(layer_id, layer);
        }
    }
    
    /// Set map name value and ensure that the BTree is valid after the change.
    pub fn set_map_name(&mut self, map_id: UniqueId, name: String) {
        if let Some(mut map) = self.maps.remove(&map_id) {
            map.name = name;
            self.maps.insert(map_id, map);
        }
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
    /// Unique id.
    id: UniqueId,
    
    /// Map name.
    pub name: String,
    
    /// Order. The layer with the highest order are drawn the topmost. 
    /// Backdrop layer is expected to be zero.
    pub order: u16,
    
    /// In case of transparent map layers this is set to Some, otherwise None.
    /// Notice that the backdrop map layer is defined in MapView.
    pub map_id: UniqueId,

    /// Map elements on the layer.
    pub element_ids: BTreeSet<UniqueId>,
}

impl Layer {
    /// Constructor to create an empty layer.
    pub fn new(name: String, order: u16) -> Layer {
        Layer{
            id: super::id::next_id(),
            name: name,
            order: order,
            map_id: NONE,
            element_ids: BTreeSet::new(),
        }    
    }

    /// Id getter.    
    pub fn id(&self) -> UniqueId { self.id }
    
    /// Returns true if this is a backdrop layer (order = 0).
    pub fn backdrop(&self) -> bool {
        (self.order == 0)
    }
}

impl Ord for Layer {
    fn cmp(&self, other: &Self) -> Ordering {
        self.order.cmp(&other.order)
    }
}

impl PartialOrd for Layer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.order.partial_cmp(&other.order)
    }
}

impl PartialEq for Layer {
    fn eq(&self, other: &Self) -> bool {
        self.order.eq(&other.order)
    }
}

impl Eq for Layer {}


// ---- MapView ------------------------------------------------------------------------------------

/// Metadata about map window.
pub struct MapView {
    /// Zoom level of the view.
    pub zoom_level: u8,
    
    /// Visible layer ids.
    pub visible_layer_ids: LinkedList<UniqueId>,
    
    /// Backdrop layer map id.
    pub map_id: UniqueId,
    
    /// Coordinates format used within the view.
    pub coordinates_format: String,
}

impl MapView {
    pub fn new() -> MapView {
        MapView {
            zoom_level: 3,
            visible_layer_ids: LinkedList::new(),
            map_id: NONE,
            coordinates_format: "dm".into(),
        }
    }
}

impl Clone for MapView {
    fn clone(&self) -> MapView {
        MapView {
            zoom_level: self.zoom_level.clone(),
            visible_layer_ids: self.visible_layer_ids.clone(),
            map_id: self.map_id,
            coordinates_format: self.coordinates_format.clone(),
        }
    }
}

// ---- tests --------------------------------------------------------------------------------------

#[test]
fn test_atlas() {
    let la = Layer::new("Nimi".into(), 0);
    // Create a atlas and layer
    let la_id = la.id();
    assert!(la.backdrop() == true);
    
    // Add the layer to the atlas
    let mut p = Atlas::new("proj".into());
    p.layers.insert(la.id(), la);
    
    assert!(p.backdrop_layer_id().unwrap() == la_id);
}

