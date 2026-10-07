//! Lotti, seriali e tracciabilità.

use axum::extract::State;
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::http::{Body, Json, Operator, Path, Query, created, ok};
use crate::inventory;
use crate::model::{Lot, LotStatus, Movement, Serial, SerialStatus, StockItem, non_empty, nullable};
use crate::pagination::{Page, PageParams};
use crate::{lot_select, movement_select, serial_select, stock_select};

#[derive(Deserialize)]
pub struct ProductLotFilter {
    page: Option<i64>,
    page_size: Option<i64>,
    with_stock: Option<bool>,
}

pub async fn list_product_lots(
    State(pool): State<PgPool>,
    Path(product_id): Path<Uuid>,
    Query(f): Query<ProductLotFilter>,
) -> ApiResult<Json<Page<Lot>>> {
    inventory::product(&mut *pool.acquire().await?, product_id).await?;
    search(
        &pool,
        LotFilter {
            page: f.page,
            page_size: f.page_size,
            product_id: Some(product_id),
            with_stock: f.with_stock,
            ..Default::default()
        },
    )
    .await
}

#[derive(Deserialize, Default)]
pub struct LotFilter {
    page: Option<i64>,
    page_size: Option<i64>,
    lot_code: Option<String>,
    product_id: Option<Uuid>,
    warehouse_id: Option<Uuid>,
    expiring_before: Option<NaiveDate>,
    expired: Option<bool>,
    status: Option<LotStatus>,
    with_stock: Option<bool>,
}

macro_rules! lot_filter {
    () => {
        "WHERE ($1::text IS NULL OR l.lot_code ILIKE '%' || $1 || '%')
           AND ($2::uuid IS NULL OR l.product_id = $2)
           AND ($3::uuid IS NULL OR EXISTS (SELECT 1 FROM stock s WHERE s.lot_id = l.id AND s.warehouse_id = $3 AND s.quantity > 0))
           AND ($4::date IS NULL OR l.expiry_date <= $4)
           AND ($5::bool IS NULL OR $5 = (l.expiry_date IS NOT NULL AND l.expiry_date < CURRENT_DATE))
           AND ($6::text IS NULL OR l.status = $6)
           AND ($7::bool IS NULL OR $7 = EXISTS (SELECT 1 FROM stock s WHERE s.lot_id = l.id AND s.quantity > 0)) "
    };
}

pub async fn search_lots(State(pool): State<PgPool>, Query(f): Query<LotFilter>) -> ApiResult<Json<Page<Lot>>> {
    search(&pool, f).await
}

async fn search(pool: &PgPool, f: LotFilter) -> ApiResult<Json<Page<Lot>>> {
    let (limit, offset, page, page_size) = PageParams {
        page: f.page,
        page_size: f.page_size,
    }
    .sql()?;
    let status = f.status.map(LotStatus::as_str);
    let items = sqlx::query_as(concat!(
        lot_select!(),
        lot_filter!(),
        "ORDER BY l.expiry_date NULLS LAST, l.received_at, l.lot_code LIMIT $8 OFFSET $9"
    ))
    .bind(&f.lot_code)
    .bind(f.product_id)
    .bind(f.warehouse_id)
    .bind(f.expiring_before)
    .bind(f.expired)
    .bind(status)
    .bind(f.with_stock)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    let total = sqlx::query_scalar(concat!("SELECT count(*) FROM lots l ", lot_filter!()))
        .bind(&f.lot_code)
        .bind(f.product_id)
        .bind(f.warehouse_id)
        .bind(f.expiring_before)
        .bind(f.expired)
        .bind(status)
        .bind(f.with_stock)
        .fetch_one(pool)
        .await?;
    Ok(ok(Page {
        items,
        page,
        page_size,
        total,
    }))
}

#[derive(Deserialize)]
pub struct LotCreate {
    lot_code: String,
    #[serde(default)]
    production_date: Option<NaiveDate>,
    #[serde(default)]
    expiry_date: Option<NaiveDate>,
}

pub fn check_lot_dates(production: Option<NaiveDate>, expiry: Option<NaiveDate>) -> ApiResult<()> {
    if let (Some(p), Some(e)) = (production, expiry)
        && e < p
    {
        return Err(ApiError::validation("expiry_date non può precedere production_date"));
    }
    Ok(())
}

