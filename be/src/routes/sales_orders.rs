//! Ordini cliente: prenotazione (impegno soft per prodotto), rilascio, modifica, annullamento, evasione.

use axum::extract::State;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::domain::alerts::Alert;
use crate::domain::picking::Strategy;
use crate::error::{ApiError, ApiResult};
use crate::http::{Body, Json, Operator, OptBody, Path, Query, created, ok};
use crate::inventory::{self, FefoOverride, NewMovement, PickInput};
use crate::model::{Movement, PickMode, SalesOrderStatus, check_quantity, non_empty};
use crate::pagination::{Page, PageParams};
use crate::routes::operations::line_quantity;
use crate::routes::purchase_orders::LineInput;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SalesOrderLine {
    id: Uuid,
    line_no: i32,
    product_id: Uuid,
    sku: String,
    product_name: String,
    quantity_ordered: Decimal,
    quantity_reserved: Decimal,
    quantity_fulfilled: Decimal,
    quantity_open: Decimal,
}

#[derive(Debug, sqlx::FromRow)]
struct Header {
    id: Uuid,
    number: String,
    external_ref: Option<String>,
    customer_name: String,
    warehouse_id: Uuid,
    status: String,
    notes: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SalesOrder {
    id: Uuid,
    number: String,
    external_ref: Option<String>,
    customer_name: String,
    warehouse_id: Uuid,
    status: String,
    notes: Option<String>,
    lines: Vec<SalesOrderLine>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SalesOrderResult {
    #[serde(flatten)]
    order: SalesOrder,
    movements: Vec<Movement>,
    alerts: Vec<Alert>,
}

async fn assemble(conn: &mut PgConnection, headers: Vec<Header>) -> ApiResult<Vec<SalesOrder>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        order_id: Uuid,
        #[sqlx(flatten)]
        line: SalesOrderLine,
    }
    let ids: Vec<Uuid> = headers.iter().map(|h| h.id).collect();
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT sol.order_id, sol.id, sol.line_no, sol.product_id, p.sku, p.name AS product_name,
                sol.quantity_ordered, sol.quantity_reserved, sol.quantity_fulfilled,
                sol.quantity_ordered - sol.quantity_reserved - sol.quantity_fulfilled AS quantity_open
         FROM sales_order_lines sol JOIN products p ON p.id = sol.product_id
         WHERE sol.order_id = ANY($1) ORDER BY sol.line_no",
    )
    .bind(&ids)
    .fetch_all(conn)
    .await?;
    Ok(headers
        .into_iter()
        .map(|h| SalesOrder {
            lines: rows
                .iter()
                .filter(|r| r.order_id == h.id)
                .map(|r| r.line.clone())
                .collect(),
            id: h.id,
            number: h.number,
            external_ref: h.external_ref,
            customer_name: h.customer_name,
            warehouse_id: h.warehouse_id,
            status: h.status,
            notes: h.notes,
            created_at: h.created_at,
            updated_at: h.updated_at,
        })
        .collect())
}

