# Catalogo delle eccezioni di dominio

Tutti gli errori seguono RFC 9457 (`Content-Type: application/problem+json`):

```json
{
  "type": "urn:warehouse:error:INSUFFICIENT_AVAILABILITY",
  "title": "Disponibilità insufficiente",
  "status": 409,
  "code": "INSUFFICIENT_AVAILABILITY",
  "detail": "Disponibili 40 PCS di LATTE-UHT-1L nel magazzino MI01, richiesti 60.",
  "details": { "product_id": "…", "sku": "LATTE-UHT-1L", "warehouse_id": "…", "requested": 60, "available": 40 }
}
```

- `code` è stabile e va usato dai client per gestire i casi. `title` e `detail` sono in italiano e servono solo da testo per l'utente.
- `details` contiene i dati strutturati del caso; i campi sono elencati per ogni codice.
- Un'operazione con più righe è atomica: al primo errore non viene registrato nulla. Quando l'errore riguarda una riga, `details.line` ne indica l'indice (da 0).

Criterio per lo stato HTTP:
- **400**: la richiesta è malformata a livello di protocollo, ad esempio manca un header.
- **404**: la risorsa non esiste.
- **405**: il metodo non è ammesso.
- **409**: la richiesta è valida, ma lo stato attuale del magazzino la impedisce (giacenze, capienza, stato degli ordini, unicità).
- **412**: la precondizione `If-Match` non è soddisfatta.
- **422**: i dati inviati violano una regola, a prescindere dallo stato del magazzino.

## Errori generici

| Codice | HTTP | Eccezione | Quando | `details` |
|---|---|---|---|---|
| `VALIDATION_ERROR` | 422 | `ValidationException` | Corpo JSON malformato, campo mancante o fuori dominio, query o path non validi | `errors: [{ field, message }]` oppure `reason` |
| `NOT_FOUND` | 404 | `RisorsaNonTrovataException` | Una risorsa nel path o nel corpo non esiste | `resource`, `id` |
| `OPERATOR_MISSING` | 400 | `OperatoreMancanteException` | Manca l'header `X-Operator` su una richiesta di scrittura | — |
| `METHOD_NOT_ALLOWED` | 405 | — | Metodo HTTP non previsto sulla risorsa | `allow` |
| `VERSION_CONFLICT` | 412 | `ConflittoVersioneException` | `If-Match` diverso dalla versione corrente | `expected`, `current` |
| `IDEMPOTENCY_KEY_REUSED` | 422 | `ChiaveIdempotenzaRiusataException` | `Idempotency-Key` già usata con un corpo, un metodo o un percorso diversi | `key` |
| `IDEMPOTENCY_IN_PROGRESS` | 409 | — | Una richiesta con la stessa chiave è ancora in esecuzione | `key` |
| `INTERNAL_ERROR` | 500 | — | Errore imprevisto (registrato nei log del server) | — |

## Anagrafiche

