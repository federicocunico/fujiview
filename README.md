# FujiView

FujiView is a fast Windows desktop application for comparing multiple JPEG/TIFF exports of the same photograph side by side. It is designed for large Fujifilm X-T5 images and uses SIMD JPEG decoding, background prefetching, an on-disk preview cache, and GPU-accelerated WebView2 compositing.

## Getting started

### Install from source

Open PowerShell in the repository and run:

```powershell
make install
```

This single command:

1. checks and installs the required Windows development tools;
2. installs the project dependencies;
3. builds the optimized application and NSIS setup executable;
4. removes an existing FujiView installation while preserving application data;
5. installs the new version for the current Windows user.

The generated setup is also available at:

```text
src-tauri/target/release/bundle/nsis/FujiView_<version>_x64-setup.exe
```

### Start a comparison

Organize the exports so that every immediate subfolder represents a preset:

```text
photos/
  Classic Chrome/
    DSCF0001.jpg
    DSCF0002.jpg
  Velvia/
    DSCF0001.jpg
    DSCF0002.jpg
  Acros/
    DSCF0001.tif
    DSCF0002.tif
```

Open FujiView, select `photos`, and start comparing. The preset folder containing the largest number of unique JPEG/TIFF images becomes the guide. Its filenames define the shot list and are sorted naturally, so `DSCF2` appears before `DSCF10`.

RAF, FP2, FP3, and other unsupported files are ignored. A missing export in another preset is shown as an unavailable variant instead of removing the shot from the guide.

## Controls

- Click a comparison panel or press `1`–`4` to select its preset as the winner.
- Press `Enter` or `Right Arrow` to move to the next shot.
- Press `Left Arrow` to return to the previous shot.
- Press `Alt+0`–`Alt+5` to set the rating.
- Double-click an image to toggle fit/zoom.
- Use the mouse wheel to zoom and drag to pan; all panels remain synchronized.
- Use the sidebar to jump directly to another shot or hide it from the toolbar to maximize image space.

The selected winner has a yellow border and a visible winner badge. Selecting a winner does not automatically advance to the next shot.

## Projects and export

FujiView project files use the `.fujiview` extension and store panel choices, winners, ratings, and the last viewed shot. Original images are never modified.

Export copies only the selected winners to the destination folder. The chosen rating is embedded as `xmp:Rating` without re-encoding the original image pixels.

## Performance

- JPEG decoding and preview encoding use statically linked libjpeg-turbo SIMD.
- JPEGs are downscaled during decoding instead of first allocating a complete 40 MP RGB image.
- Interactive previews, adjacent-shot prefetching, and sidebar thumbnails use separate work queues.
- The next four and previous two shots are prefetched in the background.
- Duplicate preview requests are coalesced and obsolete prefetch generations are cancelled.
- Thumbnails reuse an existing 2K preview whenever possible.
- Preview files use a persistent SQLite-indexed LRU cache with a 20 GB limit.
- WebView2 uses GPU acceleration for image compositing, zooming, and panning.

## Development commands

```powershell
make setup       # install/check prerequisites and project dependencies
make doctor      # check prerequisites without installing anything
make dev         # start the Tauri development application
make build       # build the optimized Windows NSIS installer
make install     # setup, build, replace the old version, and install locally
make build-web   # build only the React frontend
make test        # run frontend and Rust tests
make check       # run every non-mutating CI validation
make fmt         # format Rust sources
make clean       # remove generated build output
make distclean   # also remove node_modules
```

`make setup` checks Node.js, Rust MSVC, CMake, NASM, Visual Studio C++ Build Tools, WebView2, and npm dependencies. Missing system prerequisites are installed through WinGet.

If GNU Make is unavailable, the equivalent setup command is:

```powershell
powershell -File scripts/bootstrap.ps1 -InstallMissing
```

## Versioning and releases

[`VERSION`](VERSION) is the canonical human-readable application version. CI verifies that it matches `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json`.

Create a release with:

```powershell
make release VERSION=0.2.0
```

The release script updates every version file, runs the test suite, builds the installer, commits and tags the release, and pushes it to GitHub. A `vX.Y.Z` tag triggers the `Release Windows` workflow, which publishes the setup executable in a GitHub Release.

The installer is not Authenticode-signed yet, so Windows SmartScreen may display a warning.
