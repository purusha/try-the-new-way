# 007 — API di gestione magazzino (prodotti, lotti, giacenze, movimenti, ordini)

- **Data:** 2026-10-07
- **Stato:** Implementata
- **Ambito:** BE+FE
- **Sostituisce / correlata a:** chiude il punto aperto "contratto API BE↔FE" di 001 e 003

## Richiesta originale
> Agisci come un Lead System Architect e sviluppatore Backend esperto. Il tuo compito è progettare e implementare un'API RESTful completa ed enterprise-grade per la gestione dei prodotti, delle giacenze e delle movimentazioni di un magazzino merci.
>
> Non soffermarti su scelte di framework o codice di implementazione specifico, ma concentrati sull'architettura funzionale, sulle regole di business e sui contratti delle risorse.
>
> ---
>
> ### 1. Entità di Dominio Principali
> L'API deve gestire le seguenti risorse fondamentali:
> - **Articolo / Prodotto:** Codice unico (SKU/EAN), nome, descrizione, categoria, unità di misura (es. pezzi, kg, metri), prezzo unitario, soglia di scorta minima e stato (attivo/disattivo).
> - **Lotto e Serial/Matricola:** Identificativo lotto produttore, data di produzione, data di scadenza e numero seriale univoco per singolo pezzo (se tracciato a seriale).
> - **Ubicazione / Scaffale:** Identificativo unico (es. Zona A - Corsia 3 - Ripiano 2), capienza massima e tipo di stoccaggio.
> - **Giacenza / Stock:** Associazione tra Prodotto, Lotto (opzionale) e Ubicazione, con indicazione della quantità totale presente, quantità prenotata/impegnata e quantità effettivamente disponibile.
> - **Movimento di Magazzino:** Tracciamento di ogni variazione con tipologia (Carico, Scarico, Trasferimento interno, Rettifica inventario, Impegno stock, Disimpegno stock), causale, riferimento ordine, data/ora e operatore.
> - **Ordine (Acquisto / Vendita):** Riferimento a documenti esterni o interni (es. Ordine Cliente, Ordine Fornitore) per la gestione dell'impegno e dell'evasione merci.
>
> ---
>
> ### 2. Funzionalità ed Endpoint Richiesti
>
> #### A. Gestione Anagrafica Prodotti e Lotti
> - Creazione, modifica, consultazione e archiviazione prodotti.
> - Tracciamento della vita del lotto (inclusa la gestione della data di scadenza) e ricerca avanzata per lotto o seriale.
> - Ricerca e filtraggio avanzato prodotti (per categoria, sotto-scorta, prodotti in scadenza).
>
> #### B. Gestione Mappa Magazzino ed Ubicazioni
> - Definizione della struttura fisica delle zone/scaffali.
> - Mappatura dinamica dei prodotti e dei relativi lotti nelle specifiche ubicazioni.
>
> #### C. Operazioni di Magazzino e Logica FIFO/FEFO
> - **Ingresso Merci (Carico da Fornitore):** Aumento della giacenza fisica con assegnazione obbligatoria/opzionale di lotto, data scadenza e ubicazione di destinazione.
> - **Suggerimento Prelievo (FEFO/FIFO):** Endpoint di calcolo che raccomanda da quale ubicazione e lotto prelevare la merce (FEFO per merci con scadenza più vicina, FIFO per merci più vecchie).
> - **Uscita Merci (Scarico/Evasione):** Riduzione della giacenza fisica basata sulla selezione manuale o automatica (FEFO/FIFO).
> - **Trasferimento Interno:** Spostamento di specifici lotti/prodotti da un'ubicazione A a un'ubicazione B.
> - **Rettifica Inventario:** Allineamento del valore a sistema con il conteggio fisico (con causale obbligatoria).
>
> #### D. Ciclo di Prenotazione ed Evasione Ordini
> - **Prenotazione Stock (Ordine Cliente):** Blocco temporaneo di quantitativi di merce ("quantità impegnata") in vista di un'evasione, riducendo lo stock disponibile senza alterare la giacenza fisica totale.
> - **Sblocco / Annullamento Prenotazione:** Rilascio della quantità impegnata in caso di cancellazione o modifica dell'ordine.
> - **Evasione Ordine:** Conversione automatica della quantità impegnata in un movimento effettivo di scarico/uscita merci.
>
> #### E. Tracciabilità e Storico
> - Storico della tracciabilità di un lotto (dall'ingresso, ai trasferimenti, fino alla spedizione/cliente).
> - Storico completo delle movimentazioni per articolo, ubicazione o riferimento ordine.
>
> ---
>
> ### 3. Regole di Business e Validazioni
> L'API deve garantire l'integrità dei dati applicando i seguenti vincoli:
> 1. **Disponibilità Calcolata:** La disponibilità reale è `Giacenza Fisica Totale - Quantità Impegnata`. Impossibile prenotare o scaricare una quantità superiore alla disponibilità reale.
> 2. **Priorità Scadenze (FEFO):** L'API deve impedire (o richiedere un override esplicito) lo scarico di un lotto con scadenza posteriore se è presente nello stesso magazzino un lotto dello stesso prodotto con scadenza antecedente.
> 3. **Alert Sotto-Scorta e Scadenze:** Segnalare nel payload o via notifiche/flag se la giacenza disponibile scende sotto la soglia minima o se ci sono lotti in prossimità di scadenza.
> 4. **Immutabilità dei Movimenti:** Un movimento di magazzino confermato non è modificabile né eliminabile. Errori di registrazione richiedono un movimento inverso di rettifica.
> 5. **Capienza Ubicazione:** Blocco automatico dei carichi o trasferimenti verso ubicazioni che hanno raggiunto la loro capienza massima.
>
> ---
>
> ### 4. Output Richiesto
> Fornisci:
> 1. La **definizione OpenAPI/Swagger** (in formato YAML o JSON) completa degli endpoint, request/response body e relativi codici HTTP.
> 2. Esempi di payload JSON significativi per:
>    - Carico merce con lotto e scadenza.
>    - Prenotazione stock per ordine cliente.
>    - Evasione ordine guidata da logica FEFO.
> 3. Un elenco dettagliato delle eccezioni di dominio e relativi codici d'errore HTTP/applicativi (es. `GiacenzaInsufficienteException`, `LottoScadutoException`, `UbicazionePienaException`).

Risposte alle domande di chiarimento:
- Perimetro: "Design + implementazione FE e BE"
- Livello dell'impegno: "Su prodotto (soft)"
- Capienza ubicazione: "Peso e volume"
- Magazzini: "Multi-magazzino"
- Persistenza: "PostgreSQL + sqlx"
- Operatore: "Header X-Operator, no auth"
- Lingua API: "Inglese"
- FE: "Operatività essenziale"

## Decisioni

### Contratto
- Il contratto è in `api/` alla radice, condiviso da BE e FE:
  - `api/openapi.yaml` (OpenAPI 3.1);
  - `api/examples/` (esempi di payload);
  - `api/errors.md` (catalogo delle eccezioni di dominio).
- Prefisso delle API: `/api/v1`.
- Risorse, campi e codici d'errore sono in inglese (`snake_case` per i campi, `UPPER_SNAKE` per enum e codici). Documentazione e messaggi d'errore sono in italiano.
- Errori nel formato RFC 9457 `application/problem+json`, con i campi `type`, `title`, `status`, `detail`, `code` (codice applicativo stabile) e `details` (oggetto con i dati del caso).
- Le liste sono paginate con `page` (da 1) e `page_size` (default 50, massimo 200). La risposta è `{ items, page, page_size, total }`.
- L'header `X-Operator` è obbligatorio su tutte le richieste che scrivono e finisce nel campo `operator` dei movimenti. Se manca: 400 `OPERATOR_MISSING`.
- L'header `Idempotency-Key` è opzionale sulle POST di operazioni e ordini. Se la chiave è già stata usata con lo stesso corpo, si restituisce la risposta già data; con un corpo diverso: 422 `IDEMPOTENCY_KEY_REUSED`.
- Concorrenza ottimistica sul PATCH di prodotti, ubicazioni e magazzini: `ETag` contiene la versione e `If-Match` è opzionale. Se la versione non coincide: 412 `VERSION_CONFLICT`.
- Le risposte delle operazioni che cambiano stock includono `alerts[]`.

### Dominio
- **Magazzino** (multi-magazzino) → **Zona** → **Ubicazione**.
  - Il codice dell'ubicazione è univoco nel magazzino (es. `A-03-02`). L'ubicazione ha corsia, ripiano e livello.
  - `storage_type` vale `AMBIENT | REFRIGERATED | FROZEN | HAZARDOUS | BULK`.
  - L'ubicazione ha `max_weight_kg` e `max_volume_m3`; null significa capienza illimitata.
- **Prodotto:**
  - `sku` univoco, `ean` univoco e opzionale;
  - nome, descrizione, categoria (testo libero);
  - `uom` vale `PCS | KG | M | L`;
  - `unit_price` e `currency`, `min_stock`;
  - `tracking` vale `NONE | LOT | SERIAL`, più `requires_expiry` ed `expiry_warning_days` (default 30);
  - `unit_weight_kg` e `unit_volume_m3` (per unità di misura), `required_storage_type` opzionale;
  - `status` vale `ACTIVE | INACTIVE`;
  - `version`.
- **Lotto:**
  - `lot_code` (lotto del produttore, univoco per prodotto), `production_date`, `expiry_date`, `received_at`;
  - `status` vale `ACTIVE | BLOCKED`.
- **Seriale:** univoco per prodotto, con lotto opzionale, ubicazione corrente e `status` (`IN_STOCK | SHIPPED | SCRAPPED`).
- **Giacenza:** una riga per (prodotto, lotto, ubicazione) con la quantità fisica. Per (prodotto, magazzino) si calcolano:
  - `physical`: somma delle righe di stock;
  - `unusable`: quantità in lotti scaduti o bloccati;
  - `reserved`: somma degli impegni aperti degli ordini di vendita;
  - `available = physical − unusable − reserved` (può diventare negativo solo per effetto di rettifiche o scadenze).
- **Movimento:**
  - tipo `INBOUND | OUTBOUND | TRANSFER | ADJUSTMENT | RESERVATION | RELEASE`;
  - prodotto, lotto, seriali, ubicazione di origine e di destinazione, quantità (sempre positiva; per la rettifica conta `direction`, che vale `IN` o `OUT`);
  - `reason_code` e nota, riferimento all'ordine e alla riga d'ordine, `external_ref`;
  - `operator`, `occurred_at`;
  - `operation_id`, che raggruppa le righe di una stessa richiesta;
  - `reverses_movement_id`, `fefo_override_reason`.
- **Ordini:**
  - di vendita (impegno ed evasione);
  - di acquisto (riferimento per i carichi). Gli ordini di acquisto passano da `OPEN` a `PARTIALLY_RECEIVED` e poi a `RECEIVED`.

### Regole di business
1. **Disponibilità:** non si può prenotare né scaricare oltre `available` del magazzino. Per gli scarichi senza ordine (`INSUFFICIENT_AVAILABILITY`) conta solo lo stock non impegnato. Per l'evasione di un ordine conta l'impegno dell'ordine stesso.
2. **FEFO:**
   - Lo scarico manuale di un lotto genera 409 `FEFO_VIOLATION` se nello stesso magazzino esiste un lotto utilizzabile dello stesso prodotto con scadenza precedente, a meno che la richiesta non contenga `fefo_override: { reason }`.
   - L'override viene registrato sul movimento.
   - La modalità `auto` applica FEFO ai prodotti con scadenza e FIFO (`received_at`) agli altri.
3. **Alert:** `LOW_STOCK` (available < `min_stock`), `EXPIRING` (scadenza entro `expiry_warning_days`), `EXPIRED`, `RESERVATION_AT_RISK` (available < 0). Sono restituiti nelle risposte delle operazioni, come flag sulla disponibilità e da `GET /alerts`.
4. **Immutabilità dei movimenti:**
   - non esistono PUT, PATCH o DELETE (405 `MOVEMENT_IMMUTABLE`), e un trigger sul DB blocca UPDATE e DELETE;
   - la correzione si fa con `POST /movements/{id}/reversal`, che crea il movimento inverso collegato, una sola volta (`MOVEMENT_ALREADY_REVERSED`).
5. **Capienza:** carichi e trasferimenti verso un'ubicazione che supererebbe `max_weight_kg` o `max_volume_m3` vengono rifiutati con 409 `LOCATION_CAPACITY_EXCEEDED`, con il dettaglio di occupazione attuale, richiesta e massimo.

### Scostamenti dal testo della richiesta (scelti dall'utente)
- **Impegno soft per prodotto:** la quantità impegnata si tiene per (prodotto, magazzino) e non per riga di giacenza. Le righe di stock espongono solo la quantità fisica; lotto e ubicazione si scelgono all'evasione via FEFO/FIFO.
- **Movimento "confermato":** non c'è uno stato bozza, ogni movimento nasce già confermato.

### Tecnologia
- BE: PostgreSQL 17 (docker compose per lo sviluppo), sqlx con query a runtime e migrazioni in `be/migrations/`, applicate all'avvio.
- Concorrenza:
  - ogni operazione gira in una transazione;
  - lock advisory per (prodotto, magazzino) su prenotazioni e uscite;
  - lock di riga sulle ubicazioni di destinazione per il controllo di capienza.
- FE: React Router e tipi generati da `api/openapi.yaml` (`openapi-typescript`).
  - Il nome dell'operatore è scelto nella UI e inviato come `X-Operator`.
  - Pagine: prodotti, giacenze e alert, carico, prelievo e scarico, trasferimento, rettifica, ordini di vendita, tracciabilità di lotti e seriali, storico movimenti.

## Assunzioni
- I trasferimenti avvengono solo all'interno dello stesso magazzino (`CROSS_WAREHOUSE_TRANSFER_NOT_SUPPORTED`).
- **Lotti scaduti o bloccati:** non si possono prenotare, scaricare o evadere (`LOT_EXPIRED`, `LOT_BLOCKED`). Si possono trasferire (es. in un'area di quarantena) e rettificare (smaltimento).
- **Rettifica:** il client invia la quantità contata e il sistema calcola il delta. La causale è obbligatoria e presa da un elenco: `COUNT | DAMAGE | LOSS | FOUND | EXPIRED_DISPOSAL | OTHER`, con nota obbligatoria per `OTHER`. Una rettifica negativa è ammessa anche se porta il disponibile sotto zero, perché riflette la realtà fisica, e genera `RESERVATION_AT_RISK`.
- Un prodotto non si può archiviare se ha giacenza o impegni (`PRODUCT_HAS_STOCK`). Un prodotto inattivo non accetta carichi né prenotazioni (`PRODUCT_INACTIVE`).
- Le quantità sono `NUMERIC(18,3)`. Per i prodotti `SERIAL` sono intere e devono coincidere con il numero di seriali indicati.
- **Lotti al carico:** con `tracking = LOT` o `SERIAL` (se `requires_expiry`) il lotto è obbligatorio; con `requires_expiry` anche la scadenza. Un lotto si crea al carico oppure in anticipo con `POST /products/{id}/lots`.
- Lo storage type si controlla solo se il prodotto dichiara `required_storage_type`.
- **Modifica di una riga d'ordine:** se la nuova quantità scende sotto il già impegnato, l'eccedenza viene rilasciata con un movimento `RELEASE`. Non può scendere sotto la quantità già evasa.
- **Annullamento dell'ordine:** rilascia tutti gli impegni. Non è ammesso su un ordine già evaso completamente.

## Punti aperti / rischi
- **Nessuna autenticazione:** `X-Operator` è dichiarativo, quindi chiunque può fare override FEFO, rettifiche e storni. Ruoli reali in una specifica futura.
- Trasferimenti tra magazzini non supportati.
- **Rischio dell'impegno soft:** un impegno non è legato a un lotto, quindi i lotti possono scadere prima dell'evasione e lasciare l'ordine scoperto. Il sistema lo segnala (`RESERVATION_AT_RISK`) ma non lo impedisce.
- Le notifiche push (email, webhook) non sono previste: ci sono solo flag e alert nel payload e `GET /alerts`.
- Le credenziali di Postgres in `.env.example` sono valori di sviluppo locali. Il `.env` reale è escluso da git.
- Il deploy di produzione resta aperto (da 003).
- Valuta unica per prodotto, senza conversioni né valorizzazione del magazzino.
- **Emersi in implementazione:**
  - **Evasione FEFO in più righe:** in una stessa richiesta il controllo FEFO dei prelievi manuali è fatto riga per riga. Un lotto anteriore prelevato da una riga successiva non "giustifica" la riga precedente.
  - **Rettifica:** in aumento non applica il controllo di capienza, perché la merce è già fisicamente nell'ubicazione. Lo storno di una rettifica in diminuzione la reintegra con la stessa regola.
  - **Carico di lotti già scaduti:** è ammesso. La merce risulta subito non utilizzabile e genera l'alert `EXPIRED`.
  - **Idempotenza:** vengono memorizzate solo le risposte 2xx. Dopo un errore la stessa chiave si può riusare.
  - **FE:**
    - l'evasione degli ordini dalla UI è solo `AUTO`; la modalità `MANUAL` esiste solo via API;
    - le tendine di ubicazioni e lotti mostrano al massimo 200 voci (`page_size` massimo);
    - il filtro `status` degli ordini fornitore accetta un solo valore, quindi la pagina di carico fa due chiamate (`OPEN` e `PARTIALLY_RECEIVED`);
    - `Movement` non espone l'unità di misura.

## Impatto
- `api/`: `openapi.yaml` (validato con Redocly), `errors.md`, `examples/` (generati da uno scenario end-to-end reale), `redocly.yaml`.
- `be/`:
  - `migrations/0001_init.sql`: schema, vista `product_warehouse_figures`, trigger di immutabilità;
  - `src/domain/`: FEFO/FIFO, capienza e alert, con unit test;
  - `src/inventory.rs`: primitive transazionali;
  - `src/routes/`: endpoint;
  - `src/idempotency.rs`, `src/http.rs`, `src/error.rs`;
  - `tests/api.rs`: integrazione su Postgres, incluse le prenotazioni concorrenti.
- `fe/`: React Router, client `openapi-fetch` con tipi generati, 10 pagine operative.
- `docker-compose.yml`, `.env.example`, `.gitignore` (`.env` escluso), `README.md`.
