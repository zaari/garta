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

extern crate num_cpus;
extern crate serde_json;

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::fs;
use std::cmp::{min, max};
use std::env;
use std::path;

/// Default number of days until tiles expire if the server doesn't send expiration information.
pub static DEFAULT_TILE_EXPIRE_DAYS: i64 = 7;

// Minimum number of worker threads in case of auto detection.
static MIN_WORKER_THREADS: i32 = 2;

// Maximum number of worker threads in case of auto detection.
static MAX_WORKER_THREADS: i32 = 8;

/// A singleton-like construct for settings_read and settings_write methods.
lazy_static! {
    static ref SETTINGS: RwLock<Settings> = RwLock::new(Settings::new());
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Settings {
    // Per host data directory for the atlas
    host_data_directory: String,
    
    // Per user data directory for the atlas
    user_data_directory: String,
    
    // Directory for config file(s). Maps are also held here.
    config_directory: String,
    
    // A cache directory for tiles and images
    cache_directory: String,
    
    /// Maximum number of threads loading and processing data
    worker_threads: i32,
    
    // HTTP read timeout when fetching tiles from network sources.
    pub tile_read_timeout: u64,
    
    // HTTP write timeout when sendinf requests.
    pub tile_write_timeout: u64,
    
    /// Number of times to try reloading HTTP resources.
    pub http_retry_count: u8,
    
    // Tile memory cache size in bytes. If no limits are wanted this value should be set to None.
    pub tile_mem_cache_capacity: Option<usize>,
    
    // Tile disk size in bytes. If no limits are wanted this value should be set to None.
    pub tile_disk_cache_capacity: Option<u64>,
    
    /// Map window position and size.
    pub main_window_geometry: String,
    
    /// The command which is used to launch an external web browser.
    pub browser_command: String,
}

impl Settings {
    /// Private constructor
    fn new() -> Settings {
        Settings {
            host_data_directory: ".".to_string(), // TODO: "/usr/local/share/garta".to_string(),
            user_data_directory: "~/.local/share/garta".to_string(),
            config_directory: "~/.config/garta".to_string(),
            cache_directory: "~/.cache/garta".to_string(),
            worker_threads: -1,
            tile_read_timeout: 20,
            tile_write_timeout: 10,
            http_retry_count: 3,
            tile_mem_cache_capacity: Some(10 * 1024 * 1024),
            tile_disk_cache_capacity: Some(100 * 1024 * 1024),
            main_window_geometry: "".to_string(),
            browser_command: "xdg-open".into(),
        }
    }

    /// Get project/atlas directory
    pub fn project_directory(&self) -> path::PathBuf { 
        string_to_path(&self.user_data_directory)
    }

    /// Get ui files directory
    pub fn ui_directory(&self) -> path::PathBuf { 
        let mut pb = string_to_path(&self.host_data_directory); pb.push("ui"); pb 
    }
    
    /// Get host-wide maps directory
    pub fn host_maps_directory(&self) -> path::PathBuf { 
        let mut pb = string_to_path(&self.host_data_directory); pb.push("maps"); pb 
    }
    
    /// Get user's maps directory
    pub fn user_maps_directory(&self) -> path::PathBuf { 
        let mut pb = string_to_path(&self.config_directory); pb.push("maps"); pb 
    }

    /// Get host-wide tokens directory
    pub fn host_tokens_directory(&self) -> path::PathBuf { 
        let mut pb = string_to_path(&self.host_data_directory); pb.push("tokens"); pb 
    }
    
    /// Get user's tokens directory
    pub fn user_tokens_directory(&self) -> path::PathBuf { 
        let mut pb = string_to_path(&self.config_directory); pb.push("tokens"); pb 
    }
    
    /// Get settings filename
    pub fn settings_file(&self) -> path::PathBuf { 
        let mut pb = string_to_path(&self.config_directory); pb.push("settings"); pb 
    }
    
    /// Get mapview filename
    pub fn mapview_file(&self) -> path::PathBuf { 
        let mut pb = string_to_path(&self.config_directory); pb.push("map-view"); pb 
    }
    
    /// Get cache directory
    pub fn cache_directory(&self) -> path::PathBuf { string_to_path(&self.cache_directory) }
    
    /// Get maximum number of threads
    pub fn worker_threads(&self) -> i32 { 
        if self.worker_threads < 0 {
            min(MAX_WORKER_THREADS, max(num_cpus::get() as i32, MIN_WORKER_THREADS))
        } else if self.worker_threads == 0 {
            1
        } else {
            self.worker_threads
        }
    }
    
    /// Load settings from a file. Returns Ok if either the loading succeeded or 
    /// if the settings file wasn't found. Also creates the missing directories.
    pub fn load(&mut self) -> Result<(), &'static str> {
        // Touch directories
        let dirs = vec![
            self.project_directory().clone(), 
            self.user_maps_directory().clone(), 
            self.user_tokens_directory().clone(), 
            self.cache_directory().clone(),
        ];
        for dir_name in dirs {
            match fs::create_dir_all(&dir_name) {
                Ok(()) => { 
                    debug!("Directory {} exists", dir_name.to_str().unwrap()); 
                }
                Err(e) => {
                    warn!("Failed to ensure that directory {} exists: {}", dir_name.to_str().unwrap(), e);
                }
            }
        }
        
        // TODO: load settings from file
        
        // Return
        Ok(())
    }
    
    /// Save settings to a file. Returns Err if saving the file failed.
    pub fn save(&self) -> Result<(), &'static str> {
        Ok(()) // TODO
    }
}

/// Substitute ~ on path
fn string_to_path(s: &String) -> path::PathBuf {
    if let Some(home_dir) = env::home_dir() {
        let mut p = home_dir.to_path_buf();
        if s.starts_with("~/") {
            p.push(s[2..].to_string());
            return p;
        } else {
            return path::PathBuf::from(s);
        }
    }
    panic!("No HOME directory available!");
}

/// Unlock settings for read access.
pub fn settings_read<'a>() -> RwLockReadGuard<'a, Settings> {
    SETTINGS.read().unwrap()
}

/// Unlock settings for write access.
pub fn settings_write<'a>() -> RwLockWriteGuard<'a, Settings> {
    SETTINGS.write().unwrap()
}

#[test]
fn test_settings_path() {
    // Test that HOME substitution works as expected
    let p: String = settings_read().cache_directory().to_str().unwrap().into();
    let q = settings_read().cache_directory.replace("~", env::home_dir().unwrap().into_os_string().to_str().unwrap().into());
    println!("p={}", p);
    println!("q={}", q);
    assert!(p == q)
}

