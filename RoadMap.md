# Garta Roadmap
This is the development roadmap which is subject to change depending on the phase of moon, weather, mood, fashion trends, etc. See the [change log](ChangeLog.md) for more information about the current state. The items needed for the upcoming build are copied to Github [Issue](https://github.com/zaari/garta/issues) and [Milestone](https://github.com/zaari/garta/milestones) tracker.

## Version 0.2
- refactor MapCanvas::draw (for modularity, maintainability, readability, simplicity)
- GPX loading
- units of measurement (nautical, metric, imperal)
- vehicles
- track rendering
- track statistics
- keyboard controls
- transparent map layers
- layers dialog
- full data persistence

## Version 0.3
- track editing
- route planning
- waypoints
- GPX saving
- coordinates module relicensing and moving to a separate repository and also published at crates.io

## Version 0.4
- attractions (a.k.a. geo-bookmarks)
- drag & drop

## Version 0.5
- internationalization, gettext (contributors needed)

## Version 0.6
- maps dialog
- HiDPI tile support

## Version 0.7
- vehicles dialog
- track replay

## Version 0.8
- find locations by name, and other possible meta queries
- settings dialog
- settings persistence

## Version 1.0
- stable file formats and directory structure
- polished error handling 
- removing debugging from stable parts of the code

## Non-Goals
- OpenStreetMap data editing (there is [JOSM](https://josm.openstreetmap.de/) for that)
- street-based route planning (at least not at moment; [Google Maps](https://www.google.com/maps), [Gnome Maps](https://wiki.gnome.org/Apps/Maps) and numerous mobile apps do that well already)

