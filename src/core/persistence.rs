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

extern crate serde;
extern crate serde_json;
extern crate chrono;

use std::io;
use std::fs;
use std::path;
use std::fmt;
use self::chrono::{DateTime, UTC};

/// Loads all JSON elements from the given directory and sends them to closure 'handle_element'.
/// Doesn't recurse subdirectories.
pub fn deserialize_all<P, T, F>(dir: P, handle_element: F) -> Result<(), io::Error>
    where P: AsRef<path::Path>,
          T: serde::Deserialize,
          F: Fn(T, &String),
{
    if dir.as_ref().is_dir() {
        for entry_ in fs::read_dir(dir)? {
            let entry = entry_?;
            let file_type = entry.file_type()?;
            if file_type.is_file() || file_type.is_symlink() {
                let pathbuf = entry.path();
                let filename_ = entry.file_name();
                let filename = filename_.to_str().unwrap_or("");
                if let Some(stem_osstring) = pathbuf.clone().file_stem() {
                    match stem_osstring.to_os_string().into_string() {
                        Ok(stem) => {
                            if filename.ends_with(".json") {
                                let elem: T = deserialize_from(pathbuf)?;
                                handle_element(elem, &stem);
                            }
                        },
                        Err(e) => {
                            warn!("Failed to read element because filename stem converion failed");
                        }
                    }
                }
            }
        }
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::NotFound, format!("Path {} is not a directory", 
            dir.as_ref().to_str().unwrap_or("???"))))
    }
}

/// Loads a single element from the given JSON file.
pub fn deserialize_from<T, P>(filename: P) -> Result<T, io::Error>
    where T: serde::Deserialize,
          P: AsRef<path::Path>,
{
    let f = fs::File::open(&filename)?;
    match serde_json::from_reader(f) {
        Ok(element) => { Ok(element) },
        Err(e) => {
            match e {
                serde_json::error::Error::Syntax(error_code, a, b) => {
                    Err(io::Error::new(io::ErrorKind::Other, format!("Syntax error in file {} (error code {})", 
                        filename.as_ref().to_str().unwrap_or("???"), error_code)))
                }
                _ => {
                    Err(io::Error::new(io::ErrorKind::Other, format!("Unknown error when deserializing an element from {}", 
                        filename.as_ref().to_str().unwrap_or("???"))))
                }
            }
        }
    }
}

/// Saves a single element to JSON file. Try create needed directories if they don't exist already.
pub fn serialize_to<T, P>(element: &T, filename: P) -> Result<(), io::Error> 
    where T: serde::Serialize,
          P: AsRef<path::Path>,
{
    // TODO: create directories

    let mut f = fs::File::create(&filename)?;
    match serde_json::to_writer_pretty(&mut f, element) {
        Ok(()) => { Ok(()) },
        Err(e) => {
            Err(io::Error::new(io::ErrorKind::Other, format!("Error while serializing element to {}", 
                filename.as_ref().to_str().unwrap_or("???"))))
        }
    }
}

/// Removes element file. Also removes empty directories on the path.
pub fn remove<P>(filename: P) -> io::Result<()> 
    where P: AsRef<path::Path>,
{
    fs::remove_file(filename)?;
    
    // TODO: clean dirs
    
    Ok(())
}

/// Combines name and ext into a new unique filename in the given directory.
/// Possibly adds a number.
pub fn make_filename<P, S>(dir: P, name: S, ext: S) -> path::PathBuf
    where P: AsRef<path::Path>,
          S: fmt::Display,
{
    let mut i = 1;
    let safe_name = make_safe_name(&name);
    loop {
        let p = dir.as_ref().to_path_buf();
        if i < 2 {
            p.join(format!("{}.{}", safe_name, ext));
        } else {
            p.join(format!("{}-{}.{}", safe_name, i, ext));
        }
        if !p.exists() { 
            return p; 
        }
        i += 1;
    }
}

/// Converts the name into file-system-safe (but still readable) name by mapping
/// characters to safe ones.
fn make_safe_name<S>(name: &S) -> String 
    where S: fmt::Display,
{
    // Iterate through the name are replace all the unsafe characters with safe ones.
    let name_string = name.to_string();
    let mut s = String::with_capacity(name_string.len() + 3);
    for ch in name_string.chars() {
        for c in ch.to_lowercase() {
            // https://en.wikipedia.org/wiki/Latin_alphabets
            if "abcdefghijklmnopqrstuvwxyz1234567890+-".contains(c) { s.push(c);
            } else if " &/=ʔ".contains(c) { s.push('-');
            } else if ".`''\"~^*?!¿".contains(c) { 
            } else if "áàâäǎăāãåǻąa̧ɑ".contains(c) { s.push('a');
            } else if "ɓ".contains(c) { s.push('b');
            } else if "ćċĉčç".contains(c) { s.push('c');
            } else if "ďḍɗɖð".contains(c) { s.push('d');
            } else if "éèėêëěĕēẹęȩə̧ɛ̧ǝəɛ".contains(c) { s.push('e');
            } else if "ƒ".contains(c) { s.push('f');
            } else if "ġĝǧğģɣ".contains(c) { s.push('g');
            } else if "ĥḥħɦ".contains(c) { s.push('h');
            } else if "íìiîïǐĭīĩịįi̧ɨɨ̧ıɩ".contains(c) { s.push('i');
            } else if "ĳ".contains(c) { s.push_str("ij");
            } else if "ĵ".contains(c) { s.push('j');
            } else if "ķƙĸ".contains(c) { s.push('ķ');
            } else if "ĺļľŀł".contains(c) { s.push('l');
            } else if "ŉńn̈ňñņɲ".contains(c) { s.push_str("n");
            } else if "óòôöǒŏōõőọǿơǫo̧øơɔ̧ɔʊ".contains(c) { s.push('o');
            } else if "ŕřŗɍ".contains(c) { s.push('r');
            } else if "śŝšş".contains(c) { s.push('s');
            } else if "ťțṭţŧ".contains(c) { s.push('t');
            } else if "úùûüǔŭūũűůụųu̧ưʉ".contains(c) { s.push('u');
            } else if "ʋ".contains(c) { s.push('v');
            } else if "ẃẁŵẅƿ".contains(c) { s.push('w');
            } else if "ýỳŷÿȳỹy̨ƴȝ".contains(c) { s.push('y');
            } else if "źżžẓ".contains(c) { s.push('z');
            } else if "ſß".contains(c) { s.push_str("ss");
            } else if "æǽǣ".contains(c) { s.push_str("ae");
            } else if "œ".contains(c) { s.push_str("oe");
            } else if "þ".contains(c) { s.push_str("th");
            } else if "ŋ".contains(c) { s.push_str("ng");
            } else {
                s.push('_');
            }
        }
    }
    s
}

/// Serializer for chrono::DateTime
pub fn serialize_datetime<S>(dt: &DateTime<UTC>, f: &mut S) -> Result<(), S::Error> 
        where S: serde::Serializer,
{
    let s = dt.to_rfc3339();
    f.serialize_str(s.as_str())?;
    Ok(())
}

/// Deserializer for chrono::DateTime
pub fn deserialize_datetime<D>(f: &mut D) -> Result<DateTime<UTC>, D::Error> 
        where D: serde::Deserializer
{
    let s: String = serde::Deserialize::deserialize(f)?;
    let utc = UTC::now();
    match DateTime::parse_from_rfc3339(s.as_str()) {
        Ok(dt_tz) => { 
            Ok(dt_tz.with_timezone(&utc.timezone()))
        }
        Err(e) => {  
            Err(serde::de::Error::custom(e.to_string()))
        }
    }
}

