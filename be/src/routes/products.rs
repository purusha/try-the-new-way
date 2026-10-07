//! Anagrafica prodotti, ricerca avanzata e disponibilità.

use axum::extract::State;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::alerts::Alert;
use crate::error::{ApiError, ApiResult};
use crate::http::{Body, IfMatch, Json, Operator, Path, Query, created, ok};
use crate::inventory::{self, AlertFilter};
use crate::model::{Product, ProductStatus, StorageType, Tracking, Uom, check_scale, non_empty, nullable};
use crate::pagination::{Page, PageParams};

#[derive(Debug, Default, Serialize, sqlx::FromRow)]
pub struct StockFigures {
    physical: Decimal,
    unusable: Decimal,
    reserved: Decimal,
    available: Decimal,
    below_min_stock: bool,
    expiring_quantity: Decimal,
    expired_quantity: Decimal,
}

#[derive(Serialize)]
pub struct ProductListItem {
    #[serde(flatten)]
    product: Product,
    stock: StockFigures,
}

#[derive(sqlx::FromRow)]
struct ListRow {
    #[sqlx(flatten)]
    product: Product,
    #[sqlx(flatten)]
    stock: StockFigures,
}

#[derive(Deserialize)]
pub struct ProductFilter {
    page: Option<i64>,
    page_size: Option<i64>,
    q: Option<String>,
    category: Option<String>,
    status: Option<ProductStatus>,
    tracking: Option<Tracking>,
    warehouse_id: Option<Uuid>,
    below_min_stock: Option<bool>,
    expiring_within_days: Option<i32>,
    sort: Option<String>,
}

macro_rules! product_list_from {
    () => {
        "FROM products p
         LEFT JOIN LATERAL (
             SELECT COALESCE(sum(f.physical), 0) AS physical, COALESCE(sum(f.unusable), 0) AS unusable,
                    COALESCE(sum(f.reserved), 0) AS reserved, COALESCE(sum(f.available), 0) AS available,
                    COALESCE(sum(f.expiring), 0) AS expiring_quantity, COALESCE(sum(f.expired), 0) AS expired_quantity
             FROM product_warehouse_figures f
             WHERE f.product_id = p.id AND ($1::uuid IS NULL OR f.warehouse_id = $1)
         ) f ON true
         WHERE ($2::text IS NULL OR p.sku ILIKE '%' || $2 || '%' OR p.ean ILIKE '%' || $2 || '%' OR p.name ILIKE '%' || $2 || '%')
           AND ($3::text IS NULL OR lower(p.category) = lower($3))
           AND ($4::text IS NULL OR p.status = $4)
           AND ($5::text IS NULL OR p.tracking = $5)
           AND ($6::bool IS NULL OR $6 = (p.min_stock > 0 AND f.available < p.min_stock))
           AND ($7::int IS NULL OR EXISTS (
                 SELECT 1 FROM stock s JOIN lots l ON l.id = s.lot_id
                 WHERE s.product_id = p.id AND s.quantity > 0 AND ($1::uuid IS NULL OR s.warehouse_id = $1)
                   AND l.expiry_date >= CURRENT_DATE AND l.expiry_date <= CURRENT_DATE + $7)) "
    };
}

const SORTS: [&str; 10] = [
    "sku",
    "-sku",
    "name",
    "-name",
    "category",
    "-category",
    "created_at",
    "-created_at",
    "available",
    "-available",
];