async fn load(conn: &mut PgConnection, id: Uuid) -> ApiResult<SalesOrder> {
    let header: Header = sqlx::query_as("SELECT * FROM sales_orders WHERE id = $1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| ApiError::not_found("sales_order", id))?;
    Ok(assemble(conn, vec![header]).await?.remove(0))
}

/// Blocca l'ordine per la durata della transazione e ne restituisce lo stato.
async fn lock(conn: &mut PgConnection, id: Uuid) -> ApiResult<SalesOrder> {
    sqlx::query("SELECT 1 FROM sales_orders WHERE id = $1 FOR UPDATE")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    load(conn, id).await
}

fn ensure_state(order: &SalesOrder, action: &str, forbidden: &[&str]) -> ApiResult<()> {
    if forbidden.contains(&order.status.as_str()) {
        return Err(ApiError::conflict(
            "ORDER_STATE_INVALID",
            format!("Operazione '{action}' non ammessa su un ordine {}", order.status),
        )
        .with(json!({ "order_id": order.id, "status": order.status, "action": action })));
    }
    Ok(())
}

fn find_line(order: &SalesOrder, line_id: Uuid) -> ApiResult<&SalesOrderLine> {
    order.lines.iter().find(|l| l.id == line_id).ok_or_else(|| {
        ApiError::unprocessable("ORDER_LINE_NOT_FOUND", "La riga indicata non appartiene all'ordine")
            .with(json!({ "line_id": line_id }))
    })
}

/// Stato calcolato dalle righe (un ordine annullato resta annullato).
async fn refresh_status(conn: &mut PgConnection, id: Uuid) -> ApiResult<()> {
    sqlx::query(
        "UPDATE sales_orders so SET status = CASE
             WHEN x.all_fulfilled THEN 'FULFILLED'
             WHEN x.any_fulfilled THEN 'PARTIALLY_FULFILLED'
             WHEN x.all_reserved THEN 'RESERVED'
             WHEN x.any_reserved THEN 'PARTIALLY_RESERVED'
             ELSE 'OPEN' END,
             updated_at = now()
         FROM (SELECT bool_and(quantity_fulfilled = quantity_ordered) AS all_fulfilled,
                      bool_or(quantity_fulfilled > 0) AS any_fulfilled,
                      bool_and(quantity_reserved + quantity_fulfilled = quantity_ordered) AS all_reserved,
                      bool_or(quantity_reserved > 0) AS any_reserved
               FROM sales_order_lines WHERE order_id = $1) x
         WHERE so.id = $1 AND so.status <> 'CANCELLED'",
    )
    .bind(id)
    .execute(conn)
    .await?;
    Ok(())
}

struct Ctx<'a> {
    order: &'a SalesOrder,
    operation_id: Uuid,
    operator: &'a str,
}

/// Movimento `RESERVATION`/`RELEASE`: non tocca giacenze né ubicazioni.
async fn reservation_movement(
    conn: &mut PgConnection,
    ctx: &Ctx<'_>,
    kind: &'static str,
    line: &SalesOrderLine,
    quantity: Decimal,
    note: Option<String>,
) -> ApiResult<Uuid> {
    let delta = if kind == "RESERVATION" { quantity } else { -quantity };
    sqlx::query("UPDATE sales_order_lines SET quantity_reserved = quantity_reserved + $2 WHERE id = $1")
        .bind(line.id)
        .bind(delta)
        .execute(&mut *conn)
        .await?;
    inventory::insert_movement(
        conn,
        NewMovement {
            operation_id: ctx.operation_id,
            kind,
            product_id: line.product_id,
            warehouse_id: ctx.order.warehouse_id,
            quantity,
            reason_code: if kind == "RESERVATION" {
                "SALES_ORDER_RESERVATION"
            } else {
                "SALES_ORDER_RELEASE"
            }
            .into(),
            note,
            sales_order_id: Some(ctx.order.id),
            sales_order_line_id: Some(line.id),
            external_ref: ctx.order.external_ref.clone(),
            operator: ctx.operator.to_string(),
            ..Default::default()
        },
    )
    .await
}

async fn result(conn: &mut PgConnection, id: Uuid, movement_ids: &[Uuid]) -> ApiResult<SalesOrderResult> {
    let order = load(conn, id).await?;
    let movements = inventory::movements_by_id(conn, movement_ids).await?;
    let pairs: Vec<_> = order.lines.iter().map(|l| (l.product_id, order.warehouse_id)).collect();
    let alerts = inventory::alerts_for(conn, &pairs).await?;
    Ok(SalesOrderResult {
        order,
        movements,
        alerts,
    })
}

// ---------------------------------------------------------------- Lettura

#[derive(Deserialize)]
pub struct SalesOrderFilter {
    page: Option<i64>,
    page_size: Option<i64>,
    status: Option<SalesOrderStatus>,
    warehouse_id: Option<Uuid>,
    q: Option<String>,
}

