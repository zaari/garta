# Garta
Garta is a GPX editor for GNOME 3 desktop environment.

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
export RUSTFLAGS="$RUSTFLAGS -A dead_code -A unused_variables"
cargo test
cargo run
cargo build --release
```

## License
Garta is distributed under the terms of the [GNU General Public License (Version 3)](https://www.gnu.org/licenses/gpl-3.0.en.html).

