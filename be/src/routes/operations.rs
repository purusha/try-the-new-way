//! Operazioni di magazzino: carico, suggerimento di prelievo, scarico, trasferimento, rettifica.

use axum::extract::State;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::picking::{self, Strategy};
use crate::error::{ApiError, ApiResult};
use crate::http::{Body, Json, Operator, created, ok};
use crate::inventory::{self, FefoOverride, NewMovement, OperationResult, PickInput};
use crate::model::{AdjustmentReason, Lot, PickMode, Product, check_quantity, check_scale};
use crate::routes::lots::check_lot_dates;
use crate::routes::purchase_orders;

// ---------------------------------------------------------------- Carico

#[derive(Debug, Deserialize)]
pub struct LotInput {
    pub lot_code: String,
    #[serde(default)]
    pub production_date: Option<NaiveDate>,
    #[serde(default)]
    pub expiry_date: Option<NaiveDate>,
}

#[derive(Debug, Deserialize)]
pub struct ReceiptLine {
    product_id: Uuid,
    quantity: Decimal,
    location_id: Uuid,
    #[serde(default)]
    lot_id: Option<Uuid>,
    #[serde(default)]
    lot: Option<LotInput>,
    #[serde(default)]
    serials: Vec<String>,
    #[serde(default)]
    purchase_order_line_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct ReceiptRequest {
    warehouse_id: Uuid,
    #[serde(default)]
    purchase_order_id: Option<Uuid>,
    #[serde(default)]
    external_ref: Option<String>,
    #[serde(default)]
    reason_code: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    occurred_at: Option<DateTime<Utc>>,
    lines: Vec<ReceiptLine>,
}

/// Trova o crea il lotto di un carico. Un lotto esistente deve avere le stesse date indicate.
pub async fn resolve_lot(
    conn: &mut PgConnection,
    p: &Product,
    lot_id: Option<Uuid>,
    input: Option<&LotInput>,
) -> ApiResult<Option<Lot>> {
    if lot_id.is_some() && input.is_some() {
        return Err(ApiError::validation("Indicare lot_id oppure lot, non entrambi"));
    }
    let Some(input) = input else {
        return inventory::lot_for_product(conn, p, lot_id).await;
    };
    if !p.lot_allowed() {
        return Err(ApiError::unprocessable(
            "LOT_NOT_ALLOWED",
            format!("Il prodotto {} non è tracciato a lotto", p.sku),
        )
        .with(json!({ "product_id": p.id })));
    }
    let code = input.lot_code.trim();
    if code.is_empty() {
        return Err(ApiError::validation("lot.lot_code è obbligatorio"));
    }
    check_lot_dates(input.production_date, input.expiry_date)?;
    let existing: Option<(Uuid, Option<NaiveDate>, Option<NaiveDate>)> = sqlx::query_as(
        "SELECT id, production_date, expiry_date FROM lots WHERE product_id = $1 AND lot_code = $2 FOR UPDATE",
    )
    .bind(p.id)
    .bind(code)
    .fetch_optional(&mut *conn)
    .await?;
    let id = match existing {
        Some((id, production, expiry)) => {
            for (field, registered, received) in [
                ("production_date", production, input.production_date),
                ("expiry_date", expiry, input.expiry_date),
            ] {
                if received.is_some() && received != registered {
                    return Err(ApiError::unprocessable(
                        "LOT_DATA_MISMATCH",
                        format!("Il lotto {code} è già registrato con {field} diversa"),
                    )
                    .with(
                        json!({ "lot_code": code, "field": field, "registered": registered, "received": received }),
                    ));
                }
            }
            id
        }
        None => {
            if p.requires_expiry && input.expiry_date.is_none() {
                return Err(ApiError::unprocessable(
                    "EXPIRY_DATE_REQUIRED",
                    format!("Il prodotto {} richiede la data di scadenza del lotto", p.sku),
                )
                .with(json!({ "product_id": p.id, "lot_code": code })));
            }
            sqlx::query_scalar(
                "INSERT INTO lots (product_id, lot_code, production_date, expiry_date) VALUES ($1, $2, $3, $4) RETURNING id",
            )
            .bind(p.id)
            .bind(code)
            .bind(input.production_date)
            .bind(input.expiry_date)
            .fetch_one(&mut *conn)
            .await?
        }
    };
    Ok(Some(inventory::lot(conn, id).await?))
}

pub async fn receipt(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Body(b): Body<ReceiptRequest>,
) -> ApiResult<Json<OperationResult>> {
    if b.lines.is_empty() {
        return Err(ApiError::validation("Il carico deve avere almeno una riga"));
    }
    let mut tx = pool.begin().await?;
    inventory::active_warehouse(&mut tx, b.warehouse_id).await?;
    if let Some(po) = b.purchase_order_id {
        purchase_orders::lock_for_receipt(&mut tx, po, b.warehouse_id).await?;
    } else if b.lines.iter().any(|l| l.purchase_order_line_id.is_some()) {
        return Err(ApiError::validation(
            "purchase_order_line_id richiede purchase_order_id",
        ));
    }
    let pairs: Vec<_> = b.lines.iter().map(|l| (l.product_id, b.warehouse_id)).collect();
    inventory::lock_pairs(&mut tx, &pairs).await?;

    let operation_id = Uuid::new_v4();
    let reason = b.reason_code.clone().unwrap_or_else(|| "PURCHASE_RECEIPT".into());
    let mut ids = Vec::new();
    for (i, line) in b.lines.iter().enumerate() {
        let id = receipt_line(&mut tx, &b, line, operation_id, &reason, &operator)
            .await
            .map_err(|e| e.at_line(i))?;
        ids.push(id);
    }
    if let Some(po) = b.purchase_order_id {
        purchase_orders::refresh_status(&mut tx, po).await?;
    }
    let result = finish(&mut tx, operation_id, &ids, &pairs).await?;
    tx.commit().await?;
    Ok(created(result))
}

async fn receipt_line(
    conn: &mut PgConnection,
    b: &ReceiptRequest,
    line: &ReceiptLine,
    operation_id: Uuid,
    reason: &str,
    operator: &str,
) -> ApiResult<Uuid> {
    check_quantity(line.quantity, "quantity")?;
    let p = inventory::product(conn, line.product_id).await?;
    inventory::ensure_active(&p)?;
    let lot = resolve_lot(conn, &p, line.lot_id, line.lot.as_ref()).await?;
    if lot.is_none() && p.lot_required() {
        return Err(ApiError::unprocessable(
            "LOT_REQUIRED",
            format!("Il prodotto {} è tracciato a lotto: indicare il lotto", p.sku),
        )
        .with(json!({ "product_id": p.id })));
    }
    inventory::check_serial_count(&p, line.quantity, &line.serials)?;
    let loc = inventory::location(conn, line.location_id, true).await?;
    inventory::ensure_in_warehouse(&loc, b.warehouse_id)?;
    inventory::check_destination(conn, &p, &loc, line.quantity).await?;

    let lot_id = lot.as_ref().map(|l| l.id);
    let po_line = match b.purchase_order_id {
        Some(po) => {
            Some(purchase_orders::receive_line(conn, po, line.purchase_order_line_id, p.id, line.quantity).await?)
        }
        None => None,
    };
    let serial_ids = if p.tracks_serials() {
        inventory::receive_serials(conn, &p, &line.serials, lot_id, loc.id).await?
    } else {
        Vec::new()
    };
    let received_at = b.occurred_at.unwrap_or_else(Utc::now);
    inventory::add_stock(conn, b.warehouse_id, loc.id, p.id, lot_id, line.quantity, received_at).await?;
    inventory::insert_movement(
        conn,
        NewMovement {
            operation_id,
            kind: "INBOUND",
            product_id: p.id,
            lot_id,
            warehouse_id: b.warehouse_id,
            to_location_id: Some(loc.id),
            quantity: line.quantity,
            reason_code: reason.to_string(),
            note: b.note.clone(),
            purchase_order_id: b.purchase_order_id,
            purchase_order_line_id: po_line,
            external_ref: b.external_ref.clone(),
            operator: operator.to_string(),
            occurred_at: b.occurred_at,
            serial_ids,
            ..Default::default()
        },
    )
    .await
}

/// Carica movimenti e alert dell'operazione appena eseguita.
pub async fn finish(
    conn: &mut PgConnection,
    operation_id: Uuid,
    ids: &[Uuid],
    pairs: &[(Uuid, Uuid)],
) -> ApiResult<OperationResult> {
    let movements = inventory::movements_by_id(conn, ids).await?;
    let alerts = inventory::alerts_for(conn, pairs).await?;
    Ok(OperationResult {
        operation_id,
        movements,
        alerts,
    })
}

// ---------------------------------------------------------------- Suggerimento di prelievo

#[derive(Debug, Deserialize)]
pub struct SuggestLine {
    product_id: Uuid,
    quantity: Decimal,
}

#[derive(Debug, Deserialize)]
pub struct SuggestRequest {
    warehouse_id: Uuid,
    #[serde(default)]
    strategy: Strategy,
    #[serde(default)]
    sales_order_id: Option<Uuid>,
    #[serde(default)]
    lines: Vec<SuggestLine>,
}

#[derive(Debug, Serialize)]
pub struct SuggestedPick {
    location_id: Uuid,
    location_code: String,
    lot_id: Option<Uuid>,
    lot_code: Option<String>,
    expiry_date: Option<NaiveDate>,
    received_at: DateTime<Utc>,
    quantity: Decimal,
    serials: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SuggestionLine {
    product_id: Uuid,
    sku: String,
    sales_order_line_id: Option<Uuid>,
    requested: Decimal,
    allocated: Decimal,
    shortfall: Decimal,
    strategy_applied: &'static str,
    picks: Vec<SuggestedPick>,
}

#[derive(Debug, Serialize)]
pub struct Suggestion {
    warehouse_id: Uuid,
    lines: Vec<SuggestionLine>,
}

pub async fn suggest(State(pool): State<PgPool>, Body(b): Body<SuggestRequest>) -> ApiResult<Json<Suggestion>> {
    // Solo calcolo: la transazione viene annullata alla fine (i lock servono per una vista coerente).
    let mut tx = pool.begin().await?;
    inventory::warehouse(&mut tx, b.warehouse_id).await?;
    let requests: Vec<(Uuid, Decimal, Option<Uuid>)> = match b.sales_order_id {
        Some(order_id) => {
            if !b.lines.is_empty() {
                return Err(ApiError::validation(
                    "Indicare sales_order_id oppure lines, non entrambi",
                ));
            }
            let order_wh: Uuid = sqlx::query_scalar("SELECT warehouse_id FROM sales_orders WHERE id = $1")
                .bind(order_id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| ApiError::not_found("sales_order", order_id))?;
            if order_wh != b.warehouse_id {
                return Err(ApiError::validation("L'ordine è di un altro magazzino"));
            }
            sqlx::query_as(
                "SELECT product_id, quantity_reserved, id FROM sales_order_lines
                 WHERE order_id = $1 AND quantity_reserved > 0 ORDER BY line_no",
            )
            .bind(order_id)
            .fetch_all(&mut *tx)
            .await?
        }
        None => {
            if b.lines.is_empty() {
                return Err(ApiError::validation("Indicare lines oppure sales_order_id"));
            }
            b.lines.iter().map(|l| (l.product_id, l.quantity, None)).collect()
        }
    };

    // Le righe dello stesso prodotto si allocano in sequenza sulle giacenze residue.
    let mut remaining: std::collections::HashMap<Uuid, Vec<picking::Candidate>> = Default::default();
    let mut lines = Vec::with_capacity(requests.len());
    for (i, (product_id, quantity, order_line)) in requests.into_iter().enumerate() {
        check_quantity(quantity, "quantity").map_err(|e| e.at_line(i))?;
        let p = inventory::product(&mut tx, product_id)
            .await
            .map_err(|e| e.at_line(i))?;
        if let std::collections::hash_map::Entry::Vacant(e) = remaining.entry(product_id) {
            e.insert(inventory::candidates(&mut tx, product_id, b.warehouse_id).await?);
        }
        let cands = remaining.get_mut(&product_id).expect("inserito sopra");
        let strategy = b.strategy.resolve(p.requires_expiry, cands);
        let result = picking::allocate(cands, quantity, strategy, p.tracks_serials());
        let mut picks = Vec::with_capacity(result.picks.len());
        for a in &result.picks {
            let c = &cands[a.candidate];
            let serials = if p.tracks_serials() {
                sqlx::query_scalar(
                    "SELECT serial_number FROM serials
                     WHERE product_id = $1 AND location_id = $2 AND lot_id IS NOT DISTINCT FROM $3 AND status = 'IN_STOCK'
                     ORDER BY serial_number",
                )
                .bind(p.id)
                .bind(c.location_id)
                .bind(c.lot_id)
                .fetch_all(&mut *tx)
                .await?
            } else {
                Vec::new()
            };
            picks.push(SuggestedPick {
                location_id: c.location_id,
                location_code: c.location_code.clone(),
                lot_id: c.lot_id,
                lot_code: c.lot_code.clone(),
                expiry_date: c.expiry_date,
                received_at: c.received_at,
                quantity: a.quantity,
                serials: serials
                    .into_iter()
                    .take(a.quantity.trunc().try_into().unwrap_or(0))
                    .collect(),
            });
        }
        for a in &result.picks {
            cands[a.candidate].quantity -= a.quantity;
        }
        lines.push(SuggestionLine {
            product_id,
            sku: p.sku,
            sales_order_line_id: order_line,
            requested: quantity,
            allocated: result.allocated,
            shortfall: result.shortfall,
            strategy_applied: strategy.as_str(),
            picks,
        });
    }
    tx.rollback().await?;
    Ok(ok(Suggestion {
        warehouse_id: b.warehouse_id,
        lines,
    }))
}

// ---------------------------------------------------------------- Scarico

#[derive(Debug, Deserialize)]
pub struct ShipmentLine {
    product_id: Uuid,
    #[serde(default)]
    quantity: Option<Decimal>,
    #[serde(default)]
    picks: Vec<PickInput>,
}

#[derive(Debug, Deserialize)]
pub struct ShipmentRequest {
    warehouse_id: Uuid,
    #[serde(default)]
    mode: PickMode,
    #[serde(default)]
    strategy: Strategy,
    #[serde(default)]
    external_ref: Option<String>,
    #[serde(default)]
    reason_code: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    occurred_at: Option<DateTime<Utc>>,
    #[serde(default)]
    fefo_override: Option<FefoOverride>,
    lines: Vec<ShipmentLine>,
}

/// Quantità di una riga di uscita: in AUTO è obbligatoria, in MANUAL è la somma dei prelievi.
pub fn line_quantity(mode: PickMode, quantity: Option<Decimal>, picks: &[PickInput]) -> ApiResult<Decimal> {
    match mode {
        PickMode::Auto => {
            if !picks.is_empty() {
                return Err(ApiError::validation(
                    "I prelievi (picks) si indicano solo in modalità MANUAL",
                ));
            }
            let q = quantity.ok_or_else(|| ApiError::validation("quantity è obbligatoria in modalità AUTO"))?;
            check_quantity(q, "quantity")?;
            Ok(q)
        }
        PickMode::Manual => {
            if picks.is_empty() {
                return Err(ApiError::validation(
                    "In modalità MANUAL ogni riga deve indicare i prelievi (picks)",
                ));
            }
            let sum: Decimal = picks.iter().map(|p| p.quantity).sum();
            if quantity.is_some_and(|q| q != sum) {
                return Err(ApiError::validation("quantity deve essere la somma dei prelievi"));
            }
            Ok(sum)
        }
    }
}

pub async fn shipment(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Body(b): Body<ShipmentRequest>,
) -> ApiResult<Json<OperationResult>> {
    if b.lines.is_empty() {
        return Err(ApiError::validation("Lo scarico deve avere almeno una riga"));
    }
    if let Some(o) = &b.fefo_override
        && o.reason.trim().len() < 3
    {
        return Err(ApiError::validation(
            "fefo_override.reason deve avere almeno 3 caratteri",
        ));
    }
    let mut tx = pool.begin().await?;
    let warehouse = inventory::active_warehouse(&mut tx, b.warehouse_id).await?;
    let pairs: Vec<_> = b.lines.iter().map(|l| (l.product_id, b.warehouse_id)).collect();
    inventory::lock_pairs(&mut tx, &pairs).await?;

    // Regola 1: lo scarico senza ordine non può intaccare la merce impegnata.
    let mut quantities = Vec::with_capacity(b.lines.len());
    for (i, line) in b.lines.iter().enumerate() {
        quantities.push(line_quantity(b.mode, line.quantity, &line.picks).map_err(|e| e.at_line(i))?);
    }
    for (product_id, requested) in
        inventory::sum_by(b.lines.iter().map(|l| l.product_id).zip(quantities.iter().copied()))
    {
        let p = inventory::product(&mut tx, product_id).await?;
        let f = inventory::figures(&mut tx, product_id, b.warehouse_id).await?;
        if requested > f.available {
            return Err(inventory::insufficient_availability(
                &p,
                &warehouse,
                requested,
                f.available,
            ));
        }
    }

    let operation_id = Uuid::new_v4();
    let template = NewMovement {
        operation_id,
        warehouse_id: b.warehouse_id,
        reason_code: b.reason_code.clone().unwrap_or_else(|| "SHIPMENT".into()),
        note: b.note.clone(),
        external_ref: b.external_ref.clone(),
        operator,
        occurred_at: b.occurred_at,
        ..Default::default()
    };
    let mut ids = Vec::new();
    for (i, (line, quantity)) in b.lines.iter().zip(quantities).enumerate() {
        let p = inventory::product(&mut tx, line.product_id)
            .await
            .map_err(|e| e.at_line(i))?;
        inventory::check_whole_units(&p, quantity).map_err(|e| e.at_line(i))?;
        let plans = match b.mode {
            PickMode::Auto => inventory::plan_auto(&mut tx, &p, b.warehouse_id, quantity, b.strategy).await,
            PickMode::Manual => {
                inventory::plan_manual(&mut tx, &p, b.warehouse_id, &line.picks, b.fefo_override.as_ref()).await
            }
        }
        .map_err(|e| e.at_line(i))?;
        ids.extend(
            inventory::execute_outbound(&mut tx, &p, &plans, &template)
                .await
                .map_err(|e| e.at_line(i))?,
        );
    }
    let result = finish(&mut tx, operation_id, &ids, &pairs).await?;
    tx.commit().await?;
    Ok(created(result))
}

// ---------------------------------------------------------------- Trasferimento

#[derive(Debug, Deserialize)]
pub struct TransferLine {
    product_id: Uuid,
    #[serde(default)]
    lot_id: Option<Uuid>,
    from_location_id: Uuid,
    to_location_id: Uuid,
    quantity: Decimal,
    #[serde(default)]
    serials: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TransferRequest {
    #[serde(default)]
    reason_code: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    occurred_at: Option<DateTime<Utc>>,
    lines: Vec<TransferLine>,
}

pub async fn transfer(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Body(b): Body<TransferRequest>,
) -> ApiResult<Json<OperationResult>> {
    if b.lines.is_empty() {
        return Err(ApiError::validation("Il trasferimento deve avere almeno una riga"));
    }
    let mut tx = pool.begin().await?;
    let operation_id = Uuid::new_v4();
    let mut ids = Vec::new();
    let mut pairs = Vec::new();
    for (i, line) in b.lines.iter().enumerate() {
        let (id, pair) = transfer_line(&mut tx, &b, line, operation_id, &operator)
            .await
            .map_err(|e| e.at_line(i))?;
        ids.push(id);
        pairs.push(pair);
    }
    let result = finish(&mut tx, operation_id, &ids, &pairs).await?;
    tx.commit().await?;
    Ok(created(result))
}

async fn transfer_line(
    conn: &mut PgConnection,
    b: &TransferRequest,
    line: &TransferLine,
    operation_id: Uuid,
    operator: &str,
) -> ApiResult<(Uuid, (Uuid, Uuid))> {
    check_quantity(line.quantity, "quantity")?;
    if line.from_location_id == line.to_location_id {
        return Err(
            ApiError::unprocessable("SAME_LOCATION_TRANSFER", "Origine e destinazione coincidono")
                .with(json!({ "location_id": line.from_location_id })),
        );
    }
    let p = inventory::product(conn, line.product_id).await?;
    let lot = inventory::lot_for_product(conn, &p, line.lot_id).await?;
    let from = inventory::location(conn, line.from_location_id, false).await?;
    let to = inventory::location(conn, line.to_location_id, true).await?;
    if from.warehouse_id != to.warehouse_id {
        return Err(ApiError::unprocessable(
            "CROSS_WAREHOUSE_TRANSFER_NOT_SUPPORTED",
            "I trasferimenti tra magazzini diversi non sono supportati",
        )
        .with(json!({ "from_warehouse_id": from.warehouse_id, "to_warehouse_id": to.warehouse_id })));
    }
    inventory::lock_pairs(conn, &[(p.id, from.warehouse_id)]).await?;
    inventory::check_serial_count(&p, line.quantity, &line.serials)?;
    let lot_id = lot.map(|l| l.id);
    let serial_ids = inventory::serials_in_stock(conn, &p, &line.serials, from.id, lot_id).await?;
    let received_at = inventory::remove_stock(conn, from.id, p.id, lot_id, line.quantity).await?;
    inventory::check_destination(conn, &p, &to, line.quantity).await?;
    // La merce trasferita conserva la propria data di ricevimento (FIFO).
    inventory::add_stock(conn, to.warehouse_id, to.id, p.id, lot_id, line.quantity, received_at).await?;
    inventory::set_serials(conn, &serial_ids, "IN_STOCK", Some(to.id)).await?;
    let id = inventory::insert_movement(
        conn,
        NewMovement {
            operation_id,
            kind: "TRANSFER",
            product_id: p.id,
            lot_id,
            warehouse_id: from.warehouse_id,
            from_location_id: Some(from.id),
            to_location_id: Some(to.id),
            quantity: line.quantity,
            reason_code: b.reason_code.clone().unwrap_or_else(|| "INTERNAL_TRANSFER".into()),
            note: b.note.clone(),
            operator: operator.to_string(),
            occurred_at: b.occurred_at,
            serial_ids,
            ..Default::default()
        },
    )
    .await?;
    Ok((id, (p.id, from.warehouse_id)))
}

// ---------------------------------------------------------------- Rettifica

#[derive(Debug, Deserialize)]
pub struct AdjustmentRequest {
    location_id: Uuid,
    product_id: Uuid,
    #[serde(default)]
    lot_id: Option<Uuid>,
    #[serde(default)]
    lot: Option<LotInput>,
    counted_quantity: Decimal,
    reason_code: Option<serde_json::Value>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    serials: Vec<String>,
    #[serde(default)]
    occurred_at: Option<DateTime<Utc>>,
}

fn adjustment_reason(raw: &Option<serde_json::Value>, note: &Option<String>) -> ApiResult<AdjustmentReason> {
    let allowed = ["COUNT", "DAMAGE", "LOSS", "FOUND", "EXPIRED_DISPOSAL", "OTHER"];
    let err = || {
        ApiError::unprocessable(
            "ADJUSTMENT_REASON_REQUIRED",
            "La rettifica richiede una causale valida (e una nota per OTHER)",
        )
        .with(json!({ "allowed": allowed }))
    };
    let reason: AdjustmentReason = raw
        .clone()
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(err)?;
    if reason == AdjustmentReason::Other && note.as_deref().is_none_or(|n| n.trim().is_empty()) {
        return Err(err());
    }
    Ok(reason)
}

pub async fn adjustment(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Body(b): Body<AdjustmentRequest>,
) -> ApiResult<Json<OperationResult>> {
    let reason = adjustment_reason(&b.reason_code, &b.note)?;
    if b.counted_quantity < Decimal::ZERO {
        return Err(ApiError::validation("counted_quantity non può essere negativa"));
    }
    check_scale(b.counted_quantity, "counted_quantity")?;
    let mut tx = pool.begin().await?;
    let p = inventory::product(&mut tx, b.product_id).await?;
    inventory::check_whole_units(&p, b.counted_quantity)?;
    let loc = inventory::location(&mut tx, b.location_id, true).await?;
    inventory::lock_pairs(&mut tx, &[(p.id, loc.warehouse_id)]).await?;
    let lot = resolve_lot(&mut tx, &p, b.lot_id, b.lot.as_ref()).await?;
    if lot.is_none() && p.lot_required() {
        return Err(ApiError::unprocessable(
            "LOT_REQUIRED",
            format!("Il prodotto {} è tracciato a lotto: indicare il lotto", p.sku),
        )
        .with(json!({ "product_id": p.id })));
    }
    let lot_id = lot.map(|l| l.id);
    let system = inventory::on_hand(&mut tx, loc.id, p.id, lot_id).await?;
    let delta = b.counted_quantity - system;
    let operation_id = Uuid::new_v4();
    let pair = (p.id, loc.warehouse_id);
    if delta == Decimal::ZERO {
        if !b.serials.is_empty() {
            return Err(ApiError::unprocessable(
                "SERIAL_COUNT_MISMATCH",
                "Conteggio uguale al sistema: non vanno indicati seriali",
            ));
        }
        let result = finish(&mut tx, operation_id, &[], &[pair]).await?;
        tx.commit().await?;
        return Ok(created(result));
    }
    let quantity = delta.abs();
    inventory::check_serial_count(&p, quantity, &b.serials)?;
    let (direction, serial_ids) = if delta > Decimal::ZERO {
        // Merce trovata: entra in giacenza così com'è, senza controllo di capienza.
        let ids = inventory::receive_serials(&mut tx, &p, &b.serials, lot_id, loc.id).await?;
        inventory::add_stock(&mut tx, loc.warehouse_id, loc.id, p.id, lot_id, quantity, Utc::now()).await?;
        ("IN", ids)
    } else {
        let ids = inventory::serials_in_stock(&mut tx, &p, &b.serials, loc.id, lot_id).await?;
        inventory::remove_stock(&mut tx, loc.id, p.id, lot_id, quantity).await?;
        inventory::set_serials(&mut tx, &ids, "SCRAPPED", None).await?;
        ("OUT", ids)
    };
    let id = inventory::insert_movement(
        &mut tx,
        NewMovement {
            operation_id,
            kind: "ADJUSTMENT",
            direction: Some(direction),
            product_id: p.id,
            lot_id,
            warehouse_id: loc.warehouse_id,
            from_location_id: (direction == "OUT").then_some(loc.id),
            to_location_id: (direction == "IN").then_some(loc.id),
            quantity,
            reason_code: reason.as_str().to_string(),
            note: b.note.clone(),
            operator,
            occurred_at: b.occurred_at,
            serial_ids,
            ..Default::default()
        },
    )
    .await?;
    let result = finish(&mut tx, operation_id, &[id], &[pair]).await?;
    tx.commit().await?;
    Ok(created(result))
}
