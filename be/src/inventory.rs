//! Primitive transazionali sulle giacenze, condivise da operazioni, ordini e storni.
//!
//! Tutte le funzioni lavorano dentro la transazione del chiamante. Le scritture che toccano la
//! disponibilità di un (prodotto, magazzino) devono prima chiamare [`lock_pairs`].

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::alerts::{self, Alert};
use crate::domain::capacity::{self, Load};
use crate::domain::picking::{self, Candidate, Strategy};
use crate::error::{ApiError, ApiResult};
use crate::model::{Lot, Movement, Product};
use crate::{lot_select, movement_select, product_select};

// ---------------------------------------------------------------- Caricamento

pub async fn product(conn: &mut PgConnection, id: Uuid) -> ApiResult<Product> {
    sqlx::query_as(concat!(product_select!(), "WHERE p.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| ApiError::not_found("product", id))
}

pub fn ensure_active(p: &Product) -> ApiResult<()> {
    if !p.is_active() {
        return Err(
            ApiError::conflict("PRODUCT_INACTIVE", format!("Il prodotto {} è archiviato", p.sku))
                .with(json!({ "product_id": p.id, "sku": p.sku })),
        );
    }
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WarehouseRef {
    pub id: Uuid,
    pub code: String,
    pub active: bool,
}

pub async fn warehouse(conn: &mut PgConnection, id: Uuid) -> ApiResult<WarehouseRef> {
    sqlx::query_as("SELECT id, code, active FROM warehouses WHERE id = $1")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| ApiError::not_found("warehouse", id))
}

pub async fn active_warehouse(conn: &mut PgConnection, id: Uuid) -> ApiResult<WarehouseRef> {
    let w = warehouse(conn, id).await?;
    if !w.active {
        return Err(
            ApiError::conflict("WAREHOUSE_INACTIVE", format!("Il magazzino {} non è attivo", w.code))
                .with(json!({ "warehouse_id": id })),
        );
    }
    Ok(w)
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LocationRef {
    pub id: Uuid,
    pub warehouse_id: Uuid,
    pub code: String,
    pub storage_type: String,
    pub max_weight_kg: Option<Decimal>,
    pub max_volume_m3: Option<Decimal>,
    pub active: bool,
}

/// Carica un'ubicazione; con `lock` la blocca fino a fine transazione (controllo di capienza).
pub async fn location(conn: &mut PgConnection, id: Uuid, lock: bool) -> ApiResult<LocationRef> {
    let sql = if lock {
        "SELECT id, warehouse_id, code, storage_type, max_weight_kg, max_volume_m3, active
         FROM locations WHERE id = $1 FOR UPDATE"
    } else {
        "SELECT id, warehouse_id, code, storage_type, max_weight_kg, max_volume_m3, active
         FROM locations WHERE id = $1"
    };
    sqlx::query_as(sql)
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| ApiError::not_found("location", id))
}

pub fn ensure_in_warehouse(loc: &LocationRef, warehouse_id: Uuid) -> ApiResult<()> {
    if loc.warehouse_id != warehouse_id {
        return Err(ApiError::unprocessable(
            "LOCATION_WAREHOUSE_MISMATCH",
            format!("L'ubicazione {} non appartiene al magazzino dell'operazione", loc.code),
        )
        .with(json!({ "location_id": loc.id, "warehouse_id": warehouse_id })));
    }
    Ok(())
}

pub async fn lot(conn: &mut PgConnection, id: Uuid) -> ApiResult<Lot> {
    sqlx::query_as(concat!(lot_select!(), "WHERE l.id = $1"))
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| ApiError::not_found("lot", id))
}

/// Carica il lotto indicato per un movimento e ne verifica la coerenza con il prodotto.
pub async fn lot_for_product(conn: &mut PgConnection, p: &Product, lot_id: Option<Uuid>) -> ApiResult<Option<Lot>> {
    match lot_id {
        None if p.lot_required() => Err(ApiError::unprocessable(
            "LOT_REQUIRED",
            format!("Il prodotto {} è tracciato a lotto: indicare il lotto", p.sku),
        )
        .with(json!({ "product_id": p.id }))),
        None => Ok(None),
        Some(_) if !p.lot_allowed() => Err(ApiError::unprocessable(
            "LOT_NOT_ALLOWED",
            format!("Il prodotto {} non è tracciato a lotto", p.sku),
        )
        .with(json!({ "product_id": p.id }))),
        Some(id) => {
            let l = lot(conn, id).await?;
            if l.product_id != p.id {
                return Err(ApiError::unprocessable(
                    "LOT_PRODUCT_MISMATCH",
                    format!("Il lotto {} appartiene a un altro prodotto", l.lot_code),
                )
                .with(json!({ "lot_id": id, "product_id": p.id })));
            }
            Ok(Some(l))
        }
    }
}

/// Un lotto scaduto o bloccato non si scarica, non si evade e non si prenota.
pub fn ensure_lot_usable(l: &Lot) -> ApiResult<()> {
    if l.expired {
        return Err(ApiError::unprocessable(
            "LOT_EXPIRED",
            format!(
                "Il lotto {} è scaduto il {}",
                l.lot_code,
                l.expiry_date.map(|d| d.to_string()).unwrap_or_default()
            ),
        )
        .with(json!({ "lot_id": l.id, "lot_code": l.lot_code, "expiry_date": l.expiry_date })));
    }
    if l.is_blocked() {
        return Err(
            ApiError::conflict("LOT_BLOCKED", format!("Il lotto {} è bloccato", l.lot_code))
                .with(json!({ "lot_id": l.id, "lot_code": l.lot_code })),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- Lock

/// Serializza le operazioni sullo stesso (prodotto, magazzino), in ordine stabile per evitare deadlock.
pub async fn lock_pairs(conn: &mut PgConnection, pairs: &[(Uuid, Uuid)]) -> ApiResult<()> {
    let mut pairs = pairs.to_vec();
    pairs.sort();
    pairs.dedup();
    for (product_id, warehouse_id) in pairs {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1::text || ':' || $2::text, 0))")
            .bind(product_id)
            .bind(warehouse_id)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

// ---------------------------------------------------------------- Disponibilità

#[derive(Debug, Clone, Default, Serialize, sqlx::FromRow)]
pub struct Figures {
    pub physical: Decimal,
    pub unusable: Decimal,
    pub reserved: Decimal,
    pub available: Decimal,
    pub expiring: Decimal,
    pub expired: Decimal,
}

pub async fn figures(conn: &mut PgConnection, product_id: Uuid, warehouse_id: Uuid) -> ApiResult<Figures> {
    Ok(sqlx::query_as(
        "SELECT physical, unusable, reserved, available, expiring, expired
         FROM product_warehouse_figures WHERE product_id = $1 AND warehouse_id = $2",
    )
    .bind(product_id)
    .bind(warehouse_id)
    .fetch_optional(conn)
    .await?
    .unwrap_or_default())
}

pub fn insufficient_availability(
    p: &Product,
    warehouse: &WarehouseRef,
    requested: Decimal,
    available: Decimal,
) -> ApiError {
    ApiError::conflict(
        "INSUFFICIENT_AVAILABILITY",
        format!(
            "Disponibili {} {} di {} nel magazzino {}, richiesti {}",
            available.max(Decimal::ZERO).normalize(),
            p.uom,
            p.sku,
            warehouse.code,
            requested.normalize()
        ),
    )
    .with(json!({
        "product_id": p.id, "sku": p.sku, "warehouse_id": warehouse.id,
        "requested": requested, "available": available,
    }))
}

/// Alert correnti per le coppie (prodotto, magazzino) toccate da un'operazione.
pub async fn alerts_for(conn: &mut PgConnection, pairs: &[(Uuid, Uuid)]) -> ApiResult<Vec<Alert>> {
    let mut pairs = pairs.to_vec();
    pairs.sort();
    pairs.dedup();
    let (products, warehouses): (Vec<Uuid>, Vec<Uuid>) = pairs.into_iter().unzip();
    alert_query(
        conn,
        AlertFilter {
            pairs: Some((products, warehouses)),
            ..Default::default()
        },
    )
    .await
}

#[derive(Default)]
pub struct AlertFilter {
    pub pairs: Option<(Vec<Uuid>, Vec<Uuid>)>,
    pub warehouse_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct FigureRow {
    product_id: Uuid,
    sku: String,
    min_stock: Decimal,
    warehouse_id: Uuid,
    warehouse_code: String,
    available: Decimal,
}

#[derive(sqlx::FromRow)]
struct LotRow {
    product_id: Uuid,
    sku: String,
    warehouse_id: Uuid,
    warehouse_code: String,
    lot_id: Uuid,
    lot_code: String,
    expiry_date: NaiveDate,
    expiry_warning_days: i32,
    quantity: Decimal,
    today: NaiveDate,
}

/// Calcola gli alert dei prodotti attivi. Le coppie considerate sono quelle con giacenza, impegni
/// o movimenti passati, così un prodotto esaurito continua a segnalare il sotto-scorta.
pub async fn alert_query(conn: &mut PgConnection, f: AlertFilter) -> ApiResult<Vec<Alert>> {
    let (pair_products, pair_warehouses) = f.pairs.unwrap_or_default();
    let use_pairs = !pair_products.is_empty();
    let rows: Vec<FigureRow> = sqlx::query_as(
        "WITH pairs AS (
             SELECT product_id, warehouse_id FROM product_warehouse_figures
             UNION SELECT DISTINCT product_id, warehouse_id FROM movements
         )
         SELECT p.id AS product_id, p.sku, p.min_stock, w.id AS warehouse_id, w.code AS warehouse_code,
                COALESCE(f.available, 0) AS available
         FROM pairs x
         JOIN products p ON p.id = x.product_id AND p.status = 'ACTIVE'
         JOIN warehouses w ON w.id = x.warehouse_id
         LEFT JOIN product_warehouse_figures f ON f.product_id = x.product_id AND f.warehouse_id = x.warehouse_id
         WHERE ($1::uuid IS NULL OR x.warehouse_id = $1)
           AND ($2::uuid IS NULL OR x.product_id = $2)
           AND (NOT $3 OR (x.product_id, x.warehouse_id) IN (SELECT * FROM unnest($4::uuid[], $5::uuid[])))
         ORDER BY p.sku, w.code",
    )
    .bind(f.warehouse_id)
    .bind(f.product_id)
    .bind(use_pairs)
    .bind(&pair_products)
    .bind(&pair_warehouses)
    .fetch_all(&mut *conn)
    .await?;

    let mut out = Vec::new();
    for r in rows {
        out.extend(alerts::quantity_alerts(&alerts::Figures {
            warehouse_id: r.warehouse_id,
            warehouse_code: r.warehouse_code,
            product_id: r.product_id,
            sku: r.sku,
            min_stock: r.min_stock,
            available: r.available,
        }));
    }

    let lots: Vec<LotRow> = sqlx::query_as(
        "SELECT p.id AS product_id, p.sku, w.id AS warehouse_id, w.code AS warehouse_code, l.id AS lot_id,
                l.lot_code, l.expiry_date, p.expiry_warning_days, sum(s.quantity) AS quantity, CURRENT_DATE AS today
         FROM stock s
         JOIN lots l ON l.id = s.lot_id
         JOIN products p ON p.id = s.product_id AND p.status = 'ACTIVE'
         JOIN warehouses w ON w.id = s.warehouse_id
         WHERE l.expiry_date IS NOT NULL AND s.quantity > 0
           AND l.expiry_date <= CURRENT_DATE + p.expiry_warning_days
           AND ($1::uuid IS NULL OR s.warehouse_id = $1)
           AND ($2::uuid IS NULL OR s.product_id = $2)
           AND (NOT $3 OR (s.product_id, s.warehouse_id) IN (SELECT * FROM unnest($4::uuid[], $5::uuid[])))
         GROUP BY p.id, p.sku, w.id, w.code, l.id, l.lot_code, l.expiry_date, p.expiry_warning_days
         ORDER BY l.expiry_date, p.sku",
    )
    .bind(f.warehouse_id)
    .bind(f.product_id)
    .bind(use_pairs)
    .bind(&pair_products)
    .bind(&pair_warehouses)
    .fetch_all(&mut *conn)
    .await?;

    for l in lots {
        let today = l.today;
        if let Some(a) = alerts::expiry_alert(
            &alerts::LotOnHand {
                warehouse_id: l.warehouse_id,
                warehouse_code: l.warehouse_code,
                product_id: l.product_id,
                sku: l.sku,
                lot_id: l.lot_id,
                lot_code: l.lot_code,
                expiry_date: l.expiry_date,
                warning_days: l.expiry_warning_days,
                quantity: l.quantity,
            },
            today,
        ) {
            out.push(a);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- Giacenza fisica

pub fn load_of(p: &Product, quantity: Decimal) -> Load {
    Load {
        weight_kg: p.unit_weight_kg * quantity,
        volume_m3: p.unit_volume_m3 * quantity,
    }
}

pub async fn location_load(conn: &mut PgConnection, location_id: Uuid) -> ApiResult<Load> {
    let (weight_kg, volume_m3): (Decimal, Decimal) = sqlx::query_as(
        "SELECT COALESCE(sum(s.quantity * p.unit_weight_kg), 0), COALESCE(sum(s.quantity * p.unit_volume_m3), 0)
         FROM stock s JOIN products p ON p.id = s.product_id WHERE s.location_id = $1",
    )
    .bind(location_id)
    .fetch_one(conn)
    .await?;
    Ok(Load { weight_kg, volume_m3 })
}

/// Controlli sull'ubicazione di destinazione di un carico, trasferimento o storno: attiva,
/// tipo di stoccaggio compatibile e capienza (l'ubicazione va caricata con `lock = true`).
pub async fn check_destination(
    conn: &mut PgConnection,
    p: &Product,
    loc: &LocationRef,
    quantity: Decimal,
) -> ApiResult<()> {
    if !loc.active {
        return Err(
            ApiError::conflict("LOCATION_INACTIVE", format!("L'ubicazione {} non è attiva", loc.code))
                .with(json!({ "location_id": loc.id, "location_code": loc.code })),
        );
    }
    if let Some(required) = &p.required_storage_type
        && *required != loc.storage_type
    {
        return Err(ApiError::conflict(
            "STORAGE_TYPE_MISMATCH",
            format!(
                "Il prodotto {} richiede stoccaggio {required}, l'ubicazione {} è {}",
                p.sku, loc.code, loc.storage_type
            ),
        )
        .with(json!({ "required": required, "actual": loc.storage_type, "location_id": loc.id })));
    }
    let current = location_load(conn, loc.id).await?;
    capacity::check(current, load_of(p, quantity), loc.max_weight_kg, loc.max_volume_m3).map_err(|e| {
        let (label, unit) = match e.dimension {
            capacity::Dimension::Weight => ("il peso massimo", "kg"),
            capacity::Dimension::Volume => ("il volume massimo", "m³"),
        };
        ApiError::conflict(
            "LOCATION_CAPACITY_EXCEEDED",
            format!(
                "L'ubicazione {} supererebbe {label} ({} {unit} + {} {unit} > {} {unit})",
                loc.code,
                e.current.normalize(),
                e.incoming.normalize(),
                e.max.normalize()
            ),
        )
        .with(json!({
            "location_id": loc.id, "location_code": loc.code, "dimension": e.dimension,
            "current": e.current.normalize(), "incoming": e.incoming.normalize(), "max": e.max.normalize(),
        }))
    })
}

pub async fn add_stock(
    conn: &mut PgConnection,
    warehouse_id: Uuid,
    location_id: Uuid,
    product_id: Uuid,
    lot_id: Option<Uuid>,
    quantity: Decimal,
    received_at: DateTime<Utc>,
) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO stock (warehouse_id, location_id, product_id, lot_id, quantity, received_at)
         VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT ON CONSTRAINT stock_product_id_lot_id_location_id_key
         DO UPDATE SET quantity = stock.quantity + EXCLUDED.quantity, updated_at = now()",
    )
    .bind(warehouse_id)
    .bind(location_id)
    .bind(product_id)
    .bind(lot_id)
    .bind(quantity)
    .bind(received_at)
    .execute(conn)
    .await?;
    Ok(())
}

/// Preleva dalla riga di giacenza; restituisce la data di ricevimento della riga (per i trasferimenti).
/// Le righe che arrivano a zero vengono eliminate.
pub async fn remove_stock(
    conn: &mut PgConnection,
    location_id: Uuid,
    product_id: Uuid,
    lot_id: Option<Uuid>,
    quantity: Decimal,
) -> ApiResult<DateTime<Utc>> {
    let row: Option<(Uuid, Decimal, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, quantity, received_at FROM stock
         WHERE location_id = $1 AND product_id = $2 AND lot_id IS NOT DISTINCT FROM $3 FOR UPDATE",
    )
    .bind(location_id)
    .bind(product_id)
    .bind(lot_id)
    .fetch_optional(&mut *conn)
    .await?;
    let on_hand = row.as_ref().map(|r| r.1).unwrap_or_default();
    let Some((id, current, received_at)) = row.filter(|r| r.1 >= quantity) else {
        return Err(ApiError::conflict(
            "INSUFFICIENT_STOCK",
            format!(
                "Giacenza insufficiente nell'ubicazione: presenti {}, richiesti {}",
                on_hand.normalize(),
                quantity.normalize()
            ),
        )
        .with(json!({
            "product_id": product_id, "location_id": location_id, "lot_id": lot_id,
            "requested": quantity, "on_hand": on_hand,
        })));
    };
    if current == quantity {
        sqlx::query("DELETE FROM stock WHERE id = $1")
            .bind(id)
            .execute(conn)
            .await?;
    } else {
        sqlx::query("UPDATE stock SET quantity = quantity - $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(quantity)
            .execute(conn)
            .await?;
    }
    Ok(received_at)
}

pub async fn on_hand(
    conn: &mut PgConnection,
    location_id: Uuid,
    product_id: Uuid,
    lot_id: Option<Uuid>,
) -> ApiResult<Decimal> {
    let q: Option<Decimal> = sqlx::query_scalar(
        "SELECT quantity FROM stock WHERE location_id = $1 AND product_id = $2 AND lot_id IS NOT DISTINCT FROM $3 FOR UPDATE",
    )
    .bind(location_id)
    .bind(product_id)
    .bind(lot_id)
    .fetch_optional(conn)
    .await?;
    Ok(q.unwrap_or_default())
}

// ---------------------------------------------------------------- Seriali

/// Verifica che i seriali siano coerenti con la quantità (intera, uno per pezzo).
pub fn check_serial_count(p: &Product, quantity: Decimal, serials: &[String]) -> ApiResult<()> {
    if !p.tracks_serials() {
        if !serials.is_empty() {
            return Err(ApiError::unprocessable(
                "SERIALS_NOT_ALLOWED",
                format!("Il prodotto {} non è tracciato a seriale", p.sku),
            )
            .with(json!({ "product_id": p.id })));
        }
        return Ok(());
    }
    if serials.is_empty() {
        return Err(ApiError::unprocessable(
            "SERIALS_REQUIRED",
            format!("Il prodotto {} è tracciato a seriale: indicare i seriali", p.sku),
        )
        .with(json!({ "product_id": p.id })));
    }
    let mut unique = serials.to_vec();
    unique.sort();
    unique.dedup();
    if quantity.fract() != Decimal::ZERO || Decimal::from(serials.len()) != quantity || unique.len() != serials.len() {
        return Err(ApiError::unprocessable(
            "SERIAL_COUNT_MISMATCH",
            "Il numero di seriali (distinti) deve coincidere con la quantità, che deve essere intera",
        )
        .with(json!({ "quantity": quantity, "serials": serials.len() })));
    }
    if serials.iter().any(|s| s.trim().is_empty()) {
        return Err(ApiError::validation("I numeri seriali non possono essere vuoti"));
    }
    Ok(())
}

pub fn check_whole_units(p: &Product, quantity: Decimal) -> ApiResult<()> {
    if p.tracks_serials() && quantity.fract() != Decimal::ZERO {
        return Err(ApiError::unprocessable(
            "SERIAL_COUNT_MISMATCH",
            "Per i prodotti a seriale la quantità deve essere intera",
        )
        .with(json!({ "quantity": quantity })));
    }
    Ok(())
}

/// Registra l'ingresso di seriali: nuovi, oppure rientro di seriali spediti o smaltiti.
pub async fn receive_serials(
    conn: &mut PgConnection,
    p: &Product,
    numbers: &[String],
    lot_id: Option<Uuid>,
    location_id: Uuid,
) -> ApiResult<Vec<Uuid>> {
    let mut ids = Vec::with_capacity(numbers.len());
    for n in numbers {
        let n = n.trim();
        let existing: Option<(Uuid, String)> =
            sqlx::query_as("SELECT id, status FROM serials WHERE product_id = $1 AND serial_number = $2 FOR UPDATE")
                .bind(p.id)
                .bind(n)
                .fetch_optional(&mut *conn)
                .await?;
        let id = match existing {
            Some((_, status)) if status == "IN_STOCK" => {
                return Err(
                    ApiError::conflict("SERIAL_ALREADY_EXISTS", format!("Il seriale {n} è già in giacenza"))
                        .with(json!({ "serial_number": n })),
                );
            }
            Some((id, _)) => {
                sqlx::query("UPDATE serials SET status = 'IN_STOCK', location_id = $2, lot_id = $3, updated_at = now() WHERE id = $1")
                    .bind(id)
                    .bind(location_id)
                    .bind(lot_id)
                    .execute(&mut *conn)
                    .await?;
                id
            }
            None => {
                sqlx::query_scalar(
                    "INSERT INTO serials (product_id, serial_number, lot_id, location_id, status)
                     VALUES ($1, $2, $3, $4, 'IN_STOCK') RETURNING id",
                )
                .bind(p.id)
                .bind(n)
                .bind(lot_id)
                .bind(location_id)
                .fetch_one(&mut *conn)
                .await?
            }
        };
        ids.push(id);
    }
    Ok(ids)
}

/// Risolve seriali che devono essere in giacenza nell'ubicazione e nel lotto indicati.
pub async fn serials_in_stock(
    conn: &mut PgConnection,
    p: &Product,
    numbers: &[String],
    location_id: Uuid,
    lot_id: Option<Uuid>,
) -> ApiResult<Vec<Uuid>> {
    let mut ids = Vec::with_capacity(numbers.len());
    for n in numbers {
        let n = n.trim();
        let row: Option<(Uuid, String, Option<Uuid>, Option<Uuid>)> = sqlx::query_as(
            "SELECT id, status, location_id, lot_id FROM serials WHERE product_id = $1 AND serial_number = $2 FOR UPDATE",
        )
        .bind(p.id)
        .bind(n)
        .fetch_optional(&mut *conn)
        .await?;
        match row {
            Some((id, status, loc, lot)) if status == "IN_STOCK" && loc == Some(location_id) && lot == lot_id => {
                ids.push(id)
            }
            other => {
                return Err(ApiError::conflict(
                    "SERIAL_NOT_AVAILABLE",
                    format!("Il seriale {n} non è in giacenza nell'ubicazione e nel lotto indicati"),
                )
                .with(json!({
                    "serial_number": n,
                    "status": other.as_ref().map(|r| r.1.clone()),
                    "location_id": other.and_then(|r| r.2),
                })));
            }
        }
    }
    Ok(ids)
}

/// Sceglie in automatico `count` seriali presenti nell'ubicazione e nel lotto.
pub async fn pick_serials(
    conn: &mut PgConnection,
    product_id: Uuid,
    location_id: Uuid,
    lot_id: Option<Uuid>,
    count: Decimal,
) -> ApiResult<Vec<Uuid>> {
    let n = count.trunc().to_i64().unwrap_or(0);
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM serials
         WHERE product_id = $1 AND location_id = $2 AND lot_id IS NOT DISTINCT FROM $3 AND status = 'IN_STOCK'
         ORDER BY serial_number LIMIT $4 FOR UPDATE",
    )
    .bind(product_id)
    .bind(location_id)
    .bind(lot_id)
    .bind(n)
    .fetch_all(conn)
    .await?;
    if ids.len() as i64 != n {
        return Err(ApiError::conflict(
            "SERIAL_NOT_AVAILABLE",
            "Seriali in giacenza non coerenti con la quantità dell'ubicazione",
        )
        .with(json!({ "location_id": location_id, "requested": n, "found": ids.len() })));
    }
    Ok(ids)
}

pub async fn set_serials(
    conn: &mut PgConnection,
    ids: &[Uuid],
    status: &str,
    location_id: Option<Uuid>,
) -> ApiResult<()> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query("UPDATE serials SET status = $2, location_id = $3, updated_at = now() WHERE id = ANY($1)")
        .bind(ids)
        .bind(status)
        .bind(location_id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn movement_serial_ids(conn: &mut PgConnection, movement_id: Uuid) -> ApiResult<Vec<Uuid>> {
    Ok(
        sqlx::query_scalar("SELECT serial_id FROM movement_serials WHERE movement_id = $1")
            .bind(movement_id)
            .fetch_all(conn)
            .await?,
    )
}

// ---------------------------------------------------------------- Movimenti

#[derive(Debug, Clone, Default)]
pub struct NewMovement {
    pub operation_id: Uuid,
    pub kind: &'static str,
    pub direction: Option<&'static str>,
    pub product_id: Uuid,
    pub lot_id: Option<Uuid>,
    pub warehouse_id: Uuid,
    pub from_location_id: Option<Uuid>,
    pub to_location_id: Option<Uuid>,
    pub quantity: Decimal,
    pub reason_code: String,
    pub note: Option<String>,
    pub sales_order_id: Option<Uuid>,
    pub sales_order_line_id: Option<Uuid>,
    pub purchase_order_id: Option<Uuid>,
    pub purchase_order_line_id: Option<Uuid>,
    pub external_ref: Option<String>,
    pub operator: String,
    pub occurred_at: Option<DateTime<Utc>>,
    pub reverses_movement_id: Option<Uuid>,
    pub fefo_override_reason: Option<String>,
    pub serial_ids: Vec<Uuid>,
}

pub async fn insert_movement(conn: &mut PgConnection, m: NewMovement) -> ApiResult<Uuid> {
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO movements (operation_id, type, direction, product_id, lot_id, warehouse_id, from_location_id,
             to_location_id, quantity, reason_code, note, sales_order_id, sales_order_line_id, purchase_order_id,
             purchase_order_line_id, external_ref, operator, occurred_at, reverses_movement_id, fefo_override_reason)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, COALESCE($18, now()), $19, $20)
         RETURNING id",
    )
    .bind(m.operation_id)
    .bind(m.kind)
    .bind(m.direction)
    .bind(m.product_id)
    .bind(m.lot_id)
    .bind(m.warehouse_id)
    .bind(m.from_location_id)
    .bind(m.to_location_id)
    .bind(m.quantity)
    .bind(&m.reason_code)
    .bind(&m.note)
    .bind(m.sales_order_id)
    .bind(m.sales_order_line_id)
    .bind(m.purchase_order_id)
    .bind(m.purchase_order_line_id)
    .bind(&m.external_ref)
    .bind(&m.operator)
    .bind(m.occurred_at)
    .bind(m.reverses_movement_id)
    .bind(&m.fefo_override_reason)
    .fetch_one(&mut *conn)
    .await?;
    if !m.serial_ids.is_empty() {
        sqlx::query("INSERT INTO movement_serials (movement_id, serial_id) SELECT $1, unnest($2::uuid[])")
            .bind(id)
            .bind(&m.serial_ids)
            .execute(conn)
            .await?;
    }
    Ok(id)
}

pub async fn movements_by_id(conn: &mut PgConnection, ids: &[Uuid]) -> ApiResult<Vec<Movement>> {
    Ok(
        sqlx::query_as(concat!(movement_select!(), "WHERE m.id = ANY($1) ORDER BY m.number"))
            .bind(ids)
            .fetch_all(conn)
            .await?,
    )
}

/// Risultato standard delle operazioni che muovono stock.
#[derive(Debug, Serialize)]
pub struct OperationResult {
    pub operation_id: Uuid,
    pub movements: Vec<Movement>,
    pub alerts: Vec<Alert>,
}

// ---------------------------------------------------------------- Prelievi

/// Righe di giacenza di un prodotto in un magazzino, bloccate per la transazione.
pub async fn candidates(conn: &mut PgConnection, product_id: Uuid, warehouse_id: Uuid) -> ApiResult<Vec<Candidate>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        location_id: Uuid,
        location_code: String,
        lot_id: Option<Uuid>,
        lot_code: Option<String>,
        expiry_date: Option<NaiveDate>,
        received_at: DateTime<Utc>,
        quantity: Decimal,
        usable: bool,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT s.location_id, loc.code AS location_code, s.lot_id, l.lot_code, l.expiry_date,
                COALESCE(l.received_at, s.received_at) AS received_at, s.quantity,
                NOT (l.id IS NOT NULL AND (l.status = 'BLOCKED' OR l.expiry_date < CURRENT_DATE)) AS usable
         FROM stock s
         JOIN locations loc ON loc.id = s.location_id
         LEFT JOIN lots l ON l.id = s.lot_id
         WHERE s.product_id = $1 AND s.warehouse_id = $2 AND s.quantity > 0
         ORDER BY loc.code
         FOR UPDATE OF s",
    )
    .bind(product_id)
    .bind(warehouse_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Candidate {
            location_id: r.location_id,
            location_code: r.location_code,
            lot_id: r.lot_id,
            lot_code: r.lot_code,
            expiry_date: r.expiry_date,
            received_at: r.received_at,
            quantity: r.quantity,
            usable: r.usable,
        })
        .collect())
}

/// Prelievo indicato dal client (modalità `MANUAL`).
#[derive(Debug, Clone, Deserialize)]
pub struct PickInput {
    pub location_id: Uuid,
    #[serde(default)]
    pub lot_id: Option<Uuid>,
    pub quantity: Decimal,
    #[serde(default)]
    pub serials: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FefoOverride {
    pub reason: String,
}

/// Prelievo pianificato e validato, pronto per lo scarico.
#[derive(Debug, Clone)]
pub struct PlannedPick {
    pub location_id: Uuid,
    pub lot_id: Option<Uuid>,
    pub quantity: Decimal,
    pub serials: Vec<String>,
    pub fefo_override_reason: Option<String>,
}

/// Pianifica i prelievi in automatico (FEFO/FIFO) sulle giacenze utilizzabili.
pub async fn plan_auto(
    conn: &mut PgConnection,
    p: &Product,
    warehouse_id: Uuid,
    quantity: Decimal,
    strategy: Strategy,
) -> ApiResult<Vec<PlannedPick>> {
    let cands = candidates(conn, p.id, warehouse_id).await?;
    let strategy = strategy.resolve(p.requires_expiry, &cands);
    let result = picking::allocate(&cands, quantity, strategy, p.tracks_serials());
    if result.shortfall > Decimal::ZERO {
        let usable: Decimal = cands.iter().filter(|c| c.usable).map(|c| c.quantity).sum();
        return Err(ApiError::conflict(
            "INSUFFICIENT_STOCK",
            format!(
                "Giacenze utilizzabili insufficienti per {}: presenti {}, richiesti {}",
                p.sku,
                usable.normalize(),
                quantity.normalize()
            ),
        )
        .with(json!({
            "product_id": p.id, "warehouse_id": warehouse_id, "requested": quantity,
            "on_hand": usable, "shortfall": result.shortfall,
        })));
    }
    Ok(result
        .picks
        .iter()
        .map(|a| PlannedPick {
            location_id: cands[a.candidate].location_id,
            lot_id: cands[a.candidate].lot_id,
            quantity: a.quantity,
            serials: Vec::new(),
            fefo_override_reason: None,
        })
        .collect())
}

/// Valida i prelievi manuali: ubicazione del magazzino, lotto coerente e utilizzabile, seriali,
/// e regola FEFO (violazione rifiutata salvo `fefo_override`).
pub async fn plan_manual(
    conn: &mut PgConnection,
    p: &Product,
    warehouse_id: Uuid,
    picks: &[PickInput],
    fefo_override: Option<&FefoOverride>,
) -> ApiResult<Vec<PlannedPick>> {
    if picks.is_empty() {
        return Err(ApiError::validation(
            "In modalità MANUAL ogni riga deve indicare i prelievi (picks)",
        ));
    }
    let cands = candidates(conn, p.id, warehouse_id).await?;
    let mut taken = vec![Decimal::ZERO; cands.len()];
    let mut lots = Vec::with_capacity(picks.len());
    for pick in picks {
        crate::model::check_quantity(pick.quantity, "picks.quantity")?;
        check_whole_units(p, pick.quantity)?;
        let loc = location(conn, pick.location_id, false).await?;
        ensure_in_warehouse(&loc, warehouse_id)?;
        let lot = lot_for_product(conn, p, pick.lot_id).await?;
        if let Some(l) = &lot {
            ensure_lot_usable(l)?;
        }
        check_serial_count(p, pick.quantity, &pick.serials)?;
        if let Some(i) = cands
            .iter()
            .position(|c| c.location_id == pick.location_id && c.lot_id == pick.lot_id)
        {
            taken[i] += pick.quantity;
        }
        lots.push(lot);
    }

    let mut planned = Vec::with_capacity(picks.len());
    for (pick, lot) in picks.iter().zip(lots) {
        let mut override_reason = None;
        if let Some(l) = lot.as_ref().filter(|_| p.lot_allowed()) {
            let earlier = picking::fefo_violations(&cands, &taken, l.expiry_date);
            if !earlier.is_empty() {
                match fefo_override {
                    Some(o) if o.reason.trim().len() >= 3 => override_reason = Some(o.reason.trim().to_string()),
                    _ => {
                        let first = &earlier[0];
                        return Err(ApiError::conflict(
                            "FEFO_VIOLATION",
                            format!(
                                "Il lotto {} (scad. {}) ha scadenza posteriore al lotto {} (scad. {}) presente nel magazzino. \
                                 Ripetere con fefo_override per confermare.",
                                l.lot_code,
                                l.expiry_date.map(|d| d.to_string()).unwrap_or_else(|| "nessuna".into()),
                                first.lot_code,
                                first.expiry_date
                            ),
                        )
                        .with(json!({
                            "product_id": p.id, "picked_lot_id": l.id, "picked_lot_code": l.lot_code,
                            "picked_expiry_date": l.expiry_date, "earlier_lots": earlier,
                        })));
                    }
                }
            }
        }
        planned.push(PlannedPick {
            location_id: pick.location_id,
            lot_id: pick.lot_id,
            quantity: pick.quantity,
            serials: pick.serials.clone(),
            fefo_override_reason: override_reason,
        });
    }
    Ok(planned)
}

/// Esegue lo scarico dei prelievi pianificati: riduce le giacenze, spedisce i seriali e registra
/// un movimento `OUTBOUND` per prelievo, partendo dal modello `template`.
pub async fn execute_outbound(
    conn: &mut PgConnection,
    p: &Product,
    plans: &[PlannedPick],
    template: &NewMovement,
) -> ApiResult<Vec<Uuid>> {
    let mut ids = Vec::with_capacity(plans.len());
    for plan in plans {
        let serial_ids = if !p.tracks_serials() {
            Vec::new()
        } else if plan.serials.is_empty() {
            pick_serials(conn, p.id, plan.location_id, plan.lot_id, plan.quantity).await?
        } else {
            serials_in_stock(conn, p, &plan.serials, plan.location_id, plan.lot_id).await?
        };
        remove_stock(conn, plan.location_id, p.id, plan.lot_id, plan.quantity).await?;
        set_serials(conn, &serial_ids, "SHIPPED", None).await?;
        let id = insert_movement(
            conn,
            NewMovement {
                kind: "OUTBOUND",
                product_id: p.id,
                lot_id: plan.lot_id,
                from_location_id: Some(plan.location_id),
                quantity: plan.quantity,
                fefo_override_reason: plan.fefo_override_reason.clone(),
                serial_ids,
                ..template.clone()
            },
        )
        .await?;
        ids.push(id);
    }
    Ok(ids)
}

/// Somma per chiave, preservando l'ordine di prima apparizione.
pub fn sum_by<K: Eq + std::hash::Hash + Copy>(items: impl IntoIterator<Item = (K, Decimal)>) -> Vec<(K, Decimal)> {
    let mut order = Vec::new();
    let mut map: HashMap<K, Decimal> = HashMap::new();
    for (k, v) in items {
        if !map.contains_key(&k) {
            order.push(k);
        }
        *map.entry(k).or_default() += v;
    }
    order.into_iter().map(|k| (k, map[&k])).collect()
}
