# Garta
Garta is going to be a GPX viewer, analyzer and editor for GNOME 3 desktop environment and written in Rust programming language. The application is still at early development stage and won't be that useful before release 0.1.0.

* [Change Log](ChangeLog.md)
* [Road Map](RoadMap.md)

## Getting started
The following tools are needed to download and compile the application:

* git 
* cargo
* rustc

At first, you have to clone this repository.

```bash
git clone https://github.com/zaari/garta
cd garta
```

The standard cargo commands can be used to run the unit tests, run the application or even build a release.

```bash
export RUST_LOG=warn,garta::core=debug,garta::gui=debug
cargo test
cargo run
cargo build --release
```

## License
Garta is distributed under the terms of the [GNU General Public License (Version 3)](https://www.gnu.org/licenses/gpl-3.0.en.html).