macro_rules! so_filter {
    () => {
        "WHERE ($1::text IS NULL OR status = $1)
           AND ($2::uuid IS NULL OR warehouse_id = $2)
           AND ($3::text IS NULL OR number ILIKE '%' || $3 || '%' OR external_ref ILIKE '%' || $3 || '%'
                OR customer_name ILIKE '%' || $3 || '%') "
    };
}

pub async fn list(State(pool): State<PgPool>, Query(f): Query<SalesOrderFilter>) -> ApiResult<Json<Page<SalesOrder>>> {
    let (limit, offset, page, page_size) = PageParams {
        page: f.page,
        page_size: f.page_size,
    }
    .sql()?;
    let status = f.status.map(SalesOrderStatus::as_str);
    let mut conn = pool.acquire().await?;
    let headers = sqlx::query_as(concat!(
        "SELECT * FROM sales_orders ",
        so_filter!(),
        "ORDER BY created_at DESC, number DESC LIMIT $4 OFFSET $5"
    ))
    .bind(status)
    .bind(f.warehouse_id)
    .bind(&f.q)
    .bind(limit)
    .bind(offset)
    .fetch_all(&mut *conn)
    .await?;
    let total = sqlx::query_scalar(concat!("SELECT count(*) FROM sales_orders ", so_filter!()))
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

pub async fn get(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<SalesOrder>> {
    Ok(ok(load(&mut *pool.acquire().await?, id).await?))
}

// ---------------------------------------------------------------- Creazione

#[derive(Deserialize)]
pub struct SalesOrderCreate {
    #[serde(default)]
    number: Option<String>,
    #[serde(default)]
    external_ref: Option<String>,
    customer_name: String,
    warehouse_id: Uuid,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    reserve: bool,
    lines: Vec<LineInput>,
}

pub async fn create(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Body(b): Body<SalesOrderCreate>,
) -> ApiResult<Json<SalesOrderResult>> {
    non_empty(&b.customer_name, "customer_name")?;
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
            sqlx::query_scalar("SELECT 'SO-' || lpad(nextval('sales_order_number_seq')::text, 6, '0')")
                .fetch_one(&mut *tx)
                .await?
        }
    };
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO sales_orders (number, external_ref, customer_name, warehouse_id, notes) VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(number)
    .bind(b.external_ref)
    .bind(b.customer_name.trim())
    .bind(b.warehouse_id)
    .bind(b.notes)
    .fetch_one(&mut *tx)
    .await?;
    for (i, l) in b.lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO sales_order_lines (order_id, line_no, product_id, quantity_ordered) VALUES ($1, $2, $3, $4)",
        )
        .bind(id)
        .bind(i as i32 + 1)
        .bind(l.product_id)
        .bind(l.quantity)
        .execute(&mut *tx)
        .await?;
    }
    let ids = if b.reserve {
        let order = lock(&mut tx, id).await?;
        do_reserve(&mut tx, &order, &operator, &ReserveRequest::default()).await?
    } else {
        Vec::new()
    };
    refresh_status(&mut tx, id).await?;
    let res = result(&mut tx, id, &ids).await?;
    tx.commit().await?;
    Ok(created(res))
}

// ---------------------------------------------------------------- Prenotazione

#[derive(Debug, Default, Deserialize)]
pub struct LineQuantity {
    line_id: Uuid,
    quantity: Decimal,
}

#[derive(Debug, Default, Deserialize)]
pub struct ReserveRequest {
    #[serde(default)]
    lines: Option<Vec<LineQuantity>>,
    #[serde(default)]
    allow_partial: bool,
}

