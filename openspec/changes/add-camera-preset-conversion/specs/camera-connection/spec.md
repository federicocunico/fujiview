# Spec Delta

## Purpose

Permette a FujiView di trovare una camera Fujifilm collegata via USB in modalità USB RAW CONV./BACKUP RESTORE e di dialogarci in modo affidabile, senza driver aggiuntivi e senza X RAW Studio.

## ADDED Requirements

### Requirement: Rilevamento della camera
Il sistema SHALL elencare le camere Fujifilm collegate mostrando modello, firmware e se sono in una modalità che consente la conversione RAW. Il rilevamento MUST funzionare col driver Windows predefinito, senza installare driver o strumenti di terze parti.

#### Scenario: X-T5 in modalità USB RAW CONV
- **WHEN** una X-T5 impostata su USB RAW CONV./BACKUP RESTORE è collegata e accesa
- **THEN** il sistema mostra "X-T5" con la versione firmware e lo stato "pronta per la conversione"

#### Scenario: Camera in modalità non compatibile
- **WHEN** la camera è collegata in una modalità USB diversa (es. lettore di schede, webcam)
- **THEN** il sistema la segnala come non pronta e indica di impostare USB RAW CONV./BACKUP RESTORE nel menu della camera

#### Scenario: Nessuna camera
- **WHEN** nessuna camera Fujifilm è collegata o la camera è spenta/in standby
- **THEN** il sistema mostra "nessuna camera rilevata" e permette di ripetere la ricerca senza riavviare l'app

### Requirement: Sessione esclusiva
Il sistema SHALL aprire una sola sessione alla volta con la camera e chiuderla al termine o all'annullamento. Se la camera è già in uso da un altro programma, il sistema MUST segnalarlo invece di fallire in modo silenzioso.

#### Scenario: X RAW Studio aperto
- **WHEN** X RAW Studio sta già usando la camera e l'utente avvia una conversione
- **THEN** il sistema mostra che la camera è occupata e chiede di chiudere l'altro programma

#### Scenario: Sessione rimasta aperta
- **WHEN** una sessione precedente non è stata chiusa correttamente (es. crash dell'app)
- **THEN** il sistema la chiude e ne riapre una nuova automaticamente

### Requirement: Gestione della disconnessione e degli errori della camera
Il sistema SHALL riconoscere disconnessione, timeout e stato "occupato" della camera, interrompere l'operazione in corso senza lasciare file parziali e riportare un messaggio comprensibile con l'azione suggerita.

#### Scenario: Cavo scollegato durante una conversione
- **WHEN** la camera viene scollegata mentre una conversione è in corso
- **THEN** il job si mette in pausa con stato "camera disconnessa", nessun file parziale viene lasciato nelle cartelle di output e il job può essere ripreso dopo il ricollegamento

#### Scenario: Camera bloccata
- **WHEN** la camera smette di rispondere oltre il timeout configurato
- **THEN** il sistema interrompe il job e suggerisce di spegnere e riaccendere la camera
