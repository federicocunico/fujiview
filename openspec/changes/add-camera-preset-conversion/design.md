# Design

## Context

- FujiView è un'app Tauri 2 (React + Rust, solo Windows). Il backend (`src-tauri/src/lib.rs`, ~820 righe) espone `scan_library`, `get_preview`, `save_project`, `load_project`, `export_winners`. `scan_library` tratta ogni sottocartella immediata come un preset: l'output di questa change deve rispettare quel layout, così non serve toccare lo scan.
- Ambiente verificato (14/11/2026): X RAW Studio installato in `C:\Program Files\FUJIFILM X RAW STUDIO`. `FTLPTP.dll` contiene `IPortableDeviceEventCallback`, `AVCWPDSendCommand`, `AVCWPDPtpOperate`, `AVCMTPOpVendor`, `FTL_PTP_*`: **X RAW Studio parla con la camera tramite WPD (driver Windows standard) con i comandi MTP-extension**, non con libusb. Al momento dell'analisi la X-T5 non risultava enumerata (nessun device `VID_04CB`), probabilmente in standby: va riverificato nello spike S1.
- Dataset: `D:\Foto\<cartella di uno shooting recente>` contiene 160 RAF (~83 MB l'uno) + 160 JPG SOOC, nessuna cartella preset esistente. Spazio libero: D: ~117 GB, C: ~53 GB. RAM libera ~9.5 GB su 32.
- Protocollo (da fujifilm-ptp-reference, libfuji, rawji, FilmKit; nessuna specifica ufficiale):
  1. `SendObjectInfo 0x900C` (formato `0xF802`, nome `FUP_FILE.dat`) + `SendObject2 0x900D` con i byte del RAF.
  2. `GetDevicePropValue 0xD185` → profilo di conversione (~625 B su X-T5, IOPCode `FF179502`): header + celle `int32` LE da offset `0x1FD`. `0x2002` qui = RAF non convertibile (es. altro corpo).
  3. Patch delle celle → `SetDevicePropValue 0xD185`.
  4. `SetDevicePropValue 0xD183 = 1` (risoluzione piena) avvia la conversione.
  5. Polling `GetObjectHandles 0x1007` (o eventi `0xD212`/`0xD20E`) → `GetObject 0x1009` → `DeleteObject 0x100B`.
  6. Banchi custom: `0xD18C = k` seleziona il banco, `0xD18D` nome, `0xD18E…0xD1A4` parametri; mappatura nota banco→cella per ~19 proprietà (es. `0xD192`→film sim cella 8, `0xD195`→grana 9, `0xD199`→WB 12…). Ripristinare `0xD18C` al termine.

## Goals / Non-Goals

**Goals:**
- Trasporto PTP su WPD in Rust, nessun driver extra, coesistenza con X RAW Studio (non contemporanea).
- Codice di protocollo puro e testabile senza camera (fixture binarie dei profili reali della X-T5).
- Pipeline batch robusta: ripresa, annullamento, scritture atomiche, controllo disco.
- Ogni passo rischioso validato prima da uno spike eseguibile sulla camera reale.

**Non-Goals:**
- Scrivere o modificare i banchi custom della camera (solo lettura).
- Editor di ricette/parametri, TIFF/HEIF, conversioni in risoluzione ridotta.
- Supporto macOS/Linux o camere diverse dalla X-T5 (il design non le esclude, ma non sono testate).
- Wi-Fi/Bluetooth.

## Decisions

### D1. Trasporto: WPD + comandi MTP-extension (crate `windows`)
Uso di `IPortableDevice::SendCommand` con `WPD_COMMAND_MTP_EXT_EXECUTE_COMMAND_WITHOUT_DATA_PHASE`, `…_WITH_DATA_TO_READ`, `…_WITH_DATA_TO_WRITE`, `…_READ_DATA`, `…_WRITE_DATA`, `…_END_DATA_TRANSFER`. È lo stesso percorso di X RAW Studio, quindi è provato su Windows; il driver WPD gestisce `OpenSession`.
- *Alternativa A – libusb/`rusb` + WinUSB (Zadig)*: protocollo più diretto e codice simile a rawji, ma richiede di sostituire il driver, rompe X RAW Studio e complica l'installazione. **Fallback solo se lo spike S1/S3 fallisce**, previa conferma dell'utente.
- *Alternativa B – caricare `FTLPTP.dll`*: export non documentati, licenza proprietaria, non ridistribuibile. Scartata (utile al massimo per confronto).
- *Alternativa C – sidecar Python (rawji/pyusb)*: runtime extra + stesso problema driver di A + GPL. Scartata.

### D2. Architettura a strati
```
camera::transport  trait PtpTransport { command(op, params, DataPhase) -> Response }
  ├─ WpdTransport      (Windows, COM, thread dedicato)
  └─ MockTransport     (test: risposte registrate / fixture)
camera::fuji       FujiCamera<T>: device_info, read_banks, upload_raf, read_profile,
                   write_profile, convert, wait_output, fetch, delete
camera::profile    parse/serialize profilo 0xD185, patch celle (puro, unit test)
camera::banks      Bank {index, name, props}, mappa prop→cella, nome cartella
convert            Job runner: piano, stato, ripresa, eventi, disco
```
Il protocollo non dipende da WPD: un eventuale fallback libusb cambia solo `transport`.

### D3. Thread e concorrenza
Un solo worker thread possiede COM (`CoInitializeEx` MTA) e l'`IPortableDevice`; riceve comandi via canale e invia eventi Tauri (`variant-progress`, `camera-status`). La camera serializza comunque le conversioni: nessun parallelismo verso la camera. In parallelo, su un altro thread, si leggono dal disco il RAF successivo (prefetch in RAM, max 1 RAF = ~83 MB) e si scrivono i JPEG.

### D4. Un upload per RAF, N conversioni
Per ogni RAF: upload una volta, leggi il profilo base una volta, poi per ogni preset: copia del base → patch → write → convert → fetch → delete. Evita 7 upload da 83 MB. Lo spike **S5** verifica che la camera accetti più conversioni dopo un solo upload; se no, si torna a un upload per conversione (stesso codice, ciclo diverso). Il profilo base va sempre usato come partenza, mai un profilo già patchato.

### D5. Preset = banchi custom + "Come scattato"
I banchi vengono letti una volta all'inizio del job (snapshot). "Come scattato" usa il profilo base senza patch. Ogni cartella preset contiene `.fujiview-preset.json` (indice, nome, proprietà grezze, hash, modello/firmware). Se un banco con lo stesso nome ha parametri diversi dallo snapshot salvato, la nuova cartella riceve il suffisso ` (2)`, ` (3)`… così le varianti non si mescolano. Le proprietà del banco senza cella corrispondente sono riportate all'utente come "non applicate".

### D6. Ripresa, atomicità, sicurezza dei file
- Una variante è "completa" se esiste `<preset>/<stem>.jpg`, il JPEG ha marker EOI valido e dimensione > 0. Scrittura in `<stem>.jpg.part` + `rename`.
- All'avvio i `.part` rimasti vengono cancellati (solo dentro la cartella di output).
- I RAF sono aperti in sola lettura. Nessuna operazione di cancellazione fuori dalla cartella di output, verificata da un helper che canonicalizza i percorsi.
- Lo spike lavora su **copie** in `D:\FujiViewTest\` (mai sugli originali di `D:\Foto`).

### D7. Stima disco
Stima iniziale = conversioni mancanti × 25 MB (JPEG Fine 40 MP), poi aggiornata con la media reale dopo le prime conversioni. Margine di sicurezza 5 GB. Controllo prima dell'avvio e ogni 20 conversioni (pausa se sotto margine).

### D8. Errori
Mappatura codici PTP → errori tipizzati: `0x2002` dopo upload → "RAF non di questa camera / non convertibile" (skip file); `0x2019` busy → retry con backoff fino a 3 volte, poi stop "spegni e riaccendi"; device rimosso (`HRESULT` WPD) → pausa "camera disconnessa"; timeout conversione 60 s (come `CONVERT_TIMEOUT` di X RAW Studio). Prima di ogni nuova conversione si svuotano eventuali oggetti residui (`GetObjectHandles` + `DeleteObject`) per non prendere l'output di un job precedente.

### D9. Validazione della fedeltà
Confronto dei MakerNotes Fujifilm (film sim, grana, WB, toni, DR, …) tra la variante di FujiView e un riferimento prodotto da X RAW Studio sullo stesso RAF e banco. Strumento solo di sviluppo (`exiftool` oppure `tools/jpeg_recipe.py` di fujifilm-ptp-reference, MIT), non incluso nell'app.

### D10. Licenze
Si implementa da zero seguendo la documentazione (fujifilm-ptp-reference CC BY 4.0, libfuji MIT come riferimento). Non si copia codice da rawji (GPL-3.0). Crediti nel README.

### D11. UI
Nuovo pannello modale "Genera varianti" nella schermata iniziale e nella toolbar: stato camera, lista preset con checkbox, sorgente/destinazione, stima conversioni/spazio/tempo, barra di avanzamento, annulla, riepilogo errori, pulsante "Apri per il confronto" che chiama lo scan esistente sulla cartella di output. Testi in italiano come il resto dell'app.

### Allocazione modello per gli step
Regola: **Opus** dove c'è protocollo non documentato, COM/unsafe, concorrenza o diagnosi sulla camera reale; **Sonnet** per codice ben specificato (UI, helper puri, documentazione, test da specifica). Indicato su ogni gruppo in `tasks.md`.

## Risks / Trade-offs

- [WPD non inoltra opcode vendor con data phase (`0x900C`/`0x900D`) o tronca payload grandi] → Spike S1/S3 come gate; trasferimento a chunk con `WRITE_DATA`; fallback D1-A (libusb) previa conferma.
- [Profilo X-T5 diverso dai modelli documentati (lunghezza/celle)] → Dump reale del profilo in S3, parser che accetta lunghezze variabili e verifica IOPCode; fixture nei test.
- [Mappatura banco→cella incompleta o errata per X-T5] → S6 confronta con X RAW Studio; proprietà sconosciute segnalate come "non applicate" invece di indovinare.
- [La camera si blocca (`0x2019`) e serve un power-cycle] → Job ripristinabile; messaggio chiaro; nessun file parziale.
- [Più conversioni per upload non supportate] → S5; costo: ~7× tempo di upload (con USB 3 accettabile).
- [Durata job lunga (es. 1.120 conversioni × ~3–5 s ≈ 1–1,5 h)] → Ripresa, ETA, possibilità di selezionare meno preset.
- [Disco pieno] → D7; output su disco scelto dall'utente.
- [Spegnimento automatico della camera durante job lunghi] → Documentare di disattivare l'auto power-off; gestito comunque come disconnessione.
- [Lettura banchi lascia selezionato un altro banco] → Ripristino di `0xD18C` in `finally`, verificato in S2.

## Migration Plan

Funzione additiva: nessuna migrazione dati. Il formato `.fujiview` non cambia. Rollback = rimuovere il pannello; le cartelle generate restano normali librerie FujiView.

## Open Questions

- Se e come esporre la scelta della qualità/dimensione JPEG (per ora: come scattato).
- Se aggiungere un preset "SOOC" che copia/collega i JPG già scattati dalla camera invece di riconvertirli.