pub async fn reserve(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Path(id): Path<Uuid>,
    OptBody(b): OptBody<ReserveRequest>,
) -> ApiResult<Json<SalesOrderResult>> {
    let mut tx = pool.begin().await?;
    let order = lock(&mut tx, id).await?;
    let ids = do_reserve(&mut tx, &order, &operator, &b).await?;
    refresh_status(&mut tx, id).await?;
    let res = result(&mut tx, id, &ids).await?;
    tx.commit().await?;
    Ok(ok(res))
}

async fn do_reserve(
    conn: &mut PgConnection,
    order: &SalesOrder,
    operator: &str,
    b: &ReserveRequest,
) -> ApiResult<Vec<Uuid>> {
    ensure_state(order, "reserve", &["CANCELLED", "FULFILLED"])?;
    let requests: Vec<(&SalesOrderLine, Decimal)> = match &b.lines {
        Some(lines) => {
            let mut out = Vec::with_capacity(lines.len());
            for (i, lq) in lines.iter().enumerate() {
                check_quantity(lq.quantity, "lines.quantity").map_err(|e| e.at_line(i))?;
                let line = find_line(order, lq.line_id).map_err(|e| e.at_line(i))?;
                out.push((line, lq.quantity));
            }
            out
        }
        None => order
            .lines
            .iter()
            .filter(|l| l.quantity_open > Decimal::ZERO)
            .map(|l| (l, l.quantity_open))
            .collect(),
    };
    // Una riga indicata più volte conta per la somma delle richieste.
    for (line_id, requested) in inventory::sum_by(requests.iter().map(|(l, q)| (l.id, *q))) {
        let line = find_line(order, line_id)?;
        if requested > line.quantity_open {
            return Err(ApiError::conflict(
                "RESERVATION_EXCEEDS_ORDERED",
                format!(
                    "Riga {}: si possono impegnare al massimo {}",
                    line.line_no,
                    line.quantity_open.normalize()
                ),
            )
            .with(json!({ "line_id": line.id, "requested": requested, "open": line.quantity_open })));
        }
    }

    let warehouse = inventory::warehouse(conn, order.warehouse_id).await?;
    let pairs: Vec<_> = requests
        .iter()
        .map(|(l, _)| (l.product_id, order.warehouse_id))
        .collect();
    inventory::lock_pairs(conn, &pairs).await?;

    // Regola 1: per prodotto, l'impegno non può superare il disponibile del magazzino.
    let mut available = std::collections::HashMap::new();
    for (product_id, requested) in inventory::sum_by(requests.iter().map(|(l, q)| (l.product_id, *q))) {
        let p = inventory::product(conn, product_id).await?;
        inventory::ensure_active(&p)?;
        let f = inventory::figures(conn, product_id, order.warehouse_id).await?;
        if requested > f.available && !b.allow_partial {
            return Err(inventory::insufficient_availability(
                &p,
                &warehouse,
                requested,
                f.available,
            ));
        }
        available.insert(product_id, f.available.max(Decimal::ZERO));
    }

    let ctx = Ctx {
        order,
        operation_id: Uuid::new_v4(),
        operator,
    };
    let mut ids = Vec::new();
    for (line, requested) in requests {
        let left = available.get_mut(&line.product_id).expect("calcolato sopra");
        let mut quantity = requested.min(*left);
        if order_line_is_serial(conn, line.product_id).await? {
            quantity = quantity.floor();
        }
        if quantity <= Decimal::ZERO {
            continue;
        }
        *left -= quantity;
        ids.push(reservation_movement(conn, &ctx, "RESERVATION", line, quantity, None).await?);
    }
    Ok(ids)
}

async fn order_line_is_serial(conn: &mut PgConnection, product_id: Uuid) -> ApiResult<bool> {
    Ok(inventory::product(conn, product_id).await?.tracks_serials())
}

#[derive(Debug, Default, Deserialize)]
pub struct ReleaseRequest {
    #[serde(default)]
    lines: Option<Vec<LineQuantity>>,
    #[serde(default)]
    reason: Option<String>,
}

