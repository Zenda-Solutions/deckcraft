# Zenda build of DeckCraft for Work

Based on upstream v0.12.0. Feature `zenda` (on `deckcraft` and `deckcraft-ui-egui`) adds a bounded
`work.events` queue, the orange Theo mascot button and Share through Work. Upstream builds keep stock
behavior. No document format changes. Work owns accounts and sending; the tool emits metadata only.

DeckCraft's control channel has no key, so Work only opens it while Theo works and closes it after.
The copy the person keeps editing has no channel: there Theo and Share run the `zwork.exe` that Work
names in `ZWORK_EXE` with the fixed command `abrir guia` (an absolute path to a file named
`zwork.exe`, or nothing happens).

## Interface language

Upstream v0.12.0 has no translations. `crates/ui-egui/src/i18n.rs` adds one process-wide language:
`DECKCRAFT_LANG` (`en`, `es`, `pt`), then the system language, then English; `app.language.set
{"language": "es"}` switches it over the control channel. `i18n/catalog.rs` holds (English, Spanish,
Brazilian Portuguese) rows for the ribbon, tabs, title bar and quick access toolbar; everything else
falls back to English. Command ids, layout names and document text never change. Tests keep the
language per thread, so upstream's English UI tests are unaffected.

No startup/manual upstream update requests and no Discord links in this build. About says «Basada en
DeckCraft, del equipo de ArtCraft» and shows none of the ArtCraft marks (`docs/brand/LICENSE-brand.txt`).

Theo's image is Work's existing mascot (`app/ui/theo/theo-icon-64.png`), SHA-256
`87770f806ab803f236dd03d152cbce7d12058e3ddc91a0de768c96558cbbbeb4`.

## Build and release

`pwsh zenda/package.ps1 -Z 1` builds `deckcraft.exe` (with `zenda`) and `deckcraft-cli.exe` and packs
`dist/zenda/deckcraft-<version>-z<n>-windows-x64-portable.zip`; inside, the folder keeps upstream's name
so Work's manifest keeps the same `exe`. The `Zenda` workflow tests and packs every push to
`zenda/**`; a tag `zenda-v0.12.0-z<n>` also publishes the zip as a GitHub release with size and SHA-256
in the notes. Verify both before editing Work's manifest.

## Checks

`cargo test -p deckcraft-ui-egui --lib --features zenda zenda` (and `i18n`, `about`; also without the
feature). Set `ZENDA_TREE_DIR` to dump the AccessKit tree in en/es/pt.
