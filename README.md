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

Ogni sottocartella immediata è un preset. A ogni scansione, la sottocartella con più JPEG/TIFF unici diventa la **guida**: il suo elenco definisce gli scatti disponibili, ordinati in modo alfanumerico naturale (`DSCF2` prima di `DSCF10`). I file degli altri preset vengono abbinati per nome base ignorando maiuscole ed estensione; RAF, FP2, FP3 e gli altri file non supportati non vengono caricati.

## Uso con Make

Su Windows, dalla root del repository:

```powershell
make setup       # verifica/installa Node, Rust, C++ Build Tools, WebView2 e pacchetti npm
make dev         # avvia l'app in modalità sviluppo
make build       # crea il setup NSIS Windows
make test        # esegue test frontend e Rust
make check       # esegue tutte le verifiche usate dalla CI
make help        # mostra tutti i target disponibili
```

`make setup` usa `winget` soltanto per i prerequisiti mancanti ed esegue `npm ci` nuovamente solo quando cambia `package-lock.json`. Se GNU Make non è disponibile, installarlo prima oppure eseguire direttamente `powershell -File scripts/bootstrap.ps1 -InstallMissing`.

La sidebar sinistra mostra l'elenco della cartella guida con miniature caricate progressivamente; può essere nascosta dalla toolbar per massimizzare l'area di confronto. Un clic sulla foto sceglie quel pannello come vincitore senza cambiare scatto. `Invio` procede allo scatto successivo; `←/→` e i pulsanti freccia navigano avanti e indietro. Le altre scorciatoie sono `1–4` per scegliere il pannello vincente, `Alt+0–5` per assegnare il voto e doppio clic per alternare fit/zoom. L'export copia solo i vincitori e incorpora `xmp:Rating` nella copia senza ricodificare i pixel.

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
make release VERSION=0.2.0
```

Lo script sincronizza le versioni di npm, Cargo e Tauri, esegue i test, costruisce il setup locale, crea commit e tag annotato, quindi invia branch e tag a `origin`. Usare `-SkipLocalBuild` soltanto quando si vuole delegare la build interamente a GitHub Actions.

Il setup non è firmato con un certificato Authenticode: è installabile, ma Windows SmartScreen può mostrare un avviso finché non viene configurato un certificato di code signing.
