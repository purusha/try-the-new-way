# 008 — Documentazione HTML dello schema dati del BE

- **Data:** 2026-10-07
- **Stato:** Approvata
- **Ambito:** BE (documentazione)
- **Sostituisce / correlata a:** documenta lo schema introdotto da 007

## Richiesta originale
> ho bisogno di una doc html dello schema lato be. questa documentazione servirà anche a chi non conosce nulla dell'argomento e si approccia per la prima volta al mondo ERP. genera righe e relazione di esempio per far capire cosa ci sarà come e dati e soprattutto come mai sono organizzati in quel modo. niente allucinazioni ma solo della buona documentazione a corredo.

Risposta alla domanda di chiarimento:
- Destinazione: "File nel repo" (`docs/schema-db.html`).

## Decisioni
- Un solo file `docs/schema-db.html`, autonomo: CSS inline, diagramma in SVG inline, nessuna dipendenza esterna, tema chiaro e scuro. Lingua italiana.
- Contenuti:
  - introduzione al dominio ERP/WMS e glossario;
  - mappa delle tabelle;
  - per ogni tabella: scopo, colonne, righe d'esempio e motivazione delle scelte ("Perché così");
  - lo scenario completo dall'ordine fornitore all'evasione FEFO;
  - immutabilità e storni, la vista di disponibilità, l'idempotenza e i limiti noti.
- Fonti ammesse, e solo queste:
  - `be/migrations/0001_init.sql`;
  - la spec 007;
  - `api/examples/` e gli esempi di `api/openapi.yaml`;
  - il codice in `be/src/`.
- Dati d'esempio:
  - dove possibile, i valori dello scenario end-to-end reale (`api/examples/`);
  - i valori ricostruiti o abbreviati (UUID, indirizzi, righe non presenti negli esempi) sono segnalati nella pagina come illustrativi.

## Assunzioni
- La pagina è scritta a mano e non è generata dallo schema.
- Gli UUID sono abbreviati per leggibilità.

## Punti aperti / rischi
- **Allineamento:** la pagina va aggiornata a mano a ogni nuova migrazione. In futuro si potrà generarla o verificarla in CI, ad esempio con un controllo che ogni tabella e colonna della migrazione compaia nella doc.

## Impatto
- Nuovo `docs/schema-db.html`.
- Link nel `README.md` della radice.