pub async fn release(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Path(id): Path<Uuid>,
    OptBody(b): OptBody<ReleaseRequest>,
) -> ApiResult<Json<SalesOrderResult>> {
    let mut tx = pool.begin().await?;
    let order = lock(&mut tx, id).await?;
    let requests: Vec<(&SalesOrderLine, Decimal)> = match &b.lines {
        Some(lines) => {
            let mut out = Vec::with_capacity(lines.len());
            for (i, lq) in lines.iter().enumerate() {
                check_quantity(lq.quantity, "lines.quantity").map_err(|e| e.at_line(i))?;
                out.push((find_line(&order, lq.line_id).map_err(|e| e.at_line(i))?, lq.quantity));
            }
            out
        }
        None => order
            .lines
            .iter()
            .filter(|l| l.quantity_reserved > Decimal::ZERO)
            .map(|l| (l, l.quantity_reserved))
            .collect(),
    };
    for (line_id, requested) in inventory::sum_by(requests.iter().map(|(l, q)| (l.id, *q))) {
        let line = find_line(&order, line_id)?;
        if requested > line.quantity_reserved {
            return Err(ApiError::conflict(
                "RELEASE_EXCEEDS_RESERVED",
                format!(
                    "Riga {}: impegnati solo {}",
                    line.line_no,
                    line.quantity_reserved.normalize()
                ),
            )
            .with(json!({ "line_id": line.id, "requested": requested, "reserved": line.quantity_reserved })));
        }
    }
    let pairs: Vec<_> = requests
        .iter()
        .map(|(l, _)| (l.product_id, order.warehouse_id))
        .collect();
    inventory::lock_pairs(&mut tx, &pairs).await?;
    let ctx = Ctx {
        order: &order,
        operation_id: Uuid::new_v4(),
        operator: &operator,
    };
    let mut ids = Vec::new();
    for (line, quantity) in requests {
        ids.push(reservation_movement(&mut tx, &ctx, "RELEASE", line, quantity, b.reason.clone()).await?);
    }
    refresh_status(&mut tx, id).await?;
    let res = result(&mut tx, id, &ids).await?;
    tx.commit().await?;
    Ok(ok(res))
}

#[derive(Debug, Deserialize)]
pub struct LineUpdate {
    quantity: Decimal,
}

pub async fn update_line(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Path((id, line_id)): Path<(Uuid, Uuid)>,
    Body(b): Body<LineUpdate>,
) -> ApiResult<Json<SalesOrderResult>> {
    check_quantity(b.quantity, "quantity")?;
    let mut tx = pool.begin().await?;
    let order = lock(&mut tx, id).await?;
    ensure_state(&order, "update_line", &["CANCELLED"])?;
    let line = find_line(&order, line_id)?.clone();
    let p = inventory::product(&mut tx, line.product_id).await?;
    inventory::check_whole_units(&p, b.quantity)?;
    if b.quantity < line.quantity_fulfilled {
        return Err(ApiError::conflict(
            "QUANTITY_BELOW_FULFILLED",
            format!(
                "La quantità non può scendere sotto l'evaso ({})",
                line.quantity_fulfilled.normalize()
            ),
        )
        .with(json!({ "line_id": line.id, "quantity": b.quantity, "fulfilled": line.quantity_fulfilled })));
    }
    let mut ids = Vec::new();
    let excess = line.quantity_reserved + line.quantity_fulfilled - b.quantity;
    if excess > Decimal::ZERO {
        inventory::lock_pairs(&mut tx, &[(line.product_id, order.warehouse_id)]).await?;
        let ctx = Ctx {
            order: &order,
            operation_id: Uuid::new_v4(),
            operator: &operator,
        };
        ids.push(
            reservation_movement(
                &mut tx,
                &ctx,
                "RELEASE",
                &line,
                excess,
                Some("Riduzione quantità ordinata".into()),
            )
            .await?,
        );
    }
    sqlx::query("UPDATE sales_order_lines SET quantity_ordered = $2 WHERE id = $1")
        .bind(line_id)
        .bind(b.quantity)
        .execute(&mut *tx)
        .await?;
    refresh_status(&mut tx, id).await?;
    let res = result(&mut tx, id, &ids).await?;
    tx.commit().await?;
    Ok(ok(res))
}