pub async fn list_products(
    State(pool): State<PgPool>,
    Query(f): Query<ProductFilter>,
) -> ApiResult<Json<Page<ProductListItem>>> {
    let (limit, offset, page, page_size) = PageParams {
        page: f.page,
        page_size: f.page_size,
    }
    .sql()?;
    let sort = f.sort.as_deref().unwrap_or("sku");
    if !SORTS.contains(&sort) {
        return Err(ApiError::validation(format!(
            "sort non valido: valori ammessi {}",
            SORTS.join(", ")
        )));
    }
    if f.expiring_within_days.is_some_and(|d| d < 0) {
        return Err(ApiError::validation("expiring_within_days deve essere >= 0"));
    }
    let status = f.status.map(ProductStatus::as_str);
    let tracking = f.tracking.map(Tracking::as_str);
    let rows: Vec<ListRow> = sqlx::query_as(concat!(
        "SELECT p.id, p.sku, p.ean, p.name, p.description, p.category, p.uom, p.unit_price, p.currency,
                p.min_stock, p.tracking, p.requires_expiry, p.expiry_warning_days, p.unit_weight_kg,
                p.unit_volume_m3, p.required_storage_type, p.status, p.version, p.created_at, p.updated_at,
                f.physical, f.unusable, f.reserved, f.available, f.expiring_quantity, f.expired_quantity,
                (p.min_stock > 0 AND f.available < p.min_stock) AS below_min_stock ",
        product_list_from!(),
        "ORDER BY
            CASE WHEN $8 = 'sku' THEN p.sku END ASC, CASE WHEN $8 = '-sku' THEN p.sku END DESC,
            CASE WHEN $8 = 'name' THEN p.name END ASC, CASE WHEN $8 = '-name' THEN p.name END DESC,
            CASE WHEN $8 = 'category' THEN p.category END ASC, CASE WHEN $8 = '-category' THEN p.category END DESC,
            CASE WHEN $8 = 'created_at' THEN p.created_at END ASC, CASE WHEN $8 = '-created_at' THEN p.created_at END DESC,
            CASE WHEN $8 = 'available' THEN f.available END ASC, CASE WHEN $8 = '-available' THEN f.available END DESC,
            p.sku
         LIMIT $9 OFFSET $10"
    ))
    .bind(f.warehouse_id)
    .bind(&f.q)
    .bind(&f.category)
    .bind(status)
    .bind(tracking)
    .bind(f.below_min_stock)
    .bind(f.expiring_within_days)
    .bind(sort)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    let total = sqlx::query_scalar(concat!("SELECT count(*) ", product_list_from!()))
        .bind(f.warehouse_id)
        .bind(&f.q)
        .bind(&f.category)
        .bind(status)
        .bind(tracking)
        .bind(f.below_min_stock)
        .bind(f.expiring_within_days)
        .fetch_one(&pool)
        .await?;
    let items = rows
        .into_iter()
        .map(|r| ProductListItem {
            product: r.product,
            stock: r.stock,
        })
        .collect();
    Ok(ok(Page {
        items,
        page,
        page_size,
        total,
    }))
}

#[derive(Deserialize)]
pub struct ProductCreate {
    sku: String,
    #[serde(default)]
    ean: Option<String>,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    category: Option<String>,
    uom: Uom,
    #[serde(default)]
    unit_price: Decimal,
    #[serde(default)]
    currency: Option<String>,
    #[serde(default)]
    min_stock: Decimal,
    #[serde(default)]
    tracking: Option<Tracking>,
    #[serde(default)]
    requires_expiry: bool,
    #[serde(default)]
    expiry_warning_days: Option<i32>,
    #[serde(default)]
    unit_weight_kg: Decimal,
    #[serde(default)]
    unit_volume_m3: Decimal,
    #[serde(default)]
    required_storage_type: Option<StorageType>,
}

fn check_ean(ean: &Option<String>) -> ApiResult<()> {
    if let Some(e) = ean
        && (!(8..=14).contains(&e.len()) || !e.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(ApiError::validation("ean deve contenere da 8 a 14 cifre"));
    }
    Ok(())
}

fn check_non_negative(v: Decimal, field: &str) -> ApiResult<()> {
    if v < Decimal::ZERO {
        return Err(ApiError::validation(format!("{field} non può essere negativo")));
    }
    Ok(())
}

