# Garta
Garta is going to be a GPX viewer, analyzer and editor for GTK3 toolkit (used by e.g. GNOME 3 desktop environment) and is written in Rust programming language. The application is still at a pretty early development stage and won't be that useful before the release 0.1.0.

* [Change Log](ChangeLog.md)
* [Road Map](RoadMap.md)

## Getting started
The following tools are needed to download and compile the application:

* git 
* cargo
* rustc **v1.15** (*nightly* channel until Feb 2017)
* gtk3

At first, you have to clone this repository.

```bash
git clone https://github.com/zaari/garta
cd garta
```

If you want to get some debug from garta internals, you can configure the env_logger. The following enables warnings in any Rust module and info messages in any Garta module, but then allows debug level messages from wanted parts.

```bash
export RUST_LOG=warn,garta=info,garta::core::tiles=debug
```

The standard cargo commands can be used to run the unit tests, run the application or even build a release.

```bash
cargo test
cargo run
cargo build --release
```

## License
Garta is distributed under the terms of the [GNU General Public License (Version 3)](https://www.gnu.org/licenses/gpl-3.0.en.html).