pub async fn create_lot(
    State(pool): State<PgPool>,
    _op: Operator,
    Path(product_id): Path<Uuid>,
    Body(b): Body<LotCreate>,
) -> ApiResult<Json<Lot>> {
    non_empty(&b.lot_code, "lot_code")?;
    check_lot_dates(b.production_date, b.expiry_date)?;
    let mut tx = pool.begin().await?;
    let p = inventory::product(&mut tx, product_id).await?;
    if !p.lot_allowed() {
        return Err(ApiError::unprocessable(
            "LOT_NOT_ALLOWED",
            format!("Il prodotto {} non è tracciato a lotto", p.sku),
        ));
    }
    if p.requires_expiry && b.expiry_date.is_none() {
        return Err(ApiError::unprocessable(
            "EXPIRY_DATE_REQUIRED",
            format!("Il prodotto {} richiede la data di scadenza", p.sku),
        ));
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO lots (product_id, lot_code, production_date, expiry_date) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(product_id)
    .bind(b.lot_code.trim())
    .bind(b.production_date)
    .bind(b.expiry_date)
    .fetch_one(&mut *tx)
    .await?;
    let lot = inventory::lot(&mut tx, id).await?;
    tx.commit().await?;
    Ok(created(lot))
}

pub async fn get_lot(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<Lot>> {
    Ok(ok(inventory::lot(&mut *pool.acquire().await?, id).await?))
}

#[derive(Deserialize)]
pub struct LotUpdate {
    status: Option<LotStatus>,
    #[serde(default, deserialize_with = "nullable")]
    production_date: Option<Option<NaiveDate>>,
    #[serde(default, deserialize_with = "nullable")]
    expiry_date: Option<Option<NaiveDate>>,
}

pub async fn update_lot(
    State(pool): State<PgPool>,
    _op: Operator,
    Path(id): Path<Uuid>,
    Body(b): Body<LotUpdate>,
) -> ApiResult<Json<Lot>> {
    let mut tx = pool.begin().await?;
    let current = inventory::lot(&mut tx, id).await?;
    let p = inventory::product(&mut tx, current.product_id).await?;
    let production = b.production_date.unwrap_or(current.production_date);
    let expiry = b.expiry_date.unwrap_or(current.expiry_date);
    check_lot_dates(production, expiry)?;
    if p.requires_expiry && expiry.is_none() {
        return Err(ApiError::unprocessable(
            "EXPIRY_DATE_REQUIRED",
            format!("Il prodotto {} richiede la data di scadenza", p.sku),
        ));
    }
    sqlx::query("UPDATE lots SET status = COALESCE($2, status), production_date = $3, expiry_date = $4 WHERE id = $1")
        .bind(id)
        .bind(b.status.map(LotStatus::as_str))
        .bind(production)
        .bind(expiry)
        .execute(&mut *tx)
        .await?;
    let lot = inventory::lot(&mut tx, id).await?;
    tx.commit().await?;
    Ok(ok(lot))
}

// ---------------------------------------------------------------- Tracciabilità

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Shipment {
    movement_id: Uuid,
    quantity: Decimal,
    occurred_at: DateTime<Utc>,
    sales_order_id: Option<Uuid>,
    sales_order_number: Option<String>,
    customer_name: Option<String>,
    external_ref: Option<String>,
}

macro_rules! shipment_select {
    () => {
        "SELECT m.id AS movement_id, m.quantity, m.occurred_at, m.sales_order_id, so.number AS sales_order_number,
                so.customer_name, COALESCE(m.external_ref, so.external_ref) AS external_ref
         FROM movements m LEFT JOIN sales_orders so ON so.id = m.sales_order_id
         WHERE m.type = 'OUTBOUND' AND m.reverses_movement_id IS NULL
           AND NOT EXISTS (SELECT 1 FROM movements r WHERE r.reverses_movement_id = m.id) "
    };
}

#[derive(Serialize, sqlx::FromRow)]
pub struct LotTotals {
    received: Decimal,
    shipped: Decimal,
    adjusted_in: Decimal,
    adjusted_out: Decimal,
    on_hand: Decimal,
}

#[derive(Serialize)]
pub struct LotTrace {
    lot: Lot,
    totals: LotTotals,
    current_stock: Vec<StockItem>,
    movements: Vec<Movement>,
    shipments: Vec<Shipment>,
}

pub async fn trace_lot(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<LotTrace>> {
    let mut conn = pool.acquire().await?;
    let lot = inventory::lot(&mut conn, id).await?;
    // Le coppie movimento/storno si annullano e non entrano nei totali.
    let totals = sqlx::query_as(
        "WITH m AS (
             SELECT * FROM movements m
             WHERE m.lot_id = $1 AND m.reverses_movement_id IS NULL
               AND NOT EXISTS (SELECT 1 FROM movements r WHERE r.reverses_movement_id = m.id)
         )
         SELECT COALESCE(sum(quantity) FILTER (WHERE type = 'INBOUND'), 0) AS received,
                COALESCE(sum(quantity) FILTER (WHERE type = 'OUTBOUND'), 0) AS shipped,
                COALESCE(sum(quantity) FILTER (WHERE type = 'ADJUSTMENT' AND direction = 'IN'), 0) AS adjusted_in,
                COALESCE(sum(quantity) FILTER (WHERE type = 'ADJUSTMENT' AND direction = 'OUT'), 0) AS adjusted_out,
                COALESCE((SELECT sum(s.quantity) FROM stock s WHERE s.lot_id = $1), 0) AS on_hand
         FROM m",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    let current_stock = sqlx::query_as(concat!(stock_select!(), "WHERE s.lot_id = $1 ORDER BY loc.code"))
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
    let movements = sqlx::query_as(concat!(
        movement_select!(),
        "WHERE m.lot_id = $1 ORDER BY m.occurred_at, m.number"
    ))
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let shipments = sqlx::query_as(concat!(shipment_select!(), "AND m.lot_id = $1 ORDER BY m.occurred_at"))
        .bind(id)
        .fetch_all(&mut *conn)
        .await?;
    Ok(ok(LotTrace {
        lot,
        totals,
        current_stock,
        movements,
        shipments,
    }))
}

#[derive(Deserialize)]
pub struct SerialFilter {
    page: Option<i64>,
    page_size: Option<i64>,
    serial_number: Option<String>,
    product_id: Option<Uuid>,
    lot_id: Option<Uuid>,
    location_id: Option<Uuid>,
    status: Option<SerialStatus>,
}

macro_rules! serial_filter {
    () => {
        "WHERE ($1::text IS NULL OR sr.serial_number ILIKE '%' || $1 || '%')
           AND ($2::uuid IS NULL OR sr.product_id = $2)
           AND ($3::uuid IS NULL OR sr.lot_id = $3)
           AND ($4::uuid IS NULL OR sr.location_id = $4)
           AND ($5::text IS NULL OR sr.status = $5) "
    };
}

pub async fn search_serials(
    State(pool): State<PgPool>,
    Query(f): Query<SerialFilter>,
) -> ApiResult<Json<Page<Serial>>> {
    let (limit, offset, page, page_size) = PageParams {
        page: f.page,
        page_size: f.page_size,
    }
    .sql()?;
    let status = f.status.map(SerialStatus::as_str);
    let items = sqlx::query_as(concat!(
        serial_select!(),
        serial_filter!(),
        "ORDER BY p.sku, sr.serial_number LIMIT $6 OFFSET $7"
    ))
    .bind(&f.serial_number)
    .bind(f.product_id)
    .bind(f.lot_id)
    .bind(f.location_id)
    .bind(status)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    let total = sqlx::query_scalar(concat!("SELECT count(*) FROM serials sr ", serial_filter!()))
        .bind(&f.serial_number)
        .bind(f.product_id)
        .bind(f.lot_id)
        .bind(f.location_id)
        .bind(status)
        .fetch_one(&pool)
        .await?;
    Ok(ok(Page {
        items,
        page,
        page_size,
        total,
    }))
}

#[derive(Serialize)]
pub struct SerialTrace {
    serial: Serial,
    movements: Vec<Movement>,
    shipment: Option<Shipment>,
}

pub async fn trace_serial(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<SerialTrace>> {
    let mut conn = pool.acquire().await?;
    let serial: Serial = sqlx::query_as(concat!(serial_select!(), "WHERE sr.id = $1"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ApiError::not_found("serial", id))?;
    let movements = sqlx::query_as(concat!(
        movement_select!(),
        "WHERE m.id IN (SELECT movement_id FROM movement_serials WHERE serial_id = $1) ORDER BY m.occurred_at, m.number"
    ))
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let shipment = if serial.status == "SHIPPED" {
        sqlx::query_as(concat!(
            shipment_select!(),
            "AND m.id IN (SELECT movement_id FROM movement_serials WHERE serial_id = $1) ORDER BY m.number DESC LIMIT 1"
        ))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
    } else {
        None
    };
    Ok(ok(SerialTrace {
        serial,
        movements,
        shipment,
    }))
}