fn check_currency(c: &str) -> ApiResult<()> {
    if c.len() != 3 || !c.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(ApiError::validation(
            "currency deve essere un codice ISO 4217 (es. EUR)",
        ));
    }
    Ok(())
}

pub async fn create_product(
    State(pool): State<PgPool>,
    _op: Operator,
    Body(b): Body<ProductCreate>,
) -> ApiResult<Json<Product>> {
    non_empty(&b.sku, "sku")?;
    non_empty(&b.name, "name")?;
    check_ean(&b.ean)?;
    for (v, f) in [
        (b.unit_price, "unit_price"),
        (b.min_stock, "min_stock"),
        (b.unit_weight_kg, "unit_weight_kg"),
        (b.unit_volume_m3, "unit_volume_m3"),
    ] {
        check_non_negative(v, f)?;
    }
    check_scale(b.min_stock, "min_stock")?;
    let currency = b.currency.unwrap_or_else(|| "EUR".into());
    check_currency(&currency)?;
    let tracking = b.tracking.unwrap_or(Tracking::None);
    if b.requires_expiry && tracking == Tracking::None {
        return Err(ApiError::validation("requires_expiry richiede tracking LOT o SERIAL"));
    }
    if b.expiry_warning_days.is_some_and(|d| d < 0) {
        return Err(ApiError::validation("expiry_warning_days deve essere >= 0"));
    }
    if tracking == Tracking::Serial && b.min_stock.fract() != Decimal::ZERO {
        return Err(ApiError::validation(
            "Per i prodotti a seriale min_stock deve essere intero",
        ));
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO products (sku, ean, name, description, category, uom, unit_price, currency, min_stock, tracking,
             requires_expiry, expiry_warning_days, unit_weight_kg, unit_volume_m3, required_storage_type)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, COALESCE($12, 30), $13, $14, $15) RETURNING id",
    )
    .bind(b.sku.trim())
    .bind(b.ean)
    .bind(b.name.trim())
    .bind(b.description)
    .bind(b.category.map(|c| c.trim().to_string()).filter(|c| !c.is_empty()))
    .bind(b.uom.as_str())
    .bind(b.unit_price)
    .bind(currency)
    .bind(b.min_stock)
    .bind(tracking.as_str())
    .bind(b.requires_expiry)
    .bind(b.expiry_warning_days)
    .bind(b.unit_weight_kg)
    .bind(b.unit_volume_m3)
    .bind(b.required_storage_type.map(StorageType::as_str))
    .fetch_one(&pool)
    .await?;
    let p = inventory::product(&mut *pool.acquire().await?, id).await?;
    let version = p.version;
    Ok(created(p).etag(version))
}

