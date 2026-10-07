# Regole di lavoro del progetto

## Monorepo
- `be/` — backend Rust (API)
- `fe/` — client web React + Vite (TypeScript)
- `specs/` — registro delle specifiche e delle decisioni

## Registro delle specifiche (obbligatorio)
Ogni richiesta dell'utente che introduce o modifica requisiti, scelte tecniche o comportamento va registrata in `specs/` **prima** di implementarla.

- Un file per specifica: `specs/AAAA-MM-GG-NNN-titolo-breve.md`
  - `NNN` è un progressivo globale (001, 002, …) che rende l'ordine univoco anche con più specifiche nello stesso giorno.
- Usa il template `specs/_TEMPLATE.md` e aggiorna l'indice in `specs/README.md`.
- Le specifiche già scritte non si riscrivono: un cambio di decisione diventa una **nuova** specifica che cita quella che sostituisce, e quella vecchia passa allo stato `Superata da NNN`.
- Riporta la richiesta originale dell'utente testualmente, in modo che resti traccia di cosa è stato chiesto e non solo di cosa è stato deciso.

## Partner attivo, non passivo
Prima di registrare e implementare una specifica:
1. **Confrontala con le specifiche esistenti** e segnala contraddizioni, sovrapposizioni o dipendenze.
2. **Cerca i blocchi funzionali**: requisiti mancanti, casi limite non coperti, flussi senza uscita, dati che non arrivano da nessuna parte, vincoli tra BE e FE non allineati (contratti API, autenticazione, errori).
3. **Proponi alternative** quando una scelta ha rischi o costi evidenti, con una raccomandazione motivata.
4. **Fai domande** sui punti ambigui invece di assumere in silenzio; le assunzioni inevitabili vanno scritte nella sezione "Assunzioni" della specifica.
5. Riporta i punti aperti nella sezione "Punti aperti / rischi" e lasciali lì finché l'utente non li chiude.
