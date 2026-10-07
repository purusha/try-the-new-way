//! Ordini fornitore: riferimento per i carichi.

use axum::extract::State;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::http::{Body, Json, Operator, Path, Query, created, ok};
use crate::inventory;
use crate::model::{PurchaseOrderStatus, check_quantity, non_empty};
use crate::pagination::{Page, PageParams};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PurchaseOrderLine {
    id: Uuid,
    line_no: i32,
    product_id: Uuid,
    sku: String,
    product_name: String,
    quantity_ordered: Decimal,
    quantity_received: Decimal,
}

#[derive(Debug, sqlx::FromRow)]
struct Header {
    id: Uuid,
    number: String,
    external_ref: Option<String>,
    supplier_name: String,
    warehouse_id: Uuid,
    status: String,
    expected_date: Option<NaiveDate>,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct PurchaseOrder {
    id: Uuid,
    number: String,
    external_ref: Option<String>,
    supplier_name: String,
    warehouse_id: Uuid,
    status: String,
    expected_date: Option<NaiveDate>,
    notes: Option<String>,
    lines: Vec<PurchaseOrderLine>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn load_lines(conn: &mut PgConnection, ids: &[Uuid]) -> ApiResult<Vec<(Uuid, PurchaseOrderLine)>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        order_id: Uuid,
        #[sqlx(flatten)]
        line: PurchaseOrderLine,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT pol.order_id, pol.id, pol.line_no, pol.product_id, p.sku, p.name AS product_name,
                pol.quantity_ordered, pol.quantity_received
         FROM purchase_order_lines pol JOIN products p ON p.id = pol.product_id
         WHERE pol.order_id = ANY($1) ORDER BY pol.line_no",
    )
    .bind(ids)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(|r| (r.order_id, r.line)).collect())
}

async fn assemble(conn: &mut PgConnection, headers: Vec<Header>) -> ApiResult<Vec<PurchaseOrder>> {
    let ids: Vec<Uuid> = headers.iter().map(|h| h.id).collect();
    let mut lines = load_lines(conn, &ids).await?;
    Ok(headers
        .into_iter()
        .map(|h| {
            let (mine, rest): (Vec<_>, Vec<_>) = lines.drain(..).partition(|(o, _)| *o == h.id);
            lines = rest;
            PurchaseOrder {
                id: h.id,
                number: h.number,
                external_ref: h.external_ref,
                supplier_name: h.supplier_name,
                warehouse_id: h.warehouse_id,
                status: h.status,
                expected_date: h.expected_date,
                notes: h.notes,
                lines: mine.into_iter().map(|(_, l)| l).collect(),
                created_at: h.created_at,
                updated_at: h.updated_at,
            }
        })
        .collect())
}

