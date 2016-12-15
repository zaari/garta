
use std::cell::{RefCell};
use std::rc::{Rc};
//use std::option::*;
use std::collections::linked_list::LinkedList;
use std::collections::BTreeSet;

use core::elements::*;
use core::map::*;

use std::cmp::Ord;

pub struct MapView {
    pub zoom_level: u8,
    pub visible_layers: LinkedList<Rc<RefCell<Layer>>>,
    // TODO: move to a separate file
}

impl MapView {
    pub fn new() -> MapView {
        MapView {
            zoom_level: 3,
            visible_layers: LinkedList::new(),
        }
    }
}

// ---- Project ------------------------------------------------------------------------------------

/// The root object in the domain model.
pub struct Project {
    pub slug: String,
    pub name: String,
    pub views: LinkedList<Rc<RefCell<MapView>>>,
    pub layers: LinkedList<Rc<RefCell<Layer>>>,
    
    pub attractions: LinkedList<Rc<RefCell<Attraction>>>,
    pub routes: LinkedList<Rc<RefCell<Path>>>,
    pub tracks: LinkedList<Rc<RefCell<Path>>>,
    pub areas: LinkedList<Rc<RefCell<Area>>>,
}

impl Project {
    /// Constructor.
    pub fn new(slug: String) -> Project {
        Project{
            slug: slug,
            name: "unnamed".into(),
            views: LinkedList::new(),
            layers: LinkedList::new(),
            attractions: LinkedList::new(),
            tracks: LinkedList::new(),
            routes: LinkedList::new(),
            areas: LinkedList::new(),
        }    
    }

    /// Add a new player to the project.
    pub fn add_layer(&mut self, layer: Rc<RefCell<Layer>>) {
        self.layers.push_back(layer);
    }

    /// Load project
    pub fn load(&mut self, status: &mut ProjectLoadSaveStatus) {
        status.total = 0;
        status.loaded = 0;
        status.ready = false;
        // TODO
    }
    
    /// Save project
    pub fn save(&self, status: &mut ProjectLoadSaveStatus) -> bool {
        status.total = 0;
        status.loaded = 0;
        status.ready = false;
        // TODO
        false
    }

    /// Get name of the project.
    pub fn name(&self) -> &String {
        &self.name
    }
    
    /// Set name of the project.
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }
}

// ---- ProjectLoadSaveStatus ----------------------------------------------------------------------

pub struct ProjectLoadSaveStatus {
    pub total: i64,
    pub loaded: i64,
    pub ready: bool,
}

impl ProjectLoadSaveStatus {
    pub fn new() -> ProjectLoadSaveStatus {
        ProjectLoadSaveStatus {
            total: 0,
            loaded: 0,
            ready: false,
        }
    }
}

// ---- Layer --------------------------------------------------------------------------------------

/// Layer in a project containing map elements.
pub struct Layer {
    pub slug: String,
    pub name: String,
    pub backdrop: bool,

    elements: BTreeSet<Rc<RefCell<MapElement>>>,
}

impl Layer {
    /// Create a new empty layer.
    pub fn new(slug: String, name: String) -> Layer {
        Layer{
            slug: slug,
            name: name,
            backdrop: false,
            elements: BTreeSet::new(),
        }    
    }

    // Return layer slug.
    pub fn slug(&self) -> &String {
        &self.slug
    }
    
    // Set layer slug.
    pub fn set_slug(&mut self, slug: String) {
        self.slug = slug;
    }
}


// ---- tests --------------------------------------------------------------------------------------

#[test]
fn test_project() {
    // Create a project and layer
    let la = Rc::new(RefCell::new(Layer::new("nimi".into(), "nimi".into()) ));    
    let mut p = Project::new("proj".into());
    
    // Add the layer to the project
    p.add_layer(la.clone());
    
    // Test setting slug
    la.borrow_mut().set_slug("1".into());
    assert!(la.borrow().slug() == "1");
    la.borrow_mut().set_slug("2".into());
    assert!(la.borrow().slug() == "2");
}

