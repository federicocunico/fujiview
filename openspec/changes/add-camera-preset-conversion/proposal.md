# Proposal

## Why

FujiView confronta varianti JPEG della stessa foto, ma oggi le varianti vanno generate a mano in FUJIFILM X RAW STUDIO: un RAF alla volta, un preset alla volta, con selezione e salvataggio manuali. Per uno shooting tipico (es. 160 RAF × 7 preset = 1.120 conversioni) è un lavoro di ore. La X-T5 in modalità **USB RAW CONV./BACKUP RESTORE** sa già convertire i RAF col proprio processore tramite PTP; la comunità ha documentato il protocollo (fujifilm-ptp-reference, libfuji, rawji, FilmKit) e X RAW Studio stesso, su Windows, passa dal driver WPD standard (`FTLPTP.dll` usa `IPortableDevice` e comandi MTP-extension). Possiamo quindi automatizzare tutto da FujiView senza driver aggiuntivi.

## What Changes

- Nuovo connettore camera nel backend Rust: trova la Fujifilm collegata via WPD, apre una sessione PTP e invia opcode/proprietà vendor (upload RAF, lettura/patch del profilo `0xD185`, avvio conversione `0xD183`, polling, download, cancellazione).
- Lettura dei banchi custom **C1–C7** della camera (nome + parametri `0xD18C`–`0xD1A4`) e loro traduzione nelle celle del profilo di conversione: ogni banco diventa un "preset".
- Conversione batch: data una cartella (o selezione) di RAF, genera un JPEG per ogni RAF e per ogni preset, salvandolo in `<output>/<nome preset>/<DSCFxxxx>.jpg`, cioè esattamente il layout che FujiView già legge.
- Job ripresi/idempotenti: i file già prodotti vengono saltati, i RAF originali non vengono mai modificati, scritture atomiche, controllo dello spazio su disco prima di partire.
- UI: pannello "Genera varianti dalla camera" con stato connessione, elenco preset rilevati (selezionabili), avanzamento, annullamento, errori per file; a fine job apre direttamente la libreria generata.
- Fase iniziale di **spike di fattibilità** sulla X-T5 reale; se il percorso WPD fallisse, fallback documentato (libusb/WinUSB) da valutare prima di proseguire.

Nessuna modifica incompatibile: il flusso attuale (cartelle esportate a mano) resta invariato.

## Capabilities

### New Capabilities
- `camera-connection`: individuazione della camera Fujifilm in modalità USB RAW CONV su Windows, sessione PTP, invio di operazioni vendor, gestione errori/recupero.
- `camera-presets`: lettura dei banchi custom della camera e loro mappatura in profili di conversione.
- `variant-generation`: conversione batch RAF → JPEG per preset nella struttura di cartelle per preset, con ripresa, annullamento e avanzamento.

### Modified Capabilities
<!-- Nessuna: non esistono ancora spec in openspec/specs/. Lo scan della libreria resta invariato. -->

## Impact

- `src-tauri/Cargo.toml`: nuova dipendenza `windows` (feature COM / `Win32_Devices_PortableDevices`), solo target Windows.
- `src-tauri/src/lib.rs`: estratto in moduli; nuovi moduli `camera/` (trasporto WPD, PTP, profilo, banchi) e `convert.rs` (job), nuovi comandi Tauri ed eventi di avanzamento.
- `src/App.tsx`, `src/types.ts`, `src/styles.css`: nuova schermata/pannello di generazione.
- Esempi/strumenti di sviluppo in `src-tauri/examples/` per gli spike sulla camera reale.
- Requisiti utente: camera impostata su USB RAW CONV./BACKUP RESTORE, X RAW Studio chiuso durante la conversione (sessione esclusiva), RAF scattati dallo stesso corpo macchina collegato.
- Riferimenti (studiati, non copiati: rawji è GPL-3.0): fujifilm-ptp-reference (CC BY 4.0 / MIT), libfuji (MIT).
