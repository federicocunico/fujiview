# FujiView

Viewer desktop Windows per confrontare rapidamente export JPEG/TIFF dello stesso scatto.

## Struttura delle foto

```text
foto/
  Classic Chrome/
    DSCF0001.jpg
  Velvia/
    DSCF0001.jpg
  Acros/
    DSCF0001.tif
```

Ogni sottocartella immediata è un preset. I file vengono abbinati per nome base ignorando maiuscole ed estensione.

## Uso

1. Installa Node.js, Rust MSVC e i prerequisiti Windows di [Tauri 2](https://v2.tauri.app/start/prerequisites/).
2. Esegui `npm install`.
3. Avvia con `npm run tauri dev` oppure crea l'installer con `npm run tauri build`.

Scorciatoie: `←/→` cambia scatto, `1–4` sceglie il pannello vincente, `Alt+0–5` assegna il voto, doppio clic alterna fit/zoom. L'export copia solo i vincitori e incorpora `xmp:Rating` nella copia senza ricodificare i pixel.

I progetti `.fujiview` sono JSON versionati e gli originali non vengono mai modificati. Le preview sono conservate nella cache locale dell'app con limite LRU di 20 GB.

## Verifica

```powershell
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

## Release Windows

Il setup utilizza NSIS tramite Tauri. Ogni push di un tag `vX.Y.Z` avvia la workflow `Release Windows`, compila su `windows-latest`, carica l'installer come artefatto e crea una GitHub Release pubblica contenente `FujiView_X.Y.Z_x64-setup.exe`.

Per una nuova versione, partire da un working tree pulito ed eseguire:

```powershell
npm run release -- -Version 0.2.0
```

Lo script sincronizza le versioni di npm, Cargo e Tauri, esegue i test, costruisce il setup locale, crea commit e tag annotato, quindi invia branch e tag a `origin`. Usare `-SkipLocalBuild` soltanto quando si vuole delegare la build interamente a GitHub Actions.

Il setup non è firmato con un certificato Authenticode: è installabile, ma Windows SmartScreen può mostrare un avviso finché non viene configurato un certificato di code signing.