pub async fn get_product(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<Product>> {
    let p = inventory::product(&mut *pool.acquire().await?, id).await?;
    let version = p.version;
    Ok(ok(p).etag(version))
}

#[derive(Deserialize)]
pub struct ProductUpdate {
    #[serde(default, deserialize_with = "nullable")]
    ean: Option<Option<String>>,
    name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    description: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    category: Option<Option<String>>,
    uom: Option<Uom>,
    unit_price: Option<Decimal>,
    currency: Option<String>,
    min_stock: Option<Decimal>,
    expiry_warning_days: Option<i32>,
    unit_weight_kg: Option<Decimal>,
    unit_volume_m3: Option<Decimal>,
    #[serde(default, deserialize_with = "nullable")]
    required_storage_type: Option<Option<StorageType>>,
    // Campi non modificabili: se presenti la richiesta è rifiutata.
    sku: Option<serde_json::Value>,
    tracking: Option<serde_json::Value>,
    requires_expiry: Option<serde_json::Value>,
    status: Option<serde_json::Value>,
}

pub async fn update_product(
    State(pool): State<PgPool>,
    _op: Operator,
    if_match: IfMatch,
    Path(id): Path<Uuid>,
    Body(b): Body<ProductUpdate>,
) -> ApiResult<Json<Product>> {
    if b.sku.is_some() || b.tracking.is_some() || b.requires_expiry.is_some() {
        return Err(ApiError::validation(
            "sku, tracking e requires_expiry non sono modificabili",
        ));
    }
    if b.status.is_some() {
        return Err(ApiError::validation("Lo stato si cambia con /archive e /reactivate"));
    }
    if let Some(name) = &b.name {
        non_empty(name, "name")?;
    }
    if let Some(ean) = &b.ean {
        check_ean(ean)?;
    }
    for (v, f) in [
        (b.unit_price, "unit_price"),
        (b.min_stock, "min_stock"),
        (b.unit_weight_kg, "unit_weight_kg"),
        (b.unit_volume_m3, "unit_volume_m3"),
    ] {
        if let Some(v) = v {
            check_non_negative(v, f)?;
        }
    }
    if let Some(c) = &b.currency {
        check_currency(c)?;
    }
    if b.expiry_warning_days.is_some_and(|d| d < 0) {
        return Err(ApiError::validation("expiry_warning_days deve essere >= 0"));
    }
    let mut tx = pool.begin().await?;
    let current: i32 = sqlx::query_scalar("SELECT version FROM products WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("product", id))?;
    if_match.check(current)?;
    sqlx::query(
        "UPDATE products SET
            ean = CASE WHEN $2 THEN $3 ELSE ean END,
            name = COALESCE($4, name),
            description = CASE WHEN $5 THEN $6 ELSE description END,
            category = CASE WHEN $7 THEN $8 ELSE category END,
            uom = COALESCE($9, uom),
            unit_price = COALESCE($10, unit_price),
            currency = COALESCE($11, currency),
            min_stock = COALESCE($12, min_stock),
            expiry_warning_days = COALESCE($13, expiry_warning_days),
            unit_weight_kg = COALESCE($14, unit_weight_kg),
            unit_volume_m3 = COALESCE($15, unit_volume_m3),
            required_storage_type = CASE WHEN $16 THEN $17 ELSE required_storage_type END,
            version = version + 1, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(b.ean.is_some())
    .bind(b.ean.flatten())
    .bind(b.name.map(|n| n.trim().to_string()))
    .bind(b.description.is_some())
    .bind(b.description.flatten())
    .bind(b.category.is_some())
    .bind(b.category.flatten())
    .bind(b.uom.map(Uom::as_str))
    .bind(b.unit_price)
    .bind(b.currency)
    .bind(b.min_stock)
    .bind(b.expiry_warning_days)
    .bind(b.unit_weight_kg)
    .bind(b.unit_volume_m3)
    .bind(b.required_storage_type.is_some())
    .bind(b.required_storage_type.flatten().map(StorageType::as_str))
    .execute(&mut *tx)
    .await?;
    let p = inventory::product(&mut tx, id).await?;
    tx.commit().await?;
    let version = p.version;
    Ok(ok(p).etag(version))
}

pub async fn archive_product(
    State(pool): State<PgPool>,
    _op: Operator,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Product>> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT 1 FROM products WHERE id = $1 FOR UPDATE")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let p = inventory::product(&mut tx, id).await?;
    let (physical, reserved): (Decimal, Decimal) = sqlx::query_as(
        "SELECT COALESCE(sum(physical), 0), COALESCE(sum(reserved), 0) FROM product_warehouse_figures WHERE product_id = $1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if physical > Decimal::ZERO || reserved > Decimal::ZERO {
        return Err(ApiError::conflict(
            "PRODUCT_HAS_STOCK",
            format!(
                "Il prodotto {} ha giacenza o impegni aperti e non può essere archiviato",
                p.sku
            ),
        )
        .with(json!({ "physical": physical, "reserved": reserved })));
    }
    sqlx::query("UPDATE products SET status = 'INACTIVE', version = version + 1, updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let p = inventory::product(&mut tx, id).await?;
    tx.commit().await?;
    Ok(ok(p))
}

pub async fn reactivate_product(
    State(pool): State<PgPool>,
    _op: Operator,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Product>> {
    let mut tx = pool.begin().await?;
    inventory::product(&mut tx, id).await?;
    sqlx::query(
        "UPDATE products SET status = 'ACTIVE', version = version + 1, updated_at = now() WHERE id = $1 AND status <> 'ACTIVE'",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;
    let p = inventory::product(&mut tx, id).await?;
    tx.commit().await?;
    Ok(ok(p))
}

// ---------------------------------------------------------------- Disponibilità

#[derive(Serialize)]
pub struct WarehouseAvailability {
    warehouse_id: Uuid,
    warehouse_code: String,
    #[serde(flatten)]
    figures: StockFigures,
    alerts: Vec<Alert>,
}

#[derive(Serialize)]
pub struct ProductAvailability {
    product_id: Uuid,
    sku: String,
    uom: String,
    min_stock: Decimal,
    totals: StockFigures,
    warehouses: Vec<WarehouseAvailability>,
}

pub async fn product_availability(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ProductAvailability>> {
    let mut conn = pool.acquire().await?;
    let p = inventory::product(&mut conn, id).await?;

    #[derive(sqlx::FromRow)]
    struct Row {
        warehouse_id: Uuid,
        warehouse_code: String,
        physical: Decimal,
        unusable: Decimal,
        reserved: Decimal,
        available: Decimal,
        expiring: Decimal,
        expired: Decimal,
    }
    let rows: Vec<Row> = sqlx::query_as(
        "WITH pairs AS (
             SELECT warehouse_id FROM product_warehouse_figures WHERE product_id = $1
             UNION SELECT DISTINCT warehouse_id FROM movements WHERE product_id = $1
         )
         SELECT w.id AS warehouse_id, w.code AS warehouse_code,
                COALESCE(f.physical, 0) AS physical, COALESCE(f.unusable, 0) AS unusable,
                COALESCE(f.reserved, 0) AS reserved, COALESCE(f.available, 0) AS available,
                COALESCE(f.expiring, 0) AS expiring, COALESCE(f.expired, 0) AS expired
         FROM pairs x
         JOIN warehouses w ON w.id = x.warehouse_id
         LEFT JOIN product_warehouse_figures f ON f.product_id = $1 AND f.warehouse_id = x.warehouse_id
         ORDER BY w.code",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await?;
    let alerts = inventory::alert_query(
        &mut conn,
        AlertFilter {
            product_id: Some(id),
            ..Default::default()
        },
    )
    .await?;

    let below = |available: Decimal| p.min_stock > Decimal::ZERO && available < p.min_stock;
    let mut totals = StockFigures::default();
    let mut warehouses = Vec::with_capacity(rows.len());
    for r in rows {
        totals.physical += r.physical;
        totals.unusable += r.unusable;
        totals.reserved += r.reserved;
        totals.available += r.available;
        totals.expiring_quantity += r.expiring;
        totals.expired_quantity += r.expired;
        warehouses.push(WarehouseAvailability {
            warehouse_id: r.warehouse_id,
            warehouse_code: r.warehouse_code,
            figures: StockFigures {
                physical: r.physical,
                unusable: r.unusable,
                reserved: r.reserved,
                available: r.available,
                below_min_stock: below(r.available),
                expiring_quantity: r.expiring,
                expired_quantity: r.expired,
            },
            alerts: alerts
                .iter()
                .filter(|a| a.warehouse_id == Some(r.warehouse_id))
                .cloned()
                .collect(),
        });
    }
    totals.below_min_stock = below(totals.available);
    Ok(ok(ProductAvailability {
        product_id: p.id,
        sku: p.sku,
        uom: p.uom,
        min_stock: p.min_stock,
        totals,
        warehouses,
    }))
}
