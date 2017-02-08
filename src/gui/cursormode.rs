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

extern crate gdk;
extern crate gdk_sys;

#[derive(Debug, PartialEq)]
pub enum CursorMode {
    Unset,
    Draggable,
    Dragging,
    Target,
    Wait,
}

/// Manages current cursor state and the needed actions on transition.
pub struct CursorKeeper {
    current_mode: CursorMode,
}

impl CursorKeeper {
    /// Construct a new mode keeper with unset mode.
    pub fn new() -> CursorKeeper {
        CursorKeeper {
            current_mode: CursorMode::Unset,
        }
    } 

    /// Change curser of the widget if the new mode is different from the current one.
    #[inline]
    pub fn change_for(&mut self, new_mode: CursorMode, gdk_win: Option<gdk::Window>) {
        if self.current_mode != new_mode {
            if let Some(display) = gdk::Display::get_default() {
                // Map mode to cursor type
                let cursor_type = match new_mode {
                    CursorMode::Unset     => { gdk_sys::GdkCursorType::Hand1 },
                    CursorMode::Draggable => { gdk_sys::GdkCursorType::Hand1 },
                    CursorMode::Dragging  => { gdk_sys::GdkCursorType::Fleur },
                    CursorMode::Target    => { gdk_sys::GdkCursorType::Crosshair },
                    CursorMode::Wait      => { gdk_sys::GdkCursorType::Watch },
                };
                
                // Get cursor for display
                let cursor = gdk::Cursor::new_for_display(&display, cursor_type);
                
                // Set cursor
                if let Some(ref gdk_win) = gdk_win {
                    gdk_win.set_cursor(&cursor);
                } else {
                    warn!("No widget for cursor");
                }
            } else {
                warn!("No display for cursor");
            }

            // Remember mode change            
            self.current_mode = new_mode;
        }
    }
}

