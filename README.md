# Garta
Garta is a GPX editor for GNOME 3 desktop environment.

* [change log](ChangeLog.md)
* [Road map](RoadMap.md)

## Getting started
The following tools are needed to download and compile Garta:

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
export RUSTFLAGS="$RUSTFLAGS -A dead_code"
cargo test
cargo run
cargo build --release
```

## License
Garta is distributed under the terms of the [Apache License (Version 2.0)](https://www.apache.org/licenses/LICENSE-2.0).

