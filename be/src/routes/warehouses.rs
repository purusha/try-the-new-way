//! Magazzini, zone e ubicazioni (mappa del magazzino).

use axum::extract::State;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::capacity::{self, Load};
use crate::error::{ApiError, ApiResult};
use crate::http::{Body, IfMatch, Json, Operator, Path, Query, created, ok};
use crate::inventory;
use crate::model::{StockItem, StorageType, Warehouse, Zone, check_scale, non_empty, nullable};
use crate::pagination::{Items, Page, PageParams};
use crate::stock_select;

// ---------------------------------------------------------------- Magazzini

#[derive(Deserialize)]
pub struct WarehouseFilter {
    #[serde(flatten)]
    page: PageParams,
    active: Option<bool>,
}

pub async fn list_warehouses(
    State(pool): State<PgPool>,
    Query(f): Query<WarehouseFilter>,
) -> ApiResult<Json<Page<Warehouse>>> {
    let (limit, offset, page, page_size) = f.page.sql()?;
    let items = sqlx::query_as(
        "SELECT * FROM warehouses WHERE ($1::bool IS NULL OR active = $1) ORDER BY code LIMIT $2 OFFSET $3",
    )
    .bind(f.active)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    let total = sqlx::query_scalar("SELECT count(*) FROM warehouses WHERE ($1::bool IS NULL OR active = $1)")
        .bind(f.active)
        .fetch_one(&pool)
        .await?;
    Ok(ok(Page {
        items,
        page,
        page_size,
        total,
    }))
}

#[derive(Deserialize)]
pub struct WarehouseCreate {
    code: String,
    name: String,
    #[serde(default)]
    address: Option<String>,
}

pub async fn create_warehouse(
    State(pool): State<PgPool>,
    _op: Operator,
    Body(b): Body<WarehouseCreate>,
) -> ApiResult<Json<Warehouse>> {
    non_empty(&b.code, "code")?;
    non_empty(&b.name, "name")?;
    let w: Warehouse = sqlx::query_as("INSERT INTO warehouses (code, name, address) VALUES ($1, $2, $3) RETURNING *")
        .bind(b.code.trim())
        .bind(b.name.trim())
        .bind(b.address)
        .fetch_one(&pool)
        .await?;
    let version = w.version;
    Ok(created(w).etag(version))
}