| Codice | HTTP | Eccezione | Quando | `details` |
|---|---|---|---|---|
| `DUPLICATE_CODE` | 409 | `CodiceDuplicatoException` | Codice di magazzino, zona o ubicazione già usato (l'ubicazione è univoca nel magazzino) | `resource`, `code` |
| `DUPLICATE_SKU` | 409 | `SkuDuplicatoException` | SKU già esistente | `sku` |
| `DUPLICATE_EAN` | 409 | `EanDuplicatoException` | EAN già associato a un altro prodotto | `ean` |
| `DUPLICATE_LOT` | 409 | `LottoDuplicatoException` | Codice lotto già registrato per il prodotto (`POST /products/{id}/lots`) | `product_id`, `lot_code` |
| `DUPLICATE_ORDER_NUMBER` | 409 | `NumeroOrdineDuplicatoException` | Numero d'ordine già usato | `number` |
| `PRODUCT_INACTIVE` | 409 | `ProdottoNonAttivoException` | Carico, prenotazione o nuovo ordine su un prodotto archiviato | `product_id`, `sku` |
| `PRODUCT_HAS_STOCK` | 409 | `ProdottoConGiacenzaException` | Archiviazione di un prodotto con giacenza fisica o impegni aperti | `physical`, `reserved` |
| `WAREHOUSE_INACTIVE` | 409 | `MagazzinoNonAttivoException` | Operazione su un magazzino disattivato | `warehouse_id` |
| `LOCATION_INACTIVE` | 409 | `UbicazioneNonAttivaException` | Carico o trasferimento verso un'ubicazione disattivata | `location_id`, `location_code` |
| `LOCATION_NOT_EMPTY` | 409 | `UbicazioneNonVuotaException` | Disattivazione di un'ubicazione con giacenza | `location_id`, `quantity` |

## Regole di business delle operazioni

| Codice | HTTP | Eccezione | Quando | `details` |
|---|---|---|---|---|
| `INSUFFICIENT_AVAILABILITY` | 409 | `DisponibilitaInsufficienteException` | **Regola 1.** Prenotazione o scarico senza ordine oltre il disponibile del magazzino (`physical − unusable − reserved`) | `product_id`, `sku`, `warehouse_id`, `requested`, `available` |
| `INSUFFICIENT_STOCK` | 409 | `GiacenzaInsufficienteException` | Prelievo, trasferimento, storno o rettifica con seriali che richiede più della giacenza fisica nell'ubicazione e nel lotto indicati. In evasione: le giacenze utilizzabili non coprono l'impegnato, per esempio perché un lotto è scaduto | `product_id`, `location_id`, `lot_id`, `requested`, `on_hand` |
| `LOT_EXPIRED` | 422 | `LottoScadutoException` | Prenotazione, scarico o evasione da un lotto scaduto (`expiry_date < oggi`). Ammessi solo trasferimento e rettifica | `lot_id`, `lot_code`, `expiry_date` |
| `LOT_BLOCKED` | 409 | `LottoBloccatoException` | Scarico o evasione da un lotto bloccato | `lot_id`, `lot_code` |
| `FEFO_VIOLATION` | 409 | `ViolazioneFefoException` | **Regola 2.** Prelievo manuale di un lotto quando nello stesso magazzino c'è un lotto utilizzabile dello stesso prodotto con scadenza anteriore, non interamente prelevato dalla stessa richiesta. Si supera ripetendo la richiesta con `fefo_override: { reason }`, che viene registrato sui movimenti | `product_id`, `picked_lot_id`, `picked_lot_code`, `picked_expiry_date`, `earlier_lots: [{ lot_id, lot_code, expiry_date, quantity }]` |
| `LOCATION_CAPACITY_EXCEEDED` | 409 | `UbicazionePienaException` | **Regola 5.** Carico, trasferimento o storno porterebbero peso o volume dell'ubicazione oltre il massimo. Si verifica anche quando una modifica della capienza la porterebbe sotto l'occupazione attuale | `location_id`, `location_code`, `dimension` (`WEIGHT` o `VOLUME`), `current`, `incoming`, `max` |
| `STORAGE_TYPE_MISMATCH` | 409 | `TipoStoccaggioNonCompatibileException` | Il prodotto richiede un tipo di stoccaggio diverso da quello dell'ubicazione | `required`, `actual` |
| `LOCATION_WAREHOUSE_MISMATCH` | 422 | `UbicazioneFuoriMagazzinoException` | L'ubicazione non appartiene al magazzino dell'operazione o dell'ordine | `location_id`, `warehouse_id` |
| `CROSS_WAREHOUSE_TRANSFER_NOT_SUPPORTED` | 422 | `TrasferimentoTraMagazziniException` | Origine e destinazione sono in magazzini diversi | `from_warehouse_id`, `to_warehouse_id` |
| `SAME_LOCATION_TRANSFER` | 422 | `TrasferimentoStessaUbicazioneException` | Origine e destinazione coincidono | `location_id` |
| `ADJUSTMENT_REASON_REQUIRED` | 422 | `CausaleRettificaObbligatoriaException` | Rettifica senza causale valida, oppure con causale `OTHER` senza nota | `allowed` |

## Lotti e seriali

| Codice | HTTP | Eccezione | Quando | `details` |
|---|---|---|---|---|
| `LOT_REQUIRED` | 422 | `LottoObbligatorioException` | Prodotto `LOT` (o `SERIAL` con `requires_expiry`) senza lotto, in carico, scarico manuale, trasferimento o rettifica | `product_id` |
| `LOT_NOT_ALLOWED` | 422 | `LottoNonPrevistoException` | Lotto indicato per un prodotto con `tracking = NONE` | `product_id` |
| `EXPIRY_DATE_REQUIRED` | 422 | `ScadenzaObbligatoriaException` | Nuovo lotto senza scadenza per un prodotto con `requires_expiry` | `product_id`, `lot_code` |
| `LOT_PRODUCT_MISMATCH` | 422 | `LottoProdottoNonCoerenteException` | Il lotto indicato appartiene a un altro prodotto | `lot_id`, `product_id` |
| `LOT_DATA_MISMATCH` | 422 | `DatiLottoNonCoerentiException` | In carico, un lotto esistente viene indicato con date diverse da quelle registrate | `lot_code`, `field`, `registered`, `received` |
| `SERIALS_REQUIRED` | 422 | `SerialiObbligatoriException` | Movimento di un prodotto `SERIAL` senza seriali (eccetto `AUTO`, dove vengono scelti dal sistema) | `product_id` |
| `SERIALS_NOT_ALLOWED` | 422 | `SerialiNonPrevistiException` | Seriali indicati per un prodotto non tracciato a seriale | `product_id` |
| `SERIAL_COUNT_MISMATCH` | 422 | `NumeroSerialiNonCoerenteException` | Il numero di seriali è diverso dalla quantità, oppure la quantità non è intera | `quantity`, `serials` |
| `SERIAL_ALREADY_EXISTS` | 409 | `SerialeDuplicatoException` | Carico (o rettifica in aumento) di un seriale già presente in magazzino. Un seriale `SHIPPED` o `SCRAPPED` può rientrare | `serial_number` |
| `SERIAL_NOT_AVAILABLE` | 409 | `SerialeNonDisponibileException` | Seriale inesistente, non `IN_STOCK`, o non presente nell'ubicazione o nel lotto indicati | `serial_number`, `status`, `location_id` |

## Movimenti

| Codice | HTTP | Eccezione | Quando | `details` |
|---|---|---|---|---|
| `MOVEMENT_IMMUTABLE` | 405 | `MovimentoImmutabileException` | **Regola 4.** `PUT`, `PATCH` o `DELETE` su un movimento. La correzione si fa con `POST /movements/{id}/reversal`. Anche il database rifiuta `UPDATE` e `DELETE` sulla tabella dei movimenti | `movement_id` |
| `MOVEMENT_ALREADY_REVERSED` | 409 | `MovimentoGiaStornatoException` | Storno di un movimento già stornato | `movement_id`, `reversed_by_movement_id` |
| `MOVEMENT_NOT_REVERSIBLE` | 422 | `MovimentoNonStornabileException` | Storno di uno storno, oppure di un movimento `RESERVATION` o `RELEASE` (si gestiscono con prenotazione e rilascio dell'ordine) | `movement_id`, `type` |

## Ordini

| Codice | HTTP | Eccezione | Quando | `details` |
|---|---|---|---|---|
| `ORDER_STATE_INVALID` | 409 | `StatoOrdineNonValidoException` | Operazione non ammessa nello stato dell'ordine: prenotare, evadere o modificare un ordine `CANCELLED` o `FULFILLED`, ricevere un ordine fornitore `CANCELLED` o `RECEIVED` | `order_id`, `status`, `action` |
| `ORDER_LINE_NOT_FOUND` | 422 | `RigaOrdineNonTrovataException` | `line_id` non appartiene all'ordine | `line_id` |
| `RESERVATION_EXCEEDS_ORDERED` | 409 | `ImpegnoOltreOrdinatoException` | Si chiede di impegnare più del residuo della riga (`ordered − reserved − fulfilled`) | `line_id`, `requested`, `open` |
| `RELEASE_EXCEEDS_RESERVED` | 409 | `RilascioOltreImpegnatoException` | Si chiede di rilasciare più dell'impegnato della riga | `line_id`, `requested`, `reserved` |
| `FULFILLMENT_EXCEEDS_RESERVED` | 409 | `EvasioneOltreImpegnatoException` | Si chiede di evadere più dell'impegnato della riga | `line_id`, `requested`, `reserved` |
| `QUANTITY_BELOW_FULFILLED` | 409 | `QuantitaSottoEvasoException` | La nuova quantità di una riga è inferiore a quella già evasa | `line_id`, `quantity`, `fulfilled` |
| `RECEIPT_EXCEEDS_ORDERED` | 409 | `CaricoOltreOrdinatoException` | Il carico supera il residuo della riga dell'ordine fornitore | `purchase_order_line_id`, `requested`, `open` |
| `PURCHASE_ORDER_MISMATCH` | 422 | `OrdineFornitoreNonCoerenteException` | La riga d'ordine fornitore non appartiene all'ordine, il prodotto è diverso, o l'ordine è di un altro magazzino | `purchase_order_id`, `purchase_order_line_id` |

## Alert (non sono errori)

Le risposte delle operazioni (`alerts[]`), la disponibilità del prodotto e `GET /alerts` segnalano condizioni che non bloccano l'operazione:

| Tipo | Severità | Condizione |
|---|---|---|
| `LOW_STOCK` | `WARNING` | `available < min_stock` per prodotto e magazzino (solo se `min_stock > 0`) |
| `EXPIRING` | `WARNING` | Lotto con giacenza che scade entro `expiry_warning_days` |
| `EXPIRED` | `CRITICAL` | Lotto con giacenza già scaduto (quantità non utilizzabile) |
| `RESERVATION_AT_RISK` | `CRITICAL` | `available < 0`: gli impegni superano le giacenze utilizzabili, per esempio dopo una rettifica negativa o la scadenza di un lotto |
