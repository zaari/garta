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

extern crate gtk;

pub use self::mapcanvas::*;
pub mod mapcanvas;

pub use self::cursormode::*;
pub mod cursormode;

pub use self::mainwindow::*;
pub mod mainwindow;

pub use self::floatingtext::*;
pub mod floatingtext;

pub use self::sprite::*;
pub mod sprite;

/// Run main loop
pub fn main() {
    // Start GTK main
    gtk::main();
}

