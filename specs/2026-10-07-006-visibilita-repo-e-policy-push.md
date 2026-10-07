# 006 — Visibilità del repository e policy di push

- **Data:** 2026-10-07
- **Stato:** Implementata
- **Ambito:** Processo
- **Sostituisce / correlata a:** chiude il punto aperto "visibilità" di 005

## Richiesta originale
> il repo resta pubblico ... ma non pushare tutto in modo automatico

## Decisioni
- Il repository GitHub resta **pubblico**.
- **Niente push automatici:** si fa `git push` solo quando l'utente lo chiede esplicitamente, per ogni push.
- I commit locali sono permessi: ogni specifica registrata e il lavoro implementato vengono salvati in commit locali, che restano sulla macchina finché l'utente non chiede il push.
- La regola è scritta in `CLAUDE.md`, così vale in tutte le sessioni.

## Assunzioni
- "Non pushare in modo automatico" riguarda il push verso GitHub, non i commit locali, che restano utili per la tracciabilità. Se anche i commit vanno fatti solo su richiesta, serve una nuova specifica.

## Punti aperti / rischi
- **Repository pubblico:** tutto ciò che viene salvato è visibile a chiunque, comprese le richieste riportate parola per parola nelle specifiche. Non vanno mai inseriti segreti (chiavi, password, token, `.env`) né dati personali o aziendali riservati. Quando arriverà la configurazione (es. credenziali di un database), andrà gestita con file esclusi da git.
- Workflow sul remoto (push diretto su `main` oppure branch + pull request) ancora da definire.

## Impatto
`CLAUDE.md`, `specs/`.