pub async fn load(conn: &mut PgConnection, id: Uuid) -> ApiResult<PurchaseOrder> {
    let header: Header = sqlx::query_as("SELECT * FROM purchase_orders WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ApiError::not_found("purchase_order", id))?;
    Ok(assemble(conn, vec![header]).await?.remove(0))
}

#[derive(Deserialize)]
pub struct PurchaseOrderFilter {
    page: Option<i64>,
    page_size: Option<i64>,
    status: Option<PurchaseOrderStatus>,
    warehouse_id: Option<Uuid>,
    q: Option<String>,
}

macro_rules! po_filter {
    () => {
        "WHERE ($1::text IS NULL OR status = $1)
           AND ($2::uuid IS NULL OR warehouse_id = $2)
           AND ($3::text IS NULL OR number ILIKE '%' || $3 || '%' OR external_ref ILIKE '%' || $3 || '%'
                OR supplier_name ILIKE '%' || $3 || '%') "
    };
}

pub async fn list(
    State(pool): State<PgPool>,
    Query(f): Query<PurchaseOrderFilter>,
) -> ApiResult<Json<Page<PurchaseOrder>>> {
    let (limit, offset, page, page_size) = PageParams {
        page: f.page,
        page_size: f.page_size,
    }
    .sql()?;
    let status = f.status.map(PurchaseOrderStatus::as_str);
    let mut conn = pool.acquire().await?;
    let headers = sqlx::query_as(concat!(
        "SELECT * FROM purchase_orders ",
        po_filter!(),
        "ORDER BY created_at DESC, number DESC LIMIT $4 OFFSET $5"
    ))
    .bind(status)
    .bind(f.warehouse_id)
    .bind(&f.q)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar(concat!("SELECT count(*) FROM purchase_orders ", po_filter!()))
        .bind(status)
        .bind(f.warehouse_id)
        .bind(&f.q)
        .fetch_one(&mut *conn)
        .await?;
    let items = assemble(&mut conn, headers).await?;
    Ok(ok(Page {
        items,
        page,
        page_size,
        total,
    }))
}

pub async fn get(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<PurchaseOrder>> {
    Ok(ok(load(&mut *pool.acquire().await?, id).await?))
}

#[derive(Deserialize)]
pub struct LineInput {
    pub product_id: Uuid,
    pub quantity: Decimal,
}

#[derive(Deserialize)]
pub struct PurchaseOrderCreate {
    #[serde(default)]
    number: Option<String>,
    #[serde(default)]
    external_ref: Option<String>,
    supplier_name: String,
    warehouse_id: Uuid,
    #[serde(default)]
    expected_date: Option<NaiveDate>,
    #[serde(default)]
    notes: Option<String>,
    lines: Vec<LineInput>,
}

pub async fn create(
    State(pool): State<PgPool>,
    _op: Operator,
    Body(b): Body<PurchaseOrderCreate>,
) -> ApiResult<Json<PurchaseOrder>> {
    non_empty(&b.supplier_name, "supplier_name")?;
    if b.lines.is_empty() {
        return Err(ApiError::validation("L'ordine deve avere almeno una riga"));
    }
    let mut tx = pool.begin().await?;
    inventory::active_warehouse(&mut tx, b.warehouse_id).await?;
    for (i, l) in b.lines.iter().enumerate() {
        check_quantity(l.quantity, "lines.quantity").map_err(|e| e.at_line(i))?;
        let p = inventory::product(&mut tx, l.product_id)
            .await
            .map_err(|e| e.at_line(i))?;
        inventory::ensure_active(&p).map_err(|e| e.at_line(i))?;
        inventory::check_whole_units(&p, l.quantity).map_err(|e| e.at_line(i))?;
    }
    let number = match b.number.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) {
        Some(n) => n,
        None => {
            sqlx::query_scalar("SELECT 'PO-' || lpad(nextval('purchase_order_number_seq')::text, 6, '0')")
                .fetch_one(&mut *tx)
                .await?
        }
    };
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO purchase_orders (number, external_ref, supplier_name, warehouse_id, expected_date, notes)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(number)
    .bind(b.external_ref)
    .bind(b.supplier_name.trim())
    .bind(b.warehouse_id)
    .bind(b.expected_date)
    .bind(b.notes)
    .fetch_one(&mut *tx)
    .await?;
    for (i, l) in b.lines.iter().enumerate() {
        sqlx::query("INSERT INTO purchase_order_lines (order_id, line_no, product_id, quantity_ordered) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(i as i32 + 1)
            .bind(l.product_id)
            .bind(l.quantity)
            .execute(&mut *tx)
            .await?;
    }
    let order = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(created(order))
}

pub async fn cancel(State(pool): State<PgPool>, _op: Operator, Path(id): Path<Uuid>) -> ApiResult<Json<PurchaseOrder>> {
    let mut tx = pool.begin().await?;
    let status = lock(&mut tx, id).await?;
    if status != "OPEN" && status != "PARTIALLY_RECEIVED" {
        return Err(ApiError::conflict(
            "ORDER_STATE_INVALID",
            format!("Un ordine fornitore {status} non può essere annullato"),
        )
        .with(json!({ "order_id": id, "status": status, "action": "cancel" })));
    }
    sqlx::query("UPDATE purchase_orders SET status = 'CANCELLED', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let order = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(ok(order))
}

async fn lock(conn: &mut PgConnection, id: Uuid) -> ApiResult<String> {
    sqlx::query_scalar("SELECT status FROM purchase_orders WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| ApiError::not_found("purchase_order", id))
}

/// Verifica che l'ordine fornitore possa ricevere merce nel magazzino del carico.
pub async fn lock_for_receipt(conn: &mut PgConnection, id: Uuid, warehouse_id: Uuid) -> ApiResult<()> {
    let status = lock(conn, id).await?;
    if status != "OPEN" && status != "PARTIALLY_RECEIVED" {
        return Err(ApiError::conflict(
            "ORDER_STATE_INVALID",
            format!("L'ordine fornitore è {status} e non accetta carichi"),
        )
        .with(json!({ "order_id": id, "status": status, "action": "receive" })));
    }
    let po_wh: Uuid = sqlx::query_scalar("SELECT warehouse_id FROM purchase_orders WHERE id = $1")
        .bind(id)
        .fetch_one(&mut *conn)
        .await?;
    if po_wh != warehouse_id {
        return Err(
            ApiError::unprocessable("PURCHASE_ORDER_MISMATCH", "L'ordine fornitore è di un altro magazzino")
                .with(json!({ "purchase_order_id": id })),
        );
    }
    Ok(())
}

/// Registra la quantità ricevuta su una riga (indicata o trovata per prodotto) e ne restituisce l'id.
pub async fn receive_line(
    conn: &mut PgConnection,
    order_id: Uuid,
    line_id: Option<Uuid>,
    product_id: Uuid,
    quantity: Decimal,
) -> ApiResult<Uuid> {
    let line: Option<(Uuid, Uuid, Decimal, Decimal)> = match line_id {
        Some(lid) => {
            sqlx::query_as(
                "SELECT id, product_id, quantity_ordered, quantity_received FROM purchase_order_lines
                 WHERE id = $1 AND order_id = $2 FOR UPDATE",
            )
            .bind(lid)
            .bind(order_id)
            .fetch_optional(&mut *conn)
            .await?
        }
        None => {
            sqlx::query_as(
                "SELECT id, product_id, quantity_ordered, quantity_received FROM purchase_order_lines
                 WHERE order_id = $1 AND product_id = $2 AND quantity_received < quantity_ordered
                 ORDER BY line_no LIMIT 1 FOR UPDATE",
            )
            .bind(order_id)
            .bind(product_id)
            .fetch_optional(&mut *conn)
            .await?
        }
    };
    let Some((lid, _, ordered, received)) = line.filter(|l| l.1 == product_id) else {
        return Err(ApiError::unprocessable(
            "PURCHASE_ORDER_MISMATCH",
            "Nessuna riga aperta dell'ordine fornitore corrisponde al prodotto caricato",
        )
        .with(json!({ "purchase_order_id": order_id, "purchase_order_line_id": line_id })));
    };
    if received + quantity > ordered {
        return Err(ApiError::conflict(
            "RECEIPT_EXCEEDS_ORDERED",
            format!(
                "Il carico supera il residuo della riga ({} ancora da ricevere)",
                (ordered - received).normalize()
            ),
        )
        .with(json!({ "purchase_order_line_id": lid, "requested": quantity, "open": ordered - received })));
    }
    sqlx::query("UPDATE purchase_order_lines SET quantity_received = quantity_received + $2 WHERE id = $1")
        .bind(lid)
        .bind(quantity)
        .execute(&mut *conn)
        .await?;
    Ok(lid)
}

/// Riduce la quantità ricevuta (storno di un carico) e ricalcola lo stato.
pub async fn unreceive_line(
    conn: &mut PgConnection,
    order_id: Uuid,
    line_id: Uuid,
    quantity: Decimal,
) -> ApiResult<()> {
    lock(conn, order_id).await?;
    sqlx::query("UPDATE purchase_order_lines SET quantity_received = quantity_received - $2 WHERE id = $1")
        .bind(line_id)
        .bind(quantity)
        .execute(&mut *conn)
        .await?;
    refresh_status(conn, order_id).await
}

/// Ricalcola lo stato dalle righe (un ordine annullato resta annullato).
pub async fn refresh_status(conn: &mut PgConnection, order_id: Uuid) -> ApiResult<()> {
    sqlx::query(
        "UPDATE purchase_orders po SET status = CASE
             WHEN x.all_received THEN 'RECEIVED'
             WHEN x.any_received THEN 'PARTIALLY_RECEIVED'
             ELSE 'OPEN' END,
             updated_at = now()
         FROM (SELECT bool_and(quantity_received = quantity_ordered) AS all_received,
                      bool_or(quantity_received > 0) AS any_received
               FROM purchase_order_lines WHERE order_id = $1) x
         WHERE po.id = $1 AND po.status <> 'CANCELLED'",
    )
    .bind(order_id)
    .execute(conn)
    .await?;
    Ok(())
}
