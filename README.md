# Garta 
[![Travis Build Status](https://travis-ci.org/zaari/garta.svg?branch=master)](https://travis-ci.org/zaari/garta) 
[![License: GPL v3](https://img.shields.io/badge/License-GPL%20v3-blue.svg)](http://www.gnu.org/licenses/gpl-3.0)

Garta is going to be a GPX viewer, analyzer and editor for [GTK+ 3](http://www.gtk.org/) toolkit (used by e.g. [GNOME 3 desktop environment](https://www.gnome.org/gnome-3/)) and is written in Rust programming language. The application will soon reach version [0.1.0](https://github.com/zaari/garta/milestone/1) which allows you to explore tile-based world maps. GPX loading will be implemented for release [0.2.0](https://github.com/zaari/garta/milestone/2) and saving for release 0.3.0.

* [Road Map](RoadMap.md)
* [Change Log](ChangeLog.md)

## Getting started
The application has the following minimum dependencies at moment:

* git 
* rust **1.15** (rustc and cargo)
* gtk **3.16**

At first, you have to clone this repository.

```bash
git clone https://github.com/zaari/garta
cd garta
```

If you want to get some debug from garta internals, you can configure the env_logger. The following enables warnings in any Rust module and info messages in any Garta module, but then allows debug level messages from wanted parts.

```bash
export RUST_LOG=warn,garta=info,garta::gui=debug
```

The standard cargo commands can be used to run the unit tests, run the application in debug mode or even build a release. A release build performs noticeably smoother than a debug build.

```bash
cargo test
cargo run
cargo build --release
```

