//! Giacenze per ubicazione e alert.

use axum::extract::State;
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::alerts::Alert;
use crate::error::ApiResult;
use crate::http::{Json, Query, ok};
use crate::inventory::{self, AlertFilter};
use crate::model::{AlertType, StockItem};
use crate::pagination::{Items, Page, PageParams};
use crate::stock_select;

#[derive(Deserialize)]
pub struct StockFilter {
    page: Option<i64>,
    page_size: Option<i64>,
    warehouse_id: Option<Uuid>,
    zone_id: Option<Uuid>,
    location_id: Option<Uuid>,
    product_id: Option<Uuid>,
    lot_id: Option<Uuid>,
}

macro_rules! stock_filter {
    () => {
        "WHERE ($1::uuid IS NULL OR s.warehouse_id = $1)
           AND ($2::uuid IS NULL OR loc.zone_id = $2)
           AND ($3::uuid IS NULL OR s.location_id = $3)
           AND ($4::uuid IS NULL OR s.product_id = $4)
           AND ($5::uuid IS NULL OR s.lot_id = $5) "
    };
}

pub async fn list_stock(State(pool): State<PgPool>, Query(f): Query<StockFilter>) -> ApiResult<Json<Page<StockItem>>> {
    let (limit, offset, page, page_size) = PageParams {
        page: f.page,
        page_size: f.page_size,
    }
    .sql()?;
    let items = sqlx::query_as(concat!(
        stock_select!(),
        stock_filter!(),
        "ORDER BY p.sku, loc.code, l.expiry_date NULLS LAST LIMIT $6 OFFSET $7"
    ))
    .bind(f.warehouse_id)
    .bind(f.zone_id)
    .bind(f.location_id)
    .bind(f.product_id)
    .bind(f.lot_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    let total = sqlx::query_scalar(concat!(
        "SELECT count(*) FROM stock s JOIN locations loc ON loc.id = s.location_id ",
        stock_filter!()
    ))
    .bind(f.warehouse_id)
    .bind(f.zone_id)
    .bind(f.location_id)
    .bind(f.product_id)
    .bind(f.lot_id)
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
pub struct AlertQuery {
    warehouse_id: Option<Uuid>,
    product_id: Option<Uuid>,
    #[serde(rename = "type")]
    kind: Option<AlertType>,
}

pub async fn list_alerts(State(pool): State<PgPool>, Query(q): Query<AlertQuery>) -> ApiResult<Json<Items<Alert>>> {
    let mut conn = pool.acquire().await?;
    let mut items = inventory::alert_query(
        &mut conn,
        AlertFilter {
            warehouse_id: q.warehouse_id,
            product_id: q.product_id,
            ..Default::default()
        },
    )
    .await?;
    if let Some(kind) = q.kind {
        items.retain(|a| a.kind == kind.as_str());
    }
    Ok(ok(Items { items }))
}
