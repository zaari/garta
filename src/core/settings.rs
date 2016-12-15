extern crate num_cpus;

use std::cell::{RefCell};
use std::rc::{Rc};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
/*use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};*/
use std::collections::linked_list::{LinkedList};
use core::map::{Map};

/// A singleton-like construct for settings_read and settings_write methods.
lazy_static! {
    static ref settings: RwLock<Settings> = RwLock::new(Settings::new());
}

pub struct Settings {
    pub maps: LinkedList<Arc<Map>>,

    // Root data directory for the project
    data_directory: String,
    
    // Root cache directory for tiles and images
    cache_directory: String,
    
    /// Maximum number of threads loading and processing data
    worker_threads: i32,
}

impl Settings {
    /// Private constructor
    fn new() -> Settings {
        Settings {
            maps: LinkedList::new(),
            data_directory: "~/.local/share/garta/data".to_string(),
            cache_directory: "~/.local/share/garta/cache".to_string(),
            worker_threads: -1,
        }
    }
    
    // Get data directory
    pub fn data_directory(&self) -> &String { &self.data_directory }
    
    /// Get cache directory
    pub fn cache_directory(&self) -> &String { &self.cache_directory }
    
    /// Get maximum number of threads
    pub fn worker_threads(&self) -> i32 { 
        if self.worker_threads < 0 {
            num_cpus::get() as i32
        } else if self.worker_threads == 0 {
            1
        } else {
            self.worker_threads
        }
    }
    
    /// Load settings from a file
    pub fn load(&mut self) {
    }
    
    /// Save settings to a file
    pub fn save(&self) {
    }
}

/// Unlock settings for read access.
pub fn settings_read<'a>() -> RwLockReadGuard<'a, Settings> {
    unsafe {
        settings.read().unwrap()
    }
}

/// Unlock settings for write access.
pub fn settings_write<'a>() -> RwLockWriteGuard<'a, Settings> {
    unsafe {
        settings.write().unwrap()
    }
}

