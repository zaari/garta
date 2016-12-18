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

extern crate num_cpus;

use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::collections::linked_list::{LinkedList};
use core::map::{Map};

/// A singleton-like construct for settings_read and settings_write methods.
lazy_static! {
    static ref SETTINGS: RwLock<Settings> = RwLock::new(Settings::new());
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
    
    /// Load settings from a file. Returns Ok if either the loading succeeded or 
    /// if the settings file wasn't found.
    pub fn load(&mut self) -> Result<(), &'static str> {
        Ok(()) // TODO
    }
    
    /// Save settings to a file. Returns Err if saving the file failed.
    pub fn save(&self) -> Result<(), &'static str> {
        Ok(()) // TODO
    }
}

/// Unlock settings for read access.
pub fn settings_read<'a>() -> RwLockReadGuard<'a, Settings> {
    SETTINGS.read().unwrap()
}

/// Unlock settings for write access.
pub fn settings_write<'a>() -> RwLockWriteGuard<'a, Settings> {
    SETTINGS.write().unwrap()
}

