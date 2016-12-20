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

use std::sync::{Mutex};

lazy_static! {
    // Unique id for all the elements
    static ref UNIQUE_ID: Mutex<u64> = Mutex::new(0u64);
}

pub type UniqueId = u64;
pub const NONE: u64 = 0;

/// Generate the next unique id for model elements.
pub fn next_id() -> u64 {
    let mut uid = UNIQUE_ID.lock().unwrap();
    *uid += 1;
    if *uid == 0 { panic!("Id overflow!"); } // TODO: string or big num, maybe?
    *uid
}

