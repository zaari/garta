# Garta Roadmap
This is an open-ended development roadmap for Garta and subject to change any time. See the [change log] for more information about the past changes.

## Version 0.2
- GPX loading **[50%]**
- track rendering
- track statistics
- layers dialog
- transparent map layers
- zooming with keyboard **[100%]**
- application icon and sample .desktop file **[90%]**
- units of measurement (nautical, metric, imperial) **[100%]**
- vehicles **[80%]**
- full data persistence **[50%]**
- https support **[100%]**
- MapCanvas::draw refactored (for better modularity and maintainability)

## Version 0.3
- route planning
- track editing
- waypoints
- GPX saving
- coordinates module relicensed and moved to a separate repository and also published at crates.io

## Version 0.4
- attractions (a.k.a. geo bookmarks) **[10%]**
- drag & drop
- man page
- installer and packaging

## Version 0.5
- internationalization, gettext (contributors needed)

## Version 0.6
- maps dialog
- HiDPI tile support

## Version 0.7
- vehicles dialog
- track replay
- fullscreen mode
- sunrise/sunset awareness

## Version 0.8
- locations search by name, and other possible meta queries
- settings dialog
- settings persistence **[20%]**

## Version 1.0
- stable file formats and directory structure
- polished error handling and code in general
- debugging removed from stable parts of the code

## Post-1.0
- KML import and export
- GeoRSS, GeoJSON, GeoURI and GeoTagging
- collaborative layers editing
- vector tile maps
- SVG export
- integration with sports web services
- option for globe-wide offline maps

## Non-Goals
- OpenStreetMap data editing (there is [JOSM] for that)
- street-based routing ([Google Maps], [Gnome Maps] and especially numerous smartphone apps do that pretty well – or if you are concerned about your [privacy] the traditional paper maps still exist)

[change log]: CHANGELOG.md
[JOSM]: https://josm.openstreetmap.de
[Google Maps]: http://maps.google.com
[Gnome Maps]: https://wiki.gnome.org/Apps/Maps 
[privacy]: https://www.privacytools.io/