async fn find_warehouse(pool: &PgPool, id: Uuid) -> ApiResult<Warehouse> {
    sqlx::query_as("SELECT * FROM warehouses WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found("warehouse", id))
}

pub async fn get_warehouse(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<Warehouse>> {
    let w = find_warehouse(&pool, id).await?;
    let version = w.version;
    Ok(ok(w).etag(version))
}

#[derive(Deserialize)]
pub struct WarehouseUpdate {
    name: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    address: Option<Option<String>>,
    active: Option<bool>,
}

pub async fn update_warehouse(
    State(pool): State<PgPool>,
    _op: Operator,
    if_match: IfMatch,
    Path(id): Path<Uuid>,
    Body(b): Body<WarehouseUpdate>,
) -> ApiResult<Json<Warehouse>> {
    let mut tx = pool.begin().await?;
    let current: Warehouse = sqlx::query_as("SELECT * FROM warehouses WHERE id = $1 FOR UPDATE")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| ApiError::not_found("warehouse", id))?;
    if_match.check(current.version)?;
    if let Some(name) = &b.name {
        non_empty(name, "name")?;
    }
    let w: Warehouse = sqlx::query_as(
        "UPDATE warehouses SET name = COALESCE($2, name), address = CASE WHEN $3 THEN $4 ELSE address END,
                active = COALESCE($5, active), version = version + 1, updated_at = now()
         WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(b.name.map(|n| n.trim().to_string()))
    .bind(b.address.is_some())
    .bind(b.address.flatten())
    .bind(b.active)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    let version = w.version;
    Ok(ok(w).etag(version))
}

// ---------------------------------------------------------------- Zone

pub async fn list_zones(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<Items<Zone>>> {
    find_warehouse(&pool, id).await?;
    let items = sqlx::query_as("SELECT * FROM zones WHERE warehouse_id = $1 ORDER BY code")
        .bind(id)
        .fetch_all(&pool)
        .await?;
    Ok(ok(Items { items }))
}

#[derive(Deserialize)]
pub struct ZoneCreate {
    code: String,
    name: String,
}

pub async fn create_zone(
    State(pool): State<PgPool>,
    _op: Operator,
    Path(id): Path<Uuid>,
    Body(b): Body<ZoneCreate>,
) -> ApiResult<Json<Zone>> {
    non_empty(&b.code, "code")?;
    non_empty(&b.name, "name")?;
    find_warehouse(&pool, id).await?;
    let z = sqlx::query_as("INSERT INTO zones (warehouse_id, code, name) VALUES ($1, $2, $3) RETURNING *")
        .bind(id)
        .bind(b.code.trim())
        .bind(b.name.trim())
        .fetch_one(&pool)
        .await?;
    Ok(created(z))
}

#[derive(Deserialize)]
pub struct ZoneUpdate {
    name: String,
}

pub async fn update_zone(
    State(pool): State<PgPool>,
    _op: Operator,
    Path(id): Path<Uuid>,
    Body(b): Body<ZoneUpdate>,
) -> ApiResult<Json<Zone>> {
    non_empty(&b.name, "name")?;
    let z = sqlx::query_as("UPDATE zones SET name = $2 WHERE id = $1 RETURNING *")
        .bind(id)
        .bind(b.name.trim())
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| ApiError::not_found("zone", id))?;
    Ok(ok(z))
}

// ---------------------------------------------------------------- Ubicazioni

#[derive(sqlx::FromRow)]
struct LocationRow {
    id: Uuid,
    warehouse_id: Uuid,
    zone_id: Uuid,
    zone_code: String,
    code: String,
    aisle: Option<String>,
    shelf: Option<String>,
    level: Option<String>,
    storage_type: String,
    max_weight_kg: Option<Decimal>,
    max_volume_m3: Option<Decimal>,
    active: bool,
    version: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    occ_weight: Decimal,
    occ_volume: Decimal,
}

#[derive(Serialize)]
pub struct Occupancy {
    weight_kg: Decimal,
    volume_m3: Decimal,
    weight_pct: Option<Decimal>,
    volume_pct: Option<Decimal>,
    full: bool,
}

#[derive(Serialize)]
pub struct Location {
    id: Uuid,
    warehouse_id: Uuid,
    zone_id: Uuid,
    zone_code: String,
    code: String,
    aisle: Option<String>,
    shelf: Option<String>,
    level: Option<String>,
    storage_type: String,
    max_weight_kg: Option<Decimal>,
    max_volume_m3: Option<Decimal>,
    active: bool,
    version: i32,
    occupancy: Occupancy,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<LocationRow> for Location {
    fn from(r: LocationRow) -> Self {
        let load = Load {
            weight_kg: r.occ_weight,
            volume_m3: r.occ_volume,
        };
        Location {
            occupancy: Occupancy {
                weight_kg: r.occ_weight,
                volume_m3: r.occ_volume,
                weight_pct: capacity::percent(r.occ_weight, r.max_weight_kg),
                volume_pct: capacity::percent(r.occ_volume, r.max_volume_m3),
                full: capacity::is_full(load, r.max_weight_kg, r.max_volume_m3),
            },
            id: r.id,
            warehouse_id: r.warehouse_id,
            zone_id: r.zone_id,
            zone_code: r.zone_code,
            code: r.code,
            aisle: r.aisle,
            shelf: r.shelf,
            level: r.level,
            storage_type: r.storage_type,
            max_weight_kg: r.max_weight_kg,
            max_volume_m3: r.max_volume_m3,
            active: r.active,
            version: r.version,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

macro_rules! location_from {
    () => {
        "FROM locations l
         JOIN zones z ON z.id = l.zone_id
         LEFT JOIN LATERAL (
             SELECT COALESCE(sum(s.quantity * p.unit_weight_kg), 0) AS weight,
                    COALESCE(sum(s.quantity * p.unit_volume_m3), 0) AS volume
             FROM stock s JOIN products p ON p.id = s.product_id WHERE s.location_id = l.id
         ) o ON true "
    };
}

macro_rules! location_select {
    () => {
        concat!(
            "SELECT l.id, l.warehouse_id, l.zone_id, z.code AS zone_code, l.code, l.aisle, l.shelf, l.level,
                    l.storage_type, l.max_weight_kg, l.max_volume_m3, l.active, l.version, l.created_at,
                    l.updated_at, round(o.weight, 3) AS occ_weight, round(o.volume, 6) AS occ_volume ",
            location_from!()
        )
    };
}

macro_rules! location_filter {
    () => {
        "WHERE l.warehouse_id = $1
           AND ($2::uuid IS NULL OR l.zone_id = $2)
           AND ($3::text IS NULL OR l.storage_type = $3)
           AND ($4::bool IS NULL OR l.active = $4)
           AND ($5::bool IS NULL OR $5 = NOT ((l.max_weight_kg IS NOT NULL AND o.weight >= l.max_weight_kg)
                                           OR (l.max_volume_m3 IS NOT NULL AND o.volume >= l.max_volume_m3)))
           AND ($6::text IS NULL OR l.code ILIKE '%' || $6 || '%') "
    };
}

async fn find_location(pool: &PgPool, id: Uuid) -> ApiResult<Location> {
    let row: LocationRow = sqlx::query_as(concat!(location_select!(), "WHERE l.id = $1"))
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ApiError::not_found("location", id))?;
    Ok(row.into())
}

#[derive(Deserialize)]
pub struct LocationFilter {
    #[serde(flatten)]
    page: PageParams,
    zone_id: Option<Uuid>,
    storage_type: Option<StorageType>,
    active: Option<bool>,
    has_capacity: Option<bool>,
    q: Option<String>,
}

pub async fn list_locations(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Query(f): Query<LocationFilter>,
) -> ApiResult<Json<Page<Location>>> {
    find_warehouse(&pool, id).await?;
    let (limit, offset, page, page_size) = f.page.sql()?;
    let storage = f.storage_type.map(StorageType::as_str);
    let rows: Vec<LocationRow> = sqlx::query_as(concat!(
        location_select!(),
        location_filter!(),
        "ORDER BY l.code LIMIT $7 OFFSET $8"
    ))
    .bind(id)
    .bind(f.zone_id)
    .bind(storage)
    .bind(f.active)
    .bind(f.has_capacity)
    .bind(&f.q)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    let total = sqlx::query_scalar(concat!("SELECT count(*) ", location_from!(), location_filter!()))
        .bind(id)
        .bind(f.zone_id)
        .bind(storage)
        .bind(f.active)
        .bind(f.has_capacity)
        .bind(&f.q)
        .fetch_one(&pool)
        .await?;
    Ok(ok(Page {
        items: rows.into_iter().map(Location::from).collect(),
        page,
        page_size,
        total,
    }))
}

#[derive(Deserialize)]
pub struct LocationCreate {
    zone_id: Uuid,
    code: String,
    #[serde(default)]
    aisle: Option<String>,
    #[serde(default)]
    shelf: Option<String>,
    #[serde(default)]
    level: Option<String>,
    storage_type: StorageType,
    #[serde(default)]
    max_weight_kg: Option<Decimal>,
    #[serde(default)]
    max_volume_m3: Option<Decimal>,
}

fn check_max(value: Option<Decimal>, field: &str) -> ApiResult<()> {
    if let Some(v) = value {
        if v <= Decimal::ZERO {
            return Err(ApiError::validation(format!(
                "{field} deve essere maggiore di zero (null = illimitato)"
            )));
        }
        check_scale(v.round_dp(3), field)?;
    }
    Ok(())
}

pub async fn create_location(
    State(pool): State<PgPool>,
    _op: Operator,
    Path(warehouse_id): Path<Uuid>,
    Body(b): Body<LocationCreate>,
) -> ApiResult<Json<Location>> {
    non_empty(&b.code, "code")?;
    check_max(b.max_weight_kg, "max_weight_kg")?;
    check_max(b.max_volume_m3, "max_volume_m3")?;
    find_warehouse(&pool, warehouse_id).await?;
    let zone_wh: Option<Uuid> = sqlx::query_scalar("SELECT warehouse_id FROM zones WHERE id = $1")
        .bind(b.zone_id)
        .fetch_optional(&pool)
        .await?;
    match zone_wh {
        None => return Err(ApiError::not_found("zone", b.zone_id)),
        Some(w) if w != warehouse_id => {
            return Err(ApiError::validation("La zona indicata appartiene a un altro magazzino"));
        }
        _ => {}
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO locations (warehouse_id, zone_id, code, aisle, shelf, level, storage_type, max_weight_kg, max_volume_m3)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(warehouse_id)
    .bind(b.zone_id)
    .bind(b.code.trim())
    .bind(b.aisle)
    .bind(b.shelf)
    .bind(b.level)
    .bind(b.storage_type.as_str())
    .bind(b.max_weight_kg)
    .bind(b.max_volume_m3)
    .fetch_one(&pool)
    .await?;
    let loc = find_location(&pool, id).await?;
    let version = loc.version;
    Ok(created(loc).etag(version))
}

pub async fn get_location(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<Location>> {
    let loc = find_location(&pool, id).await?;
    let version = loc.version;
    Ok(ok(loc).etag(version))
}

#[derive(Deserialize)]
pub struct LocationUpdate {
    #[serde(default, deserialize_with = "nullable")]
    aisle: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    shelf: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    level: Option<Option<String>>,
    storage_type: Option<StorageType>,
    #[serde(default, deserialize_with = "nullable")]
    max_weight_kg: Option<Option<Decimal>>,
    #[serde(default, deserialize_with = "nullable")]
    max_volume_m3: Option<Option<Decimal>>,
    active: Option<bool>,
}

pub async fn update_location(
    State(pool): State<PgPool>,
    _op: Operator,
    if_match: IfMatch,
    Path(id): Path<Uuid>,
    Body(b): Body<LocationUpdate>,
) -> ApiResult<Json<Location>> {
    check_max(b.max_weight_kg.flatten(), "max_weight_kg")?;
    check_max(b.max_volume_m3.flatten(), "max_volume_m3")?;
    let mut tx = pool.begin().await?;
    let (version, current_max_w, current_max_v): (i32, Option<Decimal>, Option<Decimal>) =
        sqlx::query_as("SELECT version, max_weight_kg, max_volume_m3 FROM locations WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| ApiError::not_found("location", id))?;
    if_match.check(version)?;

    let load = inventory::location_load(&mut tx, id).await?;
    let new_max_w = b.max_weight_kg.unwrap_or(current_max_w);
    let new_max_v = b.max_volume_m3.unwrap_or(current_max_v);
    if let Err(e) = capacity::check(load, Load::default(), new_max_w, new_max_v) {
        return Err(ApiError::conflict(
            "LOCATION_CAPACITY_EXCEEDED",
            "La nuova capienza è inferiore all'occupazione attuale dell'ubicazione",
        )
        .with(
            json!({ "location_id": id, "dimension": e.dimension, "current": e.current, "incoming": 0, "max": e.max }),
        ));
    }
    if b.active == Some(false) {
        let qty: Decimal = sqlx::query_scalar("SELECT COALESCE(sum(quantity), 0) FROM stock WHERE location_id = $1")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if qty > Decimal::ZERO {
            return Err(ApiError::conflict(
                "LOCATION_NOT_EMPTY",
                "Non si può disattivare un'ubicazione con giacenza",
            )
            .with(json!({ "location_id": id, "quantity": qty })));
        }
    }
    sqlx::query(
        "UPDATE locations SET
            aisle = CASE WHEN $2 THEN $3 ELSE aisle END,
            shelf = CASE WHEN $4 THEN $5 ELSE shelf END,
            level = CASE WHEN $6 THEN $7 ELSE level END,
            storage_type = COALESCE($8, storage_type),
            max_weight_kg = $9, max_volume_m3 = $10,
            active = COALESCE($11, active),
            version = version + 1, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(b.aisle.is_some())
    .bind(b.aisle.flatten())
    .bind(b.shelf.is_some())
    .bind(b.shelf.flatten())
    .bind(b.level.is_some())
    .bind(b.level.flatten())
    .bind(b.storage_type.map(StorageType::as_str))
    .bind(new_max_w)
    .bind(new_max_v)
    .bind(b.active)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let loc = find_location(&pool, id).await?;
    let version = loc.version;
    Ok(ok(loc).etag(version))
}

#[derive(Serialize)]
pub struct LocationContent {
    location: Location,
    items: Vec<StockItem>,
}

pub async fn location_stock(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<LocationContent>> {
    let location = find_location(&pool, id).await?;
    let items = sqlx::query_as(concat!(
        stock_select!(),
        "WHERE s.location_id = $1 ORDER BY p.sku, l.expiry_date NULLS LAST"
    ))
    .bind(id)
    .fetch_all(&pool)
    .await?;
    Ok(ok(LocationContent { location, items }))
}
