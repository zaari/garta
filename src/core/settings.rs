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

extern crate regex;
extern crate num_cpus;
extern crate serde_json;
extern crate hyper;

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::fs;
use std::cmp::{min, max};
use std::env;
use std::path;
use self::hyper::client::{Client};
use self::regex::{Regex};

/// Default number of days until tiles expire if the server doesn't send expiration information.
pub static DEFAULT_TILE_EXPIRE_DAYS: i64 = 7;

/// Minimum number of worker threads in case of auto detection.
static MIN_WORKER_THREADS: i32 = 2;

/// Maximum number of worker threads in case of auto detection.
static MAX_WORKER_THREADS: i32 = 8;

/// Application name
pub static APP_NAME: &'static str = "Garta";

/// Application version from Cargo.toml file.
pub static APP_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub static APP_VERSION_MAJOR: &'static str = env!("CARGO_PKG_VERSION_MAJOR");
pub static APP_VERSION_MINOR: &'static str = env!("CARGO_PKG_VERSION_MINOR");

/// Default maximum zoom level for maps that don't specify this information.
pub fn default_max_zoom_level() -> u8 { 16 }

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
    
    /// HTTP read timeout when fetching tiles from network sources.
    pub tile_read_timeout: u64,
    
    /// HTTP write timeout when sendinf requests.
    pub tile_write_timeout: u64,

    /// Automatic proxy settings.
    pub http_proxy_auto: bool,
    
    /// HTTP proxy hostname,
    pub http_proxy_host: Option<String>,
    
    /// HTTP proxy port.
    pub http_proxy_port: Option<u16>,
    
    /// Number of times to try reloading HTTP resources.
    pub http_retry_count: u8,
    
    // Tile memory cache size in bytes. If no limits are wanted this value should be set to None.
    pub tile_mem_cache_capacity: Option<isize>,
    
    // Tile disk size in bytes. If no limits are wanted this value should be set to None.
    pub tile_disk_cache_capacity: Option<i64>,
    
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
            http_proxy_auto: true,
            http_proxy_host: None,
            http_proxy_port: None,
            tile_mem_cache_capacity: Some(256 * 1024 * 1024),
            tile_disk_cache_capacity: Some(1000 * 1024 * 1024),
            main_window_geometry: "".to_string(),
            browser_command: "xdg-open".into(),
        }
    }

    /// Get project/atlas directory
    pub fn project_directory(&self) -> path::PathBuf { 
        string_to_path(&self.user_data_directory)
    }

    /// Get ui files directory
    pub fn ui_directory_for(&self, filename: &'static str) -> path::PathBuf { 
        let mut pb = string_to_path(&self.host_data_directory); 
        pb.push("ui"); // TODO: test if a ui directory exists at current working directory
        pb.push(filename);
        pb 
    }

    /// Return a list of directories where to try load map json files.    
    pub fn map_directories(&self) -> Vec<path::PathBuf> {
        vec![
            { let mut pb = string_to_path(&self.host_data_directory); pb.push("maps"); pb  },
            { self.user_maps_directory() },
            { let mut pb = path::PathBuf::from("."); pb.push("maps"); pb },
        ]
    }
    
    /// Return a list of directories where to try load token json files.    
    pub fn token_directories(&self) -> Vec<path::PathBuf> {
        vec![
            { let mut pb = string_to_path(&self.host_data_directory); pb.push("maps"); pb.push("tokens"); pb  },
            { self.user_tokens_directory() },
            { let mut pb = path::PathBuf::from("."); pb.push("maps"); pb.push("tokens"); pb },
        ]
    }
    
    /// Get user's maps directory
    pub fn user_maps_directory(&self) -> path::PathBuf { 
        let mut pb = string_to_path(&self.config_directory); pb.push("maps"); pb 
    }

    /// Get user's tokens directory
    pub fn user_tokens_directory(&self) -> path::PathBuf { 
        let mut pb = string_to_path(&self.config_directory); pb.push("maps"); pb.push("tokens"); pb 
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
    pub fn cache_directory(&self) -> path::PathBuf { 
        assert_ne!(&self.cache_directory, "");
        string_to_path(&self.cache_directory) 
    }
    
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

    /// Create a new HTTP client with or without a proxy.    
    pub fn http_client(&self) -> Client {
        // Use environment HTTP proxy settings if automatic settings are wanted
        let (http_proxy_host, http_proxy_port) = {
            if self.http_proxy_auto {
                match env::var("http_proxy") {
                    Ok(var) => {
                        // Parse the environment variable
                        let re = Regex::new(r"http://([a-z0-9\\-\\.]+):([0-9]+)").unwrap();
                        if let Some(cap) = re.captures(var.as_str()) {
                            let hostname = &cap[1].to_string();
                            let port = &cap[2].parse().unwrap_or(80);
                            debug!("Auto-proxy wanted and environment http_proxy used: {}:{}", hostname, port);
                            (Some(hostname.clone()), Some(*port))
                        } else {
                            error!("Auto-proxy wanted but environment variable http_proxy has unexpected syntax: {}", var);
                            (None, None)
                        }
                    },
                    Err(e) => {
                        debug!("Auto-proxy wanted but no proxy environment variables available");
                        (None, None)
                    }
                }
            } else {
                if self.http_proxy_host.is_some() && self.http_proxy_port.is_some() {
                    debug!("No auto-proxy wanted. Returning {}:{}", 
                        self.http_proxy_host.clone().unwrap(), 
                        self.http_proxy_port.unwrap());
                    (self.http_proxy_host.clone(), self.http_proxy_port)
                } else {
                    debug!("No auto-proxy wanted, no proxy defined.");
                    (None, None)
                }
            }
        };

        // Create an HTTP client
        if let Some(ref host) = http_proxy_host {
            if let Some(port) = http_proxy_port {
                return Client::with_http_proxy(host.clone(), port)
            }
        }
        Client::new()
    }
    
    /// Return HTTP User Agent header to be used.
    pub fn user_agent_header(&self) -> String {
        // TODO: this can be simplified after reaching version 0.1.0
        if APP_VERSION_MAJOR == "0" && APP_VERSION_MINOR == "0" {
            format!("{}/{} (+https://github.com/zaari/garta)", APP_NAME, APP_VERSION)
        } else {
            format!("{}/{}.{} (+https://github.com/zaari/garta)", 
                APP_NAME, APP_VERSION_MAJOR, APP_VERSION_MINOR)
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
        Ok(()) // TODO: save settings to file
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

