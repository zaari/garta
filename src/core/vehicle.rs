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

use core::units::{Units};
use core::color::{Color};

/// Vehicle profile.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Vehicle {
    /// Unique identifier of the vehicle
    #[serde(default)]
    pub slug: String,
    
    /// Vehicle name for UI
    #[serde(default)]
    pub name: String,

    /// Symbol filename base. None means general symbol.
    #[serde(default)]
    pub symbol: Option<String>,
    
    /// Units of measurements. None means app default.
    #[serde(default)]
    pub units: Option<Units>,
    
    /// Red, green and blue components of the color. None means app default track color.
    #[serde(default)]
    pub default_color: Option<Color>,

    /// Length of step in multiplies of distance unit. This is used in user interface.
    #[serde(default)]
    pub step: Option<f64>,
}

impl Vehicle {
    /// Construct with slug and name only.
    pub fn new(slug: &'static str, name: &'static str) -> Vehicle {
        Vehicle {
            slug: slug.into(),
            name: name.into(),
            symbol: None,
            units: None,
            default_color: None,
            step: None,
        }
    }

    /// Construct with all parameters.
    pub fn with_all(slug: &'static str, name: &'static str, symbol: Option<String>, units: Option<Units>, default_color: Option<Color>, step: Option<f64>) -> Vehicle {
        Vehicle {
            slug: slug.into(),
            name: name.into(),
            symbol: symbol,
            units: units,
            default_color: default_color,
            step: step,
        }
    }

    /// True if this is "other" vehicle which doesn't fit to other available vehicle types.
    pub fn is_other(&self) -> bool {
        self.slug == ""
    }

    /// Create a list of default vehicles that are saved to users data directory.
    pub fn default_vehicles() -> Vec<Vehicle> {
        vec![
            Vehicle::with_all("boat",     "Boat",     None, Some(Units::Nautical), None, Some(5.0)),
            Vehicle::with_all("kayak",    "Kayak",    None, Some(Units::Nautical), None, Some(2.5)),
            Vehicle::with_all("bicycle",  "Bicycle",  None, None,                  None, Some(10.0)),
            Vehicle::with_all("runner",   "Runner",   None, None,                  None, Some(8.0)),
            Vehicle::with_all("hiker",    "Hiker",    None, None,                  None, Some(3.0)),
            Vehicle::with_all("skiier",   "Skiier",   None, None,                  None, Some(8.0)),
            Vehicle::with_all("aircraft", "Aircraft", None, Some(Units::Nautical), None, Some(500.0)),
            Vehicle::with_all("",         "Other",    None, None,                  None, None),
        ]
    }
}