#[derive(Debug, Default, Deserialize)]
pub struct CancelRequest {
    #[serde(default)]
    reason: Option<String>,
}

pub async fn cancel(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Path(id): Path<Uuid>,
    OptBody(b): OptBody<CancelRequest>,
) -> ApiResult<Json<SalesOrderResult>> {
    let mut tx = pool.begin().await?;
    let order = lock(&mut tx, id).await?;
    ensure_state(&order, "cancel", &["CANCELLED", "FULFILLED"])?;
    let pairs: Vec<_> = order.lines.iter().map(|l| (l.product_id, order.warehouse_id)).collect();
    inventory::lock_pairs(&mut tx, &pairs).await?;
    let ctx = Ctx {
        order: &order,
        operation_id: Uuid::new_v4(),
        operator: &operator,
    };
    let mut ids = Vec::new();
    for line in order.lines.iter().filter(|l| l.quantity_reserved > Decimal::ZERO) {
        let note = Some(b.reason.clone().unwrap_or_else(|| "Annullamento ordine".into()));
        ids.push(reservation_movement(&mut tx, &ctx, "RELEASE", line, line.quantity_reserved, note).await?);
    }
    sqlx::query("UPDATE sales_orders SET status = 'CANCELLED', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let res = result(&mut tx, id, &ids).await?;
    tx.commit().await?;
    Ok(ok(res))
}

// ---------------------------------------------------------------- Evasione

#[derive(Debug, Deserialize)]
pub struct FulfillLine {
    line_id: Uuid,
    #[serde(default)]
    quantity: Option<Decimal>,
    #[serde(default)]
    picks: Vec<PickInput>,
}

#[derive(Debug, Default, Deserialize)]
pub struct FulfillRequest {
    #[serde(default)]
    mode: PickMode,
    #[serde(default)]
    strategy: Strategy,
    #[serde(default)]
    external_ref: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    occurred_at: Option<DateTime<Utc>>,
    #[serde(default)]
    fefo_override: Option<FefoOverride>,
    #[serde(default)]
    lines: Option<Vec<FulfillLine>>,
}

pub async fn fulfill(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Path(id): Path<Uuid>,
    OptBody(b): OptBody<FulfillRequest>,
) -> ApiResult<Json<SalesOrderResult>> {
    if let Some(o) = &b.fefo_override
        && o.reason.trim().len() < 3
    {
        return Err(ApiError::validation(
            "fefo_override.reason deve avere almeno 3 caratteri",
        ));
    }
    let mut tx = pool.begin().await?;
    let order = lock(&mut tx, id).await?;
    ensure_state(&order, "fulfill", &["CANCELLED", "FULFILLED"])?;

    let no_picks: Vec<PickInput> = Vec::new();
    let requests: Vec<(&SalesOrderLine, Decimal, &[PickInput])> = match &b.lines {
        Some(lines) => {
            let mut out = Vec::with_capacity(lines.len());
            for (i, fl) in lines.iter().enumerate() {
                let line = find_line(&order, fl.line_id).map_err(|e| e.at_line(i))?;
                let quantity = match (b.mode, fl.quantity) {
                    (PickMode::Auto, None) => line.quantity_reserved,
                    _ => line_quantity(b.mode, fl.quantity, &fl.picks).map_err(|e| e.at_line(i))?,
                };
                out.push((line, quantity, fl.picks.as_slice()));
            }
            out
        }
        None if b.mode == PickMode::Manual => {
            return Err(ApiError::validation(
                "In modalità MANUAL indicare le righe con i prelievi",
            ));
        }
        None => order
            .lines
            .iter()
            .filter(|l| l.quantity_reserved > Decimal::ZERO)
            .map(|l| (l, l.quantity_reserved, no_picks.as_slice()))
            .collect(),
    };
    if requests.is_empty() {
        return Err(ApiError::conflict(
            "FULFILLMENT_EXCEEDS_RESERVED",
            "L'ordine non ha quantità impegnate da evadere",
        )
        .with(json!({ "order_id": id, "reserved": 0 })));
    }
    for (line_id, requested) in inventory::sum_by(requests.iter().map(|(l, q, _)| (l.id, *q))) {
        let line = find_line(&order, line_id)?;
        if requested > line.quantity_reserved || requested <= Decimal::ZERO {
            return Err(ApiError::conflict(
                "FULFILLMENT_EXCEEDS_RESERVED",
                format!(
                    "Riga {}: impegnati {}, richiesti {}",
                    line.line_no,
                    line.quantity_reserved.normalize(),
                    requested.normalize()
                ),
            )
            .with(json!({ "line_id": line.id, "requested": requested, "reserved": line.quantity_reserved })));
        }
    }
    let pairs: Vec<_> = requests
        .iter()
        .map(|(l, _, _)| (l.product_id, order.warehouse_id))
        .collect();
    inventory::lock_pairs(&mut tx, &pairs).await?;

    let template = NewMovement {
        operation_id: Uuid::new_v4(),
        warehouse_id: order.warehouse_id,
        reason_code: "SALES_SHIPMENT".into(),
        note: b.note.clone(),
        sales_order_id: Some(order.id),
        external_ref: b.external_ref.clone().or_else(|| order.external_ref.clone()),
        operator: operator.clone(),
        occurred_at: b.occurred_at,
        ..Default::default()
    };
    let mut ids = Vec::new();
    for (i, (line, quantity, picks)) in requests.into_iter().enumerate() {
        let p = inventory::product(&mut tx, line.product_id).await?;
        inventory::check_whole_units(&p, quantity).map_err(|e| e.at_line(i))?;
        let plans = match b.mode {
            PickMode::Auto => inventory::plan_auto(&mut tx, &p, order.warehouse_id, quantity, b.strategy).await,
            PickMode::Manual => {
                inventory::plan_manual(&mut tx, &p, order.warehouse_id, picks, b.fefo_override.as_ref()).await
            }
        }
        .map_err(|e| e.at_line(i))?;
        let line_template = NewMovement {
            sales_order_line_id: Some(line.id),
            ..template.clone()
        };
        ids.extend(
            inventory::execute_outbound(&mut tx, &p, &plans, &line_template)
                .await
                .map_err(|e| e.at_line(i))?,
        );
        // Conversione dell'impegno in scarico effettivo.
        sqlx::query(
            "UPDATE sales_order_lines SET quantity_reserved = quantity_reserved - $2, quantity_fulfilled = quantity_fulfilled + $2
             WHERE id = $1",
        )
        .bind(line.id)
        .bind(quantity)
        .execute(&mut *tx)
        .await?;
    }
    refresh_status(&mut tx, id).await?;
    let res = result(&mut tx, id, &ids).await?;
    tx.commit().await?;
    Ok(ok(res))
}

/// Storno di uno scarico d'ordine: la quantità evasa della riga diminuisce.
pub async fn unfulfill_line(
    conn: &mut PgConnection,
    order_id: Uuid,
    line_id: Uuid,
    quantity: Decimal,
) -> ApiResult<()> {
    sqlx::query("SELECT 1 FROM sales_orders WHERE id = $1 FOR UPDATE")
        .bind(order_id)
        .execute(&mut *conn)
        .await?;
    sqlx::query("UPDATE sales_order_lines SET quantity_fulfilled = quantity_fulfilled - $2 WHERE id = $1")
        .bind(line_id)
        .bind(quantity)
        .execute(&mut *conn)
        .await?;
    refresh_status(conn, order_id).await
}
