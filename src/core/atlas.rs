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

use std::collections::linked_list::LinkedList;
use std::collections::{HashMap, BTreeSet, BTreeMap};
use std::cmp::*;
use std::path;

use geocoord::geo::{Location, Projection};
use core::elements::*;
use core::id::{UniqueId};
use core::tiles::{TileSource};
use core::settings::{settings_read, default_max_zoom_level};
use core::persistence::*;

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
    pub maps: BTreeMap<String, Map>,
    
    /// Access tokens for maps.
    pub tokens: HashMap<String, MapToken>,
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
            tokens: HashMap::new(),
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
    pub fn set_map_name(&mut self, map_slug: String, name: String) {
        if let Some(mut map) = self.maps.remove(&map_slug) {
            map.name = name;
            self.maps.insert(map_slug, map);
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
#[derive(Serialize, Deserialize, Debug)]
pub struct Layer {
    /// Unique id.
    id: UniqueId,
    
    /// Map name.
    pub name: String,
    
    /// Order. The layer with the highest order are drawn the topmost. 
    /// Backdrop layer is expected to be zero.
    pub order: u16,
    
    /// In case of transparent map layers this is set to some, otherwise empty.
    /// Notice that the backdrop map layer is defined in MapView.
    #[serde(default)]
    pub map_slug: String,

    /// Map elements on the layer.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    #[serde(default)]
    pub element_ids: BTreeSet<UniqueId>,

    /// Remote layer uri.
    #[serde(skip_serializing_if = "String::is_empty")]
    #[serde(default)]
    pub remote_uri: String,
    
    /// Maps remote ids to local ones.
    #[serde(skip_serializing)]
    #[serde(default)]
    pub remote_to_local_ids: BTreeMap<UniqueId, UniqueId>,
    
    /// Maps local ids to remote ones.
    #[serde(skip_serializing)]
    #[serde(default)]
    pub local_to_remote_ids: BTreeMap<UniqueId, UniqueId>,
}

impl Layer {
    /// Constructor to create an empty layer.
    pub fn new(name: String, order: u16) -> Layer {
        Layer{
            id: super::id::next_id(),
            name: name,
            order: order,
            map_slug: "".into(),
            element_ids: BTreeSet::new(),
            remote_uri: "".into(),
            remote_to_local_ids: BTreeMap::new(),
            local_to_remote_ids: BTreeMap::new(),
        }    
    }

    /// Id getter.    
    pub fn id(&self) -> UniqueId { self.id }
    
    /// Returns true if this is a backdrop layer (order = 0).
    pub fn backdrop(&self) -> bool {
        (self.order == 0)
    }
    
    /// True if this is a remote layer.
    pub fn is_remote(&self) -> bool {
        !self.remote_uri.is_empty()
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

// ---- Map ----------------------------------------------------------------------------------------

/// Slippy map parameters.
#[derive(Serialize, Deserialize, Debug)]
pub struct Map {
    #[serde(default)]
    pub slug: String,
    
    #[serde(default)]
    pub name: String,
    
    #[serde(default)]
    pub tile_width: Option<i32>,
    
    #[serde(default)]
    pub tile_height: Option<i32>,
    
    #[serde(default = "default_max_zoom_level")]
    pub max_zoom_level: u8,
    
    #[serde(default)]
    pub transparent: bool,

    #[serde(default)]
    pub dark: bool,

    #[serde(default)]
    pub urls: Vec<String>,
    
    #[serde(default)]
    pub token: String,
    
    #[serde(default)]
    pub user_agent: Option<String>,
    
    #[serde(default)]
    pub copyrights: Vec<MapCopyright>,
}

impl Map {
    /// Constructor.
    pub fn new(name: String) -> Map {
        Map {
            slug: format!("map-{}", super::id::next_id()),
            name: "".into(),
            tile_width: None,
            tile_height: None,
            max_zoom_level: default_max_zoom_level(),
            transparent: false,
            dark: false,
            urls: Vec::new(),
            token: "".into(),
            user_agent: None,
            copyrights: Vec::new(),
        }
    }
    
    /// Convert Map into a TileSource. It's required that tile width and height are available,
    /// and None will be returned if not.
    pub fn to_tile_source(&self, tokens: &HashMap<String, MapToken>) -> Option<TileSource> {
        // Interpret the token field either as a reference or a literal
        let token = {
            if let Some(token) = tokens.get(&self.token) {
                token.value.clone()
            } else {
                self.token.clone()
            }
        };

        // Build tile source
        if self.tile_width.is_some() && self.tile_height.is_some() {
            Some(TileSource {
                slug: self.slug.clone(),
                urls: self.urls.clone(),
                token: token,
                user_agent: self.user_agent.clone(),
                tile_width: self.tile_width.unwrap(),
                tile_height: self.tile_height.unwrap(),
            })
        } else {
            None
        }
    }
    
    /// Returns projection of the map.
    pub fn as_projection(&self) -> Projection {
        // Currently the only supported projection is Mercator one.
        Projection::new_mercator_projection()
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

/// Map copyright information.
#[derive(Serialize, Deserialize, Debug)]
pub struct MapCopyright {
    pub text: String,
    pub url: String,
}

// ---- MapToken -----------------------------------------------------------------------------------
#[derive(Serialize, Deserialize, Debug)]
pub struct MapToken {
    pub value: String,
}

impl MapToken {
    pub fn new(value: String) -> MapToken {
        MapToken {
            value: value,
        }
    }
}

// ---- MapView ------------------------------------------------------------------------------------

/// Metadata about map window.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct MapView {
    /// Outline of the view area.
    pub center: Location,

    /// The current focus location. This is affected by mouse pointed position.    
    #[serde(skip_serializing, default)]
    pub focus: Option<Location>,

    /// Zoom level of the view.
    pub zoom_level: u8,
    
    /// Visible layer ids.
    pub visible_layer_ids: LinkedList<UniqueId>,
    
    /// Backdrop layer map slug.
    pub map_slug: String,
    
    /// Coordinates format used within the view.
    pub coordinates_format: String,
    
    /// Window x and y position in pixels.
    pub window_position: Option<(i32, i32)>,
    
    /// Window width and height in pixels.
    pub window_size: Option<(i32, i32)>,
}

impl MapView {
    pub fn new() -> MapView {
        MapView {
            center: Location::new(0.0, 0.0),
            focus: None,
            zoom_level: 3,
            visible_layer_ids: LinkedList::new(),
            map_slug: "".into(),
            coordinates_format: "dm".into(),
            window_position: None,
            window_size: None,
        }
    }
    
    /// Save state to disk.
    pub fn store(&self) {
        // Write to cache dir
        let pathbuf = settings_read().mapview_file();
        match serialize_to(self, pathbuf) {
            Ok(()) => {
                debug!("Map view stored successfully: {:?}", self);
            },
            Err(e) => {
                warn!("Failed to store map view: {}", e);
            }
        }
    }

    /// Load state from disk.
    pub fn restore() -> MapView {
        // Read from cache dir
        let pathbuf = settings_read().mapview_file();
        if pathbuf.exists() {
            match deserialize_from::<MapView, path::PathBuf>(pathbuf) {
                Ok(map_view) => {
                    debug!("Map view restored successfully");
                    return map_view;
                },
                Err(e) => {
                    warn!("Failed to restore map view: {}", e);
                }
            }
        }
        MapView::new()
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

