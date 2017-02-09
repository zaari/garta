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
extern crate assert;

/// Units of measurement.
#[derive(Copy, Clone, Serialize, Deserialize, Debug)]
pub enum Units {
    Nautical,
    Metric,
    Imperial,
}

impl Units {
    /// Returns distance unit name with the current locale.
    #[inline]
    pub fn distance_unit_name(&self) -> String {
        match *self {
            Units::Nautical => { "M".into() }, // TODO: localization
            Units::Metric => { "km".into() },
            Units::Imperial => { "mi".into() },
        }
    }
    
    /// Converts distance to metres.
    #[inline]
    pub fn distance_to_metres(&self, s: f64) -> f64 {
        match *self {
            Units::Nautical => { s * 1852.0 },
            Units::Metric => { s * 1000.0 },
            Units::Imperial => { s * 1609.344 },
        }
    }
    
    /// Converts distance from metres.
    #[inline]
    pub fn distance_from_metres(&self, s: f64) -> f64 {
        match *self {
            Units::Nautical => { s / 1852.0 },
            Units::Metric => { s / 1000.0 },
            Units::Imperial => { s / 1609.344 },
        }
    }
    
    /// Returns speed unit name with the current locale.
    #[inline]
    pub fn speed_unit_name(&self) -> String {
        match *self {
            Units::Nautical => { "kn".into() }, // TODO: localization
            Units::Metric => { "km/h".into() },
            Units::Imperial => { "mph".into() },
        }
    }
    
    /// Convert speed to metres per second.
    #[inline]
    pub fn speed_to_mps(&self, v: f64) -> f64 {
        match *self {
            Units::Nautical => { v * (1852.0 / 3600.0) },
            Units::Metric => { v * (1000.0 / 3600.0) },
            Units::Imperial => { v * (1609.344 / 3600.0) },
        }
    }
    
    /// Convert speed from metres per second.
    #[inline]
    pub fn speed_from_mps(&self, v: f64) -> f64 {
        match *self {
            Units::Nautical => { v / (1852.0 / 3600.0) },
            Units::Metric => { v / (1000.0 / 3600.0) },
            Units::Imperial => { v / (1609.344 / 3600.0) },
        }
    }
}


// ---- units --------------------------------------------------------------------------------------

#[test]
fn test_units() {
    let nautical = Units::Nautical;
    let metric = Units::Metric;
    let imperial = Units::Imperial;

    // Nautical unit conversions    
    assert_eq!(nautical.distance_to_metres(1.0), 1852.0);
    assert_eq!(nautical.speed_to_mps(1.0), metric.speed_to_mps(1.852));
    assert::close(nautical.distance_from_metres(1852.0), 1.0, 0.000001);
    assert::close(nautical.speed_from_mps(1.852), metric.speed_from_mps(1.0), 0.000001);
    assert_eq!(nautical.distance_unit_name(), "M");
    assert_eq!(nautical.speed_unit_name(), "kn");
    
    // Metric unit conversions    
    assert_eq!(metric.distance_to_metres(1.0), 1000.0);
    assert::close(metric.speed_to_mps(3.6), 1.0, 0.000001);
    assert::close(metric.distance_from_metres(1000.0), 1.0, 0.000001);
    assert::close(metric.speed_from_mps(1.0), 3.6, 0.000001);
    assert_eq!(metric.distance_unit_name(), "km");
    assert_eq!(metric.speed_unit_name(), "km/h");

    // Imperial unit conversions    
    assert_eq!(imperial.distance_to_metres(1.0), 1609.344);
    assert::close(imperial.speed_to_mps(1.0), metric.speed_to_mps(1.609344), 0.000001);
    assert::close(imperial.distance_from_metres(1609.344), 1.0, 0.000001);
    assert::close(imperial.speed_from_mps(1.609344), metric.speed_from_mps(1.0), 0.000001);    
    assert_eq!(imperial.distance_unit_name(), "mi");
    assert_eq!(imperial.speed_unit_name(), "mph");
}

