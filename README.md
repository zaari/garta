# Garta &emsp; [![Travis Build Status](https://travis-ci.org/zaari/garta.svg?branch=master)](https://travis-ci.org/zaari/garta) [![License: GPL v3](https://img.shields.io/badge/License-GPL%20v3-blue.svg)](http://www.gnu.org/licenses/gpl-3.0)

This is a geography application for [GTK+ 3] toolkit (used by e.g. [GNOME 3 desktop environment] on [GNU/Linux operating system]) and written in [Rust programming language]. Since release [0.1] Garta allows you to explore tile-based world maps. GPX loading will be implemented with release [0.2] and saving with release 0.3. More future goals can be found in the roadmap, and the current state in the change log.

* [Road Map]
* [Change Log]

![Garta 0.1](https://cloud.githubusercontent.com/assets/8877215/22755750/2684e262-ee4d-11e6-940d-eb54b5a9b03b.png)

## Installing Garta
There are no installer or installation packages provided yet but you can build and run Garta fairly easily on Linux.

## Building and running
The application has the following minimum requirements at moment:

* git 
* rustc **1.15**
* cargo **0.16**
* gtk **3.16**

At first, you have to clone the repository. Development is done on master branch and the releases are tagged.

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
./target/release/garta
```

[GTK+ 3]: http://www.gtk.org/
[GNOME 3 desktop environment]: https://www.gnome.org/gnome-3/
[GNU/Linux operating system]: https://en.wikipedia.org/wiki/Linux
[Rust programming language]: https://www.rust-lang.org/en-US/
[0.2]: https://github.com/zaari/garta/milestone/2
[0.1]: https://github.com/zaari/garta/releases/tag/v0.1.0
[Road Map]: ROADMAP.md
[Change Log]: CHANGELOG.md

