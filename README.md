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

## Generating variants from the camera

FujiView can produce the preset variants itself by sending your Fujifilm RAF files to the camera over USB and letting the camera render each one with its own custom presets (C1-C7) plus an "as shot" version.

Camera setup:

1. On the camera open `MENU > SET UP > CONNECTION SETTING > USB MODE` and choose `USB RAW CONV./BACKUP RESTORE`.
2. Disable auto power off so the camera does not sleep during a long batch.
3. Close X RAW Studio (and any other software using the camera).
4. All RAF files must come from the same camera body as the one connected.

Connect the camera, click "Genera varianti dalla camera", pick a RAF folder or individual files, choose the presets, and start. Before starting, FujiView shows the number of conversions, the estimated disk space against the free space (generation is blocked if there is not enough), and an estimated time. Typical timings on an X-T5 (firmware 4.31) with 87 MB uncompressed RAFs: about 2.3 s to upload each RAF, then about 1.5–2.5 s per variant. A batch of 5 RAFs × 8 presets took 2.4 s per variant on average, and each full-size JPEG is about 26 MB.

Output layout:

```text
<source>/FujiView varianti/
  <C1 name>/DSCF0001.jpg
  <C2 name>/DSCF0001.jpg
  Come scattato/DSCF0001.jpg
```

Variants already present are skipped, so an interrupted or paused job can be resumed without redoing finished work; enable "Sovrascrivi varianti esistenti" to regenerate them. Original RAF files are never modified. Generation pauses if free disk space runs out or the camera disconnects, and can be resumed afterwards. When finished, open the output folder directly for comparison.

For development, set `FUJIVIEW_FAKE_CAMERA=1` to enable a simulated camera.

## Controls

- The comparison shows one panel per preset (up to 9) and adapts the grid to their number.
- Both sidebars are open by default and can be toggled from the toolbar; their state is remembered. The right-hand sidebar has collapsible sections:
  - **Preset**: show or hide each preset (saved in the project), with each variant's film simulation, DR and white balance read from its Fujifilm MakerNote (hover for the full recipe).
  - **Scatto / Fotocamera / Luogo / File**: date and time, shutter speed, aperture, ISO, exposure compensation, focal length, exposure program, metering, flash, shutter type, camera, lens, firmware, GPS coordinates when present, pixel size and file size.
- Click a comparison panel, press arrow keys to move across the grid, or press `Ctrl+1`–`Ctrl+9` to select its preset as the winner.
- Press `Space` to move to the next shot; `Shift+Space` to return to the previous shot.
- Press `1`–`5` to set the rating, or `0` to clear it.
- Double-click an image to toggle fit/zoom.
- Use the mouse wheel to zoom and drag to pan; all panels remain synchronized.
- Use the sidebar to jump directly to another shot or hide it from the toolbar to maximize image space.

The selected winner has a yellow border and a visible winner badge. Selecting a winner does not automatically advance to the next shot.

## Projects and export

FujiView project files use the `.fujiview` extension and store panel choices and order, hidden presets, winners, ratings, and the last viewed shot. Original images are never modified.

Everything is saved automatically, with no need to press Save:

- Each opened folder has its own project file in `%APPDATA%\com.fujiview.desktop\projects\` (never inside the photo folders). Reopening the same folder restores its winners, ratings, filters, and last shot.
- App preferences (sidebar visibility, the last opened folder or project, the presets ticked in the variant generator) live in `settings.json` in the same folder. At startup FujiView reopens the last folder where you left off.
- The Save button additionally writes the project to a `.fujiview` file of your choice, which is then kept up to date too.
- Every write is atomic and keeps a `.bak` copy of the previous version, used automatically if the main file is damaged.

Export copies only the selected winners to the destination folder, filtered by minimum rating. Files are copied byte for byte (no re-encoding, original metadata untouched). Optionally they are renamed `<prefix>_0001.ext`, `<prefix>_0002.ext`… in capture-time order (EXIF DateTimeOriginal).

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
make installer   # build and copy the setup to ./installer (gitignored)
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

## Credits

Camera protocol details were learned from these references:

- [fujifilm-ptp-reference](https://github.com/zgrimz/fujifilm-ptp-reference) (CC BY 4.0 / MIT)
- [libfuji](https://github.com/petabyt/libfuji) (MIT)
