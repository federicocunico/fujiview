# Tasks

> **Modello**: `[opus]` = protocollo non documentato, COM/unsafe, concorrenza, diagnosi sulla camera; `[sonnet]` = codice ben specificato, UI, helper puri, docs. In caso di dubbio partire con Sonnet e passare a Opus solo se un gate fallisce.
> **Regole di sicurezza per tutti gli spike**: lavorare solo su copie in `D:\FujiViewTest\`; mai scrivere, spostare o cancellare in `D:\Foto`; controllare lo spazio libero prima di ogni batch (soglia: 20 GB su D:); se la RAM libera scende sotto 4 GB chiudere i processi non essenziali **tranne RustDesk**; chiudere X RAW Studio prima di aprire la sessione.
> **Gate**: se uno spike marcato GATE fallisce, fermarsi e riportare all'utente prima di proseguire.

> **Stato al 14/11/2026 (X-T5 fw 4.31 collegata)**: gate S1–S6 superati. Upload 87 MB ≈ 2,3 s; prima conversione+download ≈ 2,3 s, conversioni successive dello stesso RAF ≈ 1,4 s; batch reale 5 RAF × 8 preset = 40/40 in media 2,43 s/variante. Bug trovato e corretto: ObjectInfo di `0x900C` con un campo u32 di troppo (la camera si bloccava durante `0x900D` con HRESULT 0x8007001F). 5.3/5.4 validati senza X RAW Studio: 54 casi costruiti in memoria dai banchi reali (i banchi della camera non vengono mai scritti), convertiti dalla camera e verificati sui MakerNotes dei JPEG (`tests/fixtures/xt5_makernote_matrix.json`). Correzioni emerse: `0xD197` = Color Chrome FX Blu (cella 25), `0xD198` = pelle levigata (cella 24), `0xD191` = priorità DR (cella 7), WB/NR/temperatura sono codici u16 senza segno. Restano aperti: 2.4 (X RAW Studio aperto), 4.3 (RAF estraneo sulla camera reale), 7.2 (cavo staccato), 8.4 (rilancio che salta tutto), 11.x e 13.x (prove dall'app). Nomi effettivi dei moduli: `camera/ptp.rs` (al posto di `transport.rs`), `variants/{plan,runner,estimate,service,types,diagnostics}.rs` (al posto di `convert.rs`).

## 1. Preparazione ambiente di test `[sonnet]`

- [x] 1.1 Creare `D:\FujiViewTest\raf\` e copiare (non spostare) 5 RAF da `D:\Foto\<cartella di uno shooting recente>` (`DSCF9131.RAF` + 4 successivi); verificare con hash BLAKE3/SHA-256 che copie e originali coincidano e che lo spazio libero su D: resti > 20 GB
- [x] 1.2 Cercare in `D:\Foto` un RAF di un corpo diverso dalla X-T5 (lettura EXIF `Model`) e copiarlo in `D:\FujiViewTest\foreign\`; se non esiste, annotarlo e usare un file RAF troncato come caso negativo; verificare che la cartella contenga il file
- [x] 1.3 Scrivere `D:\FujiViewTest\README.txt` con l'origine dei file e la checklist camera (USB RAW CONV./BACKUP RESTORE, auto power-off disattivato, X RAW Studio chiuso); verificare che il file esista

## 2. Spike S1 – Trasporto WPD e DeviceInfo (GATE) `[opus]`

- [x] 2.1 Aggiungere a `src-tauri/Cargo.toml` la dipendenza `windows` (feature `Win32_Devices_PortableDevices`, `Win32_System_Com`, `Win32_Foundation`, `Win32_UI_Shell_PropertiesSystem`) sotto `[target.'cfg(windows)'.dependencies]`; verificare con `cargo check`
- [x] 2.2 Creare `src-tauri/examples/fuji_probe.rs` che enumera i device WPD (`IPortableDeviceManager`), trova la Fujifilm (`VID_04CB` / manufacturer FUJIFILM) e stampa nome e device ID; verificare con `cargo run --example fuji_probe -- list` con la X-T5 collegata
- [x] 2.3 Implementare nello spike l'invio di `GetDeviceInfo 0x1001` via `WPD_COMMAND_MTP_EXT_EXECUTE_COMMAND_WITH_DATA_TO_READ` + `READ_DATA` + `END_DATA_TRANSFER`; stampare modello, firmware, operazioni e proprietà supportate; **GATE**: l'elenco contiene `0x900C`, `0x900D`, `0xD183`, `0xD185`, `0xD18C`–`0xD1A4`. Salvare l'output in `src-tauri/tests/fixtures/xt5_device_info.bin`
- [ ] 2.4 Verificare il comportamento con X RAW Studio aperto (atteso: device occupato o accesso concorrente) e annotare l'HRESULT ottenuto in `design.md` (sezione Context)
- [ ] 2.5 Se 2.3 fallisce: documentare l'errore, valutare il fallback libusb/WinUSB (D1-A) e **chiedere conferma all'utente** prima di installare qualunque driver

## 3. Spike S2 – Lettura banchi custom (GATE) `[opus]`

- [x] 3.1 Aggiungere a `fuji_probe` il comando `banks`: legge `0xD18C` corrente, per k=1..7 imposta `0xD18C`, legge `0xD18D` e `0xD18E`–`0xD1A4`, ripristina il valore originale in ogni caso (anche su errore); verificare che i nomi stampati coincidano con quelli nel menu della camera
- [x] 3.2 Verificare che dopo il comando il banco selezionato sulla camera sia invariato (rilettura `0xD18C` + controllo visivo) e salvare i dump in `src-tauri/tests/fixtures/xt5_banks.json`

## 4. Spike S3/S4 – Upload, profilo e conversione "come scattato" (GATE) `[opus]`

- [x] 4.1 Aggiungere a `fuji_probe` il comando `convert <raf> <out.jpg>`: pulizia oggetti residui, `SendObjectInfo 0x900C` + `SendObject2 0x900D` (a chunk via `WRITE_DATA`), lettura `0xD185`, dump binario del profilo in `src-tauri/tests/fixtures/xt5_profile_asshot.bin`; **GATE**: profilo letto senza `0x2002`, lunghezza e IOPCode annotati
- [x] 4.2 Scrivere il profilo invariato su `0xD185`, impostare `0xD183 = 1`, attendere il nuovo handle (polling 50 ms, timeout 60 s), `GetObject`, `DeleteObject`, salvare in `D:\FujiViewTest\out\asshot\`; **GATE**: JPEG valido 7728×5152 (o orientato), apribile; misurare e annotare i tempi di upload/conversione/download
- [ ] 4.3 Lanciare `convert` sul RAF di `foreign\` e annotare dove e con che codice fallisce (atteso `0x2002` alla lettura del profilo); verificare che la camera resti utilizzabile subito dopo convertendo di nuovo un RAF valido

## 5. Spike S5/S6 – Più conversioni per upload e fedeltà dei preset (GATE) `[opus]`

- [x] 5.1 Aggiungere a `fuji_probe` il comando `multi <raf>`: un upload, poi due conversioni con cella 8 (film sim) diversa partendo sempre dal profilo base; verificare che escano due JPEG con simulazioni diverse (MakerNotes `FilmMode`). Annotare l'esito: decide il ciclo di D4
- [x] 5.2 Aggiungere il comando `bank <raf> <k>` che applica al profilo base le celle mappate dal banco k (tabella in `design.md`, Context punto 6) e converte; salvare in `D:\FujiViewTest\out\C<k>\`
- [x] 5.3 Produrre il riferimento: convertire lo stesso RAF con il preset C1 in X RAW Studio (manualmente dall'utente o via computer-use) salvandolo in `D:\FujiViewTest\ref\C1\`; confrontare i MakerNotes delle due versioni con `exiftool -FujiFilm:all` (o `jpeg_recipe.py` di fujifilm-ptp-reference); **GATE**: stessi valori per film sim, grana, color chrome, WB e shift, toni, colore, nitidezza, NR, chiarezza, DR. Documentare le proprietà del banco senza corrispondenza
- [x] 5.4 Ripetere 5.3 per un banco monocromatico (Acros/Monochrome con WC/MG) se presente sulla camera, per validare le celle 23/26

## 6. Modulo protocollo e profilo (puro, testabile) `[opus]`

- [x] 6.1 Creare `src-tauri/src/camera/mod.rs`, `transport.rs` con trait `PtpTransport` (comando, parametri, data phase in/out, codice risposta) ed errori tipizzati (`PtpError` con mappatura `0x2002`, `0x2019`, `0x201C`, `0x201E`, disconnessione); verificare con `cargo test camera::transport`
- [x] 6.2 Creare `camera/profile.rs`: parse/serialize del profilo `0xD185` (header, IOPCode UCS-2, celle int32), patch di celle, round-trip byte-identico sulla fixture `xt5_profile_asshot.bin`; verificare con unit test (round-trip, patch singola cella, lunghezze errate rifiutate)
- [x] 6.3 Creare `camera/banks.rs`: struct `Bank`, tabella proprietà→cella validata in 5.3, `apply_bank(base, bank) -> (profilo, proprietà_non_applicate)`; verificare con unit test basati su `xt5_banks.json`
- [x] 6.4 Creare `camera/fuji.rs` con `FujiCamera<T: PtpTransport>` (device_info, read_banks con ripristino `0xD18C`, clear_objects, upload_raf, read/write_profile, convert, wait_output, fetch, delete) e `MockTransport` che riproduce sequenze registrate; verificare con test che coprono flusso felice, `0x2002`, timeout, ripristino banco su errore

## 7. Trasporto WPD di produzione `[opus]`

- [x] 7.1 Portare il codice dello spike in `camera/wpd.rs` come `WpdTransport` (COM MTA su thread dedicato, chunking upload/download, conversione HRESULT → `PtpError`, rilascio sessione in `Drop`); verificare che `fuji_probe` usi il modulo di libreria e che `cargo run --example fuji_probe -- convert` funzioni ancora sulla X-T5
- [ ] 7.2 Gestire disconnessione e sessione rimasta aperta (riapertura automatica dopo `0x201E` o device rimosso); verificare staccando il cavo durante `convert` e ricollegando: l'esempio riporta "camera disconnessa" e un nuovo `convert` riesce senza riavviare il processo

## 8. Motore di generazione varianti `[opus]`

- [x] 8.1 Creare `src-tauri/src/convert.rs`: piano del job (RAF × preset, skip delle varianti complete, ricerca `.part` residui, nome cartella preset + `.fujiview-preset.json`, suffisso ` (n)` su parametri cambiati); verificare con unit test su cartelle temporanee (`tempfile`)
- [x] 8.2 Esecuzione: worker con un upload per RAF e N conversioni (o un upload per conversione secondo l'esito di 5.1), prefetch del RAF successivo, scrittura `.part` + rename, errori per file non bloccanti, retry con backoff su `0x2019`; verificare con test sul `MockTransport` (ripresa dopo interruzione, RAF rifiutato, annullamento)
- [x] 8.3 Guardia sui percorsi: nessuna scrittura/cancellazione fuori dalla cartella di output (canonicalizzazione), RAF aperti in sola lettura; verificare con unit test che tentano path traversal e symlink
- [ ] 8.4 Prova reale da CLI (`fuji_probe batch D:\FujiViewTest\raf D:\FujiViewTest\out`) con i 5 RAF e tutti i banchi; verificare 5 × (banchi + "Come scattato") JPEG, hash dei RAF invariati, rilancio che salta tutto

## 9. Spazio su disco e stima tempi `[sonnet]`

- [x] 9.1 Implementare in `convert.rs` la stima spazio (25 MB/variante iniziale, media reale dopo le prime conversioni, margine 5 GB) con `GetDiskFreeSpaceExW`, controllo all'avvio e ogni 20 conversioni con pausa; verificare con unit test su funzione di stima pura e con un test che simula spazio insufficiente
- [x] 9.2 Calcolo ETA dalla media mobile dei tempi per conversione; verificare con unit test

## 10. Comandi ed eventi Tauri `[sonnet]`

- [x] 10.1 Aggiungere i comandi `camera_status`, `camera_presets`, `plan_variants`, `start_variants`, `cancel_variants`, `resume_variants` ed eventi `camera-status` / `variant-progress` / `variant-finished` in `lib.rs`, registrandoli in `invoke_handler`; verificare con `cargo test` e `make check`
- [x] 10.2 Aggiungere i tipi corrispondenti in `src/types.ts`; verificare con `npm run build` (tsc)

## 11. Interfaccia "Genera varianti" `[sonnet]`

- [ ] 11.1 Pannello modale in `src/App.tsx` (eventualmente nuovo componente `src/VariantGenerator.tsx`): stato camera con "riprova", elenco preset con checkbox (tutti selezionati + "Come scattato"), scelta sorgente/destinazione (default `<sorgente>/FujiView varianti`), riepilogo RAF/conversioni/spazio/ETA, opzione "sovrascrivi"; testi in italiano; verificare in `make dev` con la camera collegata
- [ ] 11.2 Vista avanzamento (RAF e preset correnti, completate/totali, ETA, errori), "Annulla", riepilogo finale con elenco file saltati e motivo, pulsante "Apri per il confronto" che chiama `scan_library` sull'output; verificare in `make dev` su `D:\FujiViewTest\raf`
- [x] 11.3 Stili in `src/styles.css` coerenti con l'app; aggiungere un test vitest per la logica pura del pannello (es. calcolo riepilogo/nome cartella mostrato); verificare con `npm test`

## 12. Documentazione `[sonnet]`

- [x] 12.1 Aggiornare `README.md` con la sezione "Generare le varianti dalla camera" (impostazioni camera, X RAW Studio chiuso, auto power-off, stesso corpo macchina, layout di output, tempi tipici misurati in 4.2) e i crediti (fujifilm-ptp-reference, libfuji); verificare rileggendo che i passaggi corrispondano alla UI

## 13. Verifica d'integrazione sulla camera reale `[sonnet]`

- [ ] 13.1 Job completo dall'app sui 5 RAF di test: verificare tutte le cartelle preset, apertura in FujiView, confronto e scelta del vincitore funzionanti
- [ ] 13.2 Interruzioni: annullamento a metà, chiusura dell'app a metà, cavo staccato a metà; verificare assenza di `.part`, RAF invariati (hash) e ripresa che completa solo i mancanti
- [ ] 13.3 Job su una copia di 30 RAF (controllare prima lo spazio: ~30 × 8 × 25 MB ≈ 6 GB) per misurare tempo medio per conversione e stabilità su durata lunga; annotare i risultati nel README
- [ ] 13.4 Eseguire `make check` e `make test`; verificare che passino

## Workflow follow-up

- Revisionare con `/code-review` prima del merge.
- Al termine, pulire `D:\FujiViewTest\` previa conferma dell'utente (contiene solo copie).
- Archiviare la change con `/opsx:archive` dopo il merge.
