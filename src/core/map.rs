
use std::collections::linked_list::LinkedList;
use std::collections::BTreeSet;

use core::elements::*;

pub struct Map {
    pub slug: String,
    pub name: String,
    pub transparent: bool,
    pub urls: Vec<String>,
}

impl Map {
    pub fn new(slug: String, name: String) -> Map {
        Map {
            slug: slug,
            name: name,
            transparent: false,
            urls: Vec::new(),
        }
    }
}

