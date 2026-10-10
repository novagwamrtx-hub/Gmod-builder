# GMod Collection Builder

Build Garry's Mod Workshop collections without the clicking: find addons for what you are building, assemble a list, check it for problems, push it into a real Steam collection, and export the files a server needs. Written in Rust.

It has two parts:

- **Desktop app** – discover, search, build a draft, check it, export.
- **Browser extension** – adds the draft's addons to your Steam collection (and removes ones you dropped). Steam has no API for editing collections, so this has to happen in a browser where you are logged in.

## Using it

1. Open the app. In **Settings**, get a [Steam Web API key](https://steamcommunity.com/dev/apikey) (free; any domain name works). Copy it and the app offers to use it. Discovering, searching and required-item checks need the key; importing and inspecting collections do not.
2. **Discover**: say what you are building, or pick one of the 277 ideas that drop down from the search field (type to narrow them), or paste an addon you want to build around. The app reads the 300 best-rated collections that match, keeps the ones that really are about your theme, and shows the addons they have in common, grouped by type. Addons that are in every collection of any kind are ranked down. Answers are kept for three days, so asking again is instant.
3. **Search** the Workshop by name and tags (right-click a tag to exclude it), or switch to *Collections* and open one to pick addons out of it. Click an addon's name anywhere for details, what it requires, what is built for it, and more from its author.
4. **Draft** is your list. It saves itself, and *Drafts* switches between the ones you have. Adding an addon also adds what it requires. *Maybe* keeps candidates aside without adding them. Tick several addons to remove them or move them to the maybe pile in one go. Ctrl+Z undoes. You can import a collection, a pasted list of addon links, or everything Garry's Mod has downloaded on this computer.
5. **Check** flags removed, banned or private addons, missing required items, several gamemodes, addons made for a different gamemode than you said the collection is for, and very old addons. For addons Garry's Mod has downloaded on this computer it also reads their files and reports addons that overwrite each other. From the second check on, it lists what changed since last time: which addons were updated, removed or came back. *Ignore* silences a finding for good.
6. **Export → Send to Steam**: create a collection on Steam (or use one you have), paste its link and press the button. Your browser opens the collection's edit page and the extension's panel shows what it will add and what the collection has that your draft does not.
7. For a server, **Export** saves `workshop.lua` (goes in `garrysmod/lua/autorun/server/`) and copies the `+host_workshop_collection` launch option. Where the addon files are on this computer, `workshop.lua` leaves out script-only addons and sends each map only while the server is on it. It also copies the addon list, grouped by type, formatted for a Steam description or for Discord.

### Vouches

In an addon's details (and on a collection you have opened in Search) you can **Vouch** for it or **Warn** about it. Once at least three builders have voted on something it gets a *vouch score* from 0 to 100: the share who vouched, marked down while few have voted, so 3 out of 3 scores 44 and 30 out of 30 scores 89.

Your votes are kept on your computer. *Share my vouches* in Settings opens a filled-in issue on this project's GitHub page; sending it needs a GitHub account, and each account counts once per addon however often it shares. A workflow adds the votes to `community/ledger.json` and republishes `community/scores.json`, which the app downloads when it starts. There is no server.

### Making it yours

Settings has seven themes (the app's own, One Dark, Dracula, Catppuccin Mocha, Tokyo Night, Gruvbox and Nord), a colour picker for every colour, and controls for size, row height, corner roundness, the width of the detail panel, animations and thumbnails. It also sets how many collections Discover reads, how many suggestions it shows per type, and how old an addon must be for Check to flag it.

### The extension on its own

- On a Garry's Mod addon's Workshop page it adds an **Add to basket** button. Open any of your collections' edit pages and the basket is waiting in the panel.
- On a collection's edit page, **Use my subscribed items** loads everything you are subscribed to, to add in one go or to copy into the app.

## Building

Needs [Rust](https://rustup.rs).

```bash
cargo run --release -p gmcb-desktop
```

The extension also needs the WASM target and wasm-pack:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
wasm-pack build crates/extension --target web --release --no-typescript
```

Then load `crates/extension` as an unpacked extension (Chrome: `chrome://extensions` → Developer mode → Load unpacked; Firefox: `about:debugging` → This Firefox → Load Temporary Add-on → pick `manifest.json`).

## Layout

| Crate | What it is |
| --- | --- |
| `crates/core` | Steam Web API client, discovery ranking, collection checks, `.gma` reader, exports, vouch scoring. Shared by both programs. |
| `crates/desktop` | The app (egui). |
| `crates/extension` | The extension. Rust compiled to WebAssembly, plus the `manifest.json` and a small `loader.js` that browsers require. |

```bash
cargo test --workspace
```

Two things help when working on it without clicking through everything:

- `cargo test -p gmcb-desktop render_previews -- --ignored --nocapture` drives the app through its main flows against the live Workshop and writes a screenshot of every screen to `target/ui-preview/`. It needs a saved Steam key and does not touch your drafts.
- `crates/extension/dev/harness.html` and `dev/filedetails.html` run the extension on stand-in pages, outside Steam. Serve `crates/extension` and open `/dev/harness.html?id=1#gmcb=10,20,30`.

### Turning on shared vouches

Sharing is off until the app knows where the project lives. After putting the project on GitHub, set `COMMUNITY_REPO` in `crates/desktop/src/app.rs` to `"owner/name"`, and in the repository's settings allow GitHub Actions to write (Settings → Actions → General → Workflow permissions → Read and write). `.github/workflows/vouches.yml` does the rest. The repository's default branch must be `main`.

## Notes

- The extension talks only to `steamcommunity.com`, using the session you are already logged in with. It uses the same `addchild` and `removechild` requests as the page's own buttons; they are undocumented and Steam may change them. Removing asks for a second click.
- A discovery costs about 85 Steam Web API requests; Steam allows 100,000 a day per key.
- The ideas were each tried against the Workshop in October 2026 and kept only if enough of the best-rated matching collections were on the theme.
- Vouches shared through GitHub are public, with the account that sent them.
- Conflict detection and the trimmed `workshop.lua` only cover addons whose files are in `garrysmod/cache/workshop` on this computer.
- The API key is stored unencrypted in the app's settings file. Treat it like a password.
