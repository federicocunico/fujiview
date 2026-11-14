# Spec Delta

## Purpose

Converte in modo automatico un insieme di RAF in tutte le varianti JPEG richieste, una sottocartella per preset, nel layout che FujiView usa già per i confronti.

## ADDED Requirements

### Requirement: Selezione dei RAF in ingresso
L'utente SHALL poter scegliere una cartella sorgente (sono considerati solo i file `.RAF` al primo livello) oppure singoli file RAF. Il sistema SHALL mostrare quanti RAF verranno convertiti e quante conversioni totali risultano (RAF × preset).

#### Scenario: Cartella di uno shooting
- **WHEN** l'utente seleziona una cartella con 160 RAF e 160 JPG e 7 preset selezionati
- **THEN** il sistema indica 160 RAF e 1.120 conversioni; i JPG esistenti vengono ignorati

### Requirement: Layout di output
Il sistema SHALL scrivere ogni variante in `<cartella di output>/<cartella del preset>/<nome RAF>.jpg`. La cartella di output predefinita è `<cartella sorgente>/FujiView varianti` e l'utente SHALL poterla cambiare. L'output MUST essere apribile in FujiView senza ulteriori passaggi.

#### Scenario: Output pronto per il confronto
- **WHEN** il job termina con successo
- **THEN** ogni cartella preset contiene un JPEG per ogni RAF convertito con lo stesso nome base (es. `DSCF9131.jpg`) e FujiView apre la cartella di output come libreria

### Requirement: Protezione dei file originali
Il sistema MUST NOT modificare, spostare o cancellare i RAF né alcun file fuori dalle cartelle di output dei preset. Ogni JPEG SHALL essere scritto in un file temporaneo e rinominato solo a scrittura completata.

#### Scenario: Job interrotto
- **WHEN** l'app viene chiusa a metà di una conversione
- **THEN** nelle cartelle preset non restano JPEG parziali e i RAF sorgente sono identici a prima (stessa dimensione e hash)

### Requirement: Ripresa e idempotenza
Una variante già presente e completa SHALL essere saltata. Rilanciare lo stesso job MUST produrre solo le varianti mancanti.

#### Scenario: Ripresa dopo un'interruzione
- **WHEN** un job da 1.120 conversioni si interrompe dopo 400 e viene rilanciato
- **THEN** il sistema esegue solo le 720 mancanti e il riepilogo le indica come "saltate: 400"

#### Scenario: Riconvertire forzatamente
- **WHEN** l'utente attiva "sovrascrivi varianti esistenti"
- **THEN** tutte le varianti selezionate vengono rigenerate

### Requirement: Avanzamento e annullamento
Durante il job il sistema SHALL mostrare RAF e preset correnti, conteggio completate/totali, tempo stimato residuo ed errori; l'utente SHALL poter annullare, e l'annullamento MUST terminare entro la conversione in corso.

#### Scenario: Annullamento
- **WHEN** l'utente preme "Annulla" durante il job
- **THEN** la conversione in corso termina o viene scartata, nessun file parziale resta e la sessione con la camera viene chiusa

### Requirement: Errori per singolo file
Un RAF che la camera rifiuta SHALL essere segnalato e saltato senza fermare il job; il riepilogo finale MUST elencare file e motivo.

#### Scenario: RAF di un altro corpo macchina
- **WHEN** la cartella contiene un RAF scattato con una camera diversa da quella collegata
- **THEN** quel RAF viene saltato con il motivo "RAF non scattato da questa camera" e gli altri vengono convertiti

### Requirement: Controllo dello spazio su disco
Prima di avviare il job il sistema SHALL stimare lo spazio necessario e MUST rifiutare l'avvio se lo spazio libero sul disco di output non basta lasciando un margine di sicurezza; durante il job MUST mettersi in pausa se lo spazio scende sotto il margine.

#### Scenario: Disco quasi pieno
- **WHEN** lo spazio stimato supera lo spazio libero meno il margine
- **THEN** il job non parte e il sistema mostra spazio richiesto e disponibile
