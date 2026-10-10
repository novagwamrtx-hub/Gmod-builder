# Working on it

Needs [Rust](https://rustup.rs). The extension also needs the WebAssembly target and wasm-pack:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

On Linux the app links against the system's windowing libraries. On Debian and Ubuntu:

```bash
sudo apt-get install libgtk-3-dev libxkbcommon-dev libwayland-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

## Checks

CI runs these, so run them before sending a change:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Seeing a change without clicking through everything

```bash
cargo test -p gmcb-desktop render_previews -- --ignored --nocapture
```

drives the app through its main flows against the live Workshop and writes a picture of every screen to `target/ui-preview/`, without opening a window. It needs a GPU and a Steam key saved in the app, and leaves your drafts, settings and vouches alone.

`crates/extension/dev/harness.html` and `dev/filedetails.html` run the extension on stand-in pages, outside Steam. Build the extension, serve `crates/extension`, and open `/dev/harness.html?id=1#gmcb=10,20,30`.

## Where things are

| Where | What is in it |
| --- | --- |
| `crates/core/src/steam` | The Steam Web API client. |
| `crates/core/src` | Everything else both programs share: `discover` ranks addons, `analysis` checks a collection, `gma` reads addon files, `export` writes server files and descriptions, `handoff` is the link between app and extension, `community` is vouches, `updates` is the check for a newer version. |
| `crates/desktop/src/app` | What the app knows and does, by subject: `search`, `discover`, `draft`, `check`, `vouches`. |
| `crates/desktop/src/ui` | What it draws: a file per tab, `detail` for the panel beside them, `widgets` for the rows they share, `theme` for colours and sizes. |
| `crates/desktop/src` | `gmod` finds the game and its downloaded addons, `thumbs` loads thumbnails, `files` writes the app's own files safely, `settings` is what is kept between runs, `preview` is the test above. |
| `crates/extension/src` | `manage` is the panel on a collection's edit page, `basket` the button on an addon's page, `logic` what both can do without a browser. |
| `.github/workflows` | `ci` checks every push, `release` builds the downloads from a tag, `vouches` counts shared vouches. |

## The icon

`assets/icon.svg` is the master, with `icon-small.svg` for 32 pixels and under. The PNG files are exported from them with [resvg](https://github.com/linebender/resvg):

```bash
resvg assets/icon.svg assets/icon-256.png -w 256
```

`icon.ico` is packed from exports at 16, 20, 24, 32, 40, 48, 64 and 256 pixels.

## Releasing

1. Set the new version in `Cargo.toml` (under `[workspace.package]`) and in `crates/extension/manifest.json`. Run `cargo check` so that `Cargo.lock` follows, and commit.
2. Tag that commit with `v` and the version, and push the tag:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

`release.yml` builds the Windows and Linux programs and the extension, and publishes them once they are attached. It refuses a tag that is not the version, because the app compares the two to tell whether a newer version is out.
