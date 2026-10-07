//! Errori di dominio esposti come RFC 9457 `application/problem+json` (catalogo in `api/errors.md`).

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub detail: String,
    pub details: Value,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status,
            code,
            detail: detail.into(),
            details: Value::Null,
        }
    }

    pub fn with(mut self, details: Value) -> Self {
        self.details = details;
        self
    }

    /// Indice della riga della richiesta che ha causato l'errore.
    pub fn at_line(mut self, line: usize) -> Self {
        match &mut self.details {
            Value::Object(map) => {
                map.insert("line".into(), json!(line));
            }
            _ => self.details = json!({ "line": line }),
        }
        self
    }

    pub fn validation(detail: impl Into<String>) -> Self {
        let detail = detail.into();
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR", detail.clone())
            .with(json!({ "reason": detail }))
    }

    pub fn not_found(resource: &str, id: impl std::fmt::Display) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            format!("{resource} {id} non trovato"),
        )
        .with(json!({ "resource": resource, "id": id.to_string() }))
    }

    pub fn conflict(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, code, detail)
    }

    pub fn unprocessable(code: &'static str, detail: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, detail)
    }

    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR",
            "Errore interno del server",
        )
    }

    pub fn body(&self) -> Value {
        let mut body = json!({
            "type": format!("urn:warehouse:error:{}", self.code),
            "title": title(self.code),
            "status": self.status.as_u16(),
            "code": self.code,
            "detail": self.detail,
        });
        if !self.details.is_null() {
            body["details"] = self.details.clone();
        }
        crate::http::normalize_numbers(&mut body);
        body
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut res = (self.status, axum::Json(self.body())).into_response();
        res.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        res
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &err
            && db.is_unique_violation()
        {
            let constraint = db.constraint().unwrap_or_default();
            let (code, detail) = match constraint {
                "products_sku_key" => ("DUPLICATE_SKU", "SKU già esistente"),
                "products_ean_key" => ("DUPLICATE_EAN", "EAN già associato a un altro prodotto"),
                "lots_product_id_lot_code_key" => ("DUPLICATE_LOT", "Lotto già registrato per il prodotto"),
                "sales_orders_number_key" | "purchase_orders_number_key" => {
                    ("DUPLICATE_ORDER_NUMBER", "Numero d'ordine già usato")
                }
                "warehouses_code_key" | "zones_warehouse_id_code_key" | "locations_warehouse_id_code_key" => {
                    ("DUPLICATE_CODE", "Codice già usato")
                }
                _ => ("DUPLICATE_CODE", "Valore duplicato"),
            };
            return ApiError::conflict(code, detail).with(json!({ "constraint": constraint }));
        }
        tracing::error!(error = %err, "errore del database");
        ApiError::internal()
    }
}

/// Titolo (in italiano) per ogni codice applicativo.
pub fn title(code: &str) -> &'static str {
    match code {
        "VALIDATION_ERROR" => "Dati non validi",
        "NOT_FOUND" => "Risorsa non trovata",
        "OPERATOR_MISSING" => "Operatore mancante",
        "METHOD_NOT_ALLOWED" => "Metodo non ammesso",
        "VERSION_CONFLICT" => "Conflitto di versione",
        "IDEMPOTENCY_KEY_REUSED" => "Chiave di idempotenza riutilizzata",
        "IDEMPOTENCY_IN_PROGRESS" => "Richiesta già in esecuzione",
        "DUPLICATE_CODE" => "Codice duplicato",
        "DUPLICATE_SKU" => "SKU duplicato",
        "DUPLICATE_EAN" => "EAN duplicato",
        "DUPLICATE_LOT" => "Lotto duplicato",
        "DUPLICATE_ORDER_NUMBER" => "Numero d'ordine duplicato",
        "PRODUCT_INACTIVE" => "Prodotto non attivo",
        "PRODUCT_HAS_STOCK" => "Prodotto con giacenza",
        "WAREHOUSE_INACTIVE" => "Magazzino non attivo",
        "LOCATION_INACTIVE" => "Ubicazione non attiva",
        "LOCATION_NOT_EMPTY" => "Ubicazione non vuota",
        "LOCATION_WAREHOUSE_MISMATCH" => "Ubicazione di un altro magazzino",
        "STORAGE_TYPE_MISMATCH" => "Tipo di stoccaggio non compatibile",
        "LOCATION_CAPACITY_EXCEEDED" => "Capienza dell'ubicazione superata",
        "INSUFFICIENT_STOCK" => "Giacenza insufficiente",
        "INSUFFICIENT_AVAILABILITY" => "Disponibilità insufficiente",
        "LOT_EXPIRED" => "Lotto scaduto",
        "LOT_BLOCKED" => "Lotto bloccato",
        "LOT_REQUIRED" => "Lotto obbligatorio",
        "LOT_NOT_ALLOWED" => "Lotto non previsto",
        "EXPIRY_DATE_REQUIRED" => "Data di scadenza obbligatoria",
        "LOT_PRODUCT_MISMATCH" => "Lotto di un altro prodotto",
        "LOT_DATA_MISMATCH" => "Dati del lotto non coerenti",
        "FEFO_VIOLATION" => "Violazione FEFO",
        "SERIALS_REQUIRED" => "Seriali obbligatori",
        "SERIALS_NOT_ALLOWED" => "Seriali non previsti",
        "SERIAL_COUNT_MISMATCH" => "Numero di seriali non coerente",
        "SERIAL_ALREADY_EXISTS" => "Seriale già presente",
        "SERIAL_NOT_AVAILABLE" => "Seriale non disponibile",
        "SAME_LOCATION_TRANSFER" => "Trasferimento sulla stessa ubicazione",
        "CROSS_WAREHOUSE_TRANSFER_NOT_SUPPORTED" => "Trasferimento tra magazzini non supportato",
        "ADJUSTMENT_REASON_REQUIRED" => "Causale di rettifica obbligatoria",
        "MOVEMENT_IMMUTABLE" => "Movimento immutabile",
        "MOVEMENT_ALREADY_REVERSED" => "Movimento già stornato",
        "MOVEMENT_NOT_REVERSIBLE" => "Movimento non stornabile",
        "ORDER_STATE_INVALID" => "Stato dell'ordine non valido",
        "ORDER_LINE_NOT_FOUND" => "Riga d'ordine non trovata",
        "RESERVATION_EXCEEDS_ORDERED" => "Impegno oltre l'ordinato",
        "RELEASE_EXCEEDS_RESERVED" => "Rilascio oltre l'impegnato",
        "FULFILLMENT_EXCEEDS_RESERVED" => "Evasione oltre l'impegnato",
        "QUANTITY_BELOW_FULFILLED" => "Quantità inferiore all'evaso",
        "RECEIPT_EXCEEDS_ORDERED" => "Carico oltre l'ordinato",
        "PURCHASE_ORDER_MISMATCH" => "Ordine fornitore non coerente",
        _ => "Errore interno",
    }
}
