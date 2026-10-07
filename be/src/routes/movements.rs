//! Storico immutabile dei movimenti e storni (spec 007, regola 4).

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::http::{Body, Json, Operator, Path, Query, created, ok};
use crate::inventory::{self, NewMovement, OperationResult};
use crate::model::{Movement, MovementType};
use crate::movement_select;
use crate::pagination::{Page, PageParams};
use crate::routes::operations::finish;
use crate::routes::{purchase_orders, sales_orders};

#[derive(Deserialize)]
pub struct MovementFilter {
    page: Option<i64>,
    page_size: Option<i64>,
    product_id: Option<Uuid>,
    lot_id: Option<Uuid>,
    serial_id: Option<Uuid>,
    location_id: Option<Uuid>,
    warehouse_id: Option<Uuid>,
    sales_order_id: Option<Uuid>,
    purchase_order_id: Option<Uuid>,
    order_ref: Option<String>,
    operation_id: Option<Uuid>,
    #[serde(rename = "type")]
    kind: Option<MovementType>,
    operator: Option<String>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

macro_rules! movement_filter {
    () => {
        "WHERE ($1::uuid IS NULL OR m.product_id = $1)
           AND ($2::uuid IS NULL OR m.lot_id = $2)
           AND ($3::uuid IS NULL OR EXISTS (SELECT 1 FROM movement_serials ms WHERE ms.movement_id = m.id AND ms.serial_id = $3))
           AND ($4::uuid IS NULL OR m.from_location_id = $4 OR m.to_location_id = $4)
           AND ($5::uuid IS NULL OR m.warehouse_id = $5)
           AND ($6::uuid IS NULL OR m.sales_order_id = $6)
           AND ($7::uuid IS NULL OR m.purchase_order_id = $7)
           AND ($8::text IS NULL OR so.number = $8 OR so.external_ref = $8 OR po.number = $8
                OR po.external_ref = $8 OR m.external_ref = $8)
           AND ($9::uuid IS NULL OR m.operation_id = $9)
           AND ($10::text IS NULL OR m.type = $10)
           AND ($11::text IS NULL OR m.operator = $11)
           AND ($12::timestamptz IS NULL OR m.occurred_at >= $12)
           AND ($13::timestamptz IS NULL OR m.occurred_at < $13) "
    };
}

pub async fn list(State(pool): State<PgPool>, Query(f): Query<MovementFilter>) -> ApiResult<Json<Page<Movement>>> {
    let (limit, offset, page, page_size) = PageParams {
        page: f.page,
        page_size: f.page_size,
    }
    .sql()?;
    let kind = f.kind.map(MovementType::as_str);
    let items = sqlx::query_as(concat!(
        movement_select!(),
        movement_filter!(),
        "ORDER BY m.occurred_at DESC, m.number DESC LIMIT $14 OFFSET $15"
    ))
    .bind(f.product_id)
    .bind(f.lot_id)
    .bind(f.serial_id)
    .bind(f.location_id)
    .bind(f.warehouse_id)
    .bind(f.sales_order_id)
    .bind(f.purchase_order_id)
    .bind(&f.order_ref)
    .bind(f.operation_id)
    .bind(kind)
    .bind(&f.operator)
    .bind(f.from)
    .bind(f.to)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;
    let total = sqlx::query_scalar(concat!(
        "SELECT count(*) FROM movements m
         LEFT JOIN sales_orders so ON so.id = m.sales_order_id
         LEFT JOIN purchase_orders po ON po.id = m.purchase_order_id ",
        movement_filter!()
    ))
    .bind(f.product_id)
    .bind(f.lot_id)
    .bind(f.serial_id)
    .bind(f.location_id)
    .bind(f.warehouse_id)
    .bind(f.sales_order_id)
    .bind(f.purchase_order_id)
    .bind(&f.order_ref)
    .bind(f.operation_id)
    .bind(kind)
    .bind(&f.operator)
    .bind(f.from)
    .bind(f.to)
    .fetch_one(&pool)
    .await?;
    Ok(ok(Page {
        items,
        page,
        page_size,
        total,
    }))
}

async fn find(conn: &mut sqlx::PgConnection, id: Uuid) -> ApiResult<Movement> {
    inventory::movements_by_id(conn, &[id])
        .await?
        .pop()
        .ok_or_else(|| ApiError::not_found("movement", id))
}

pub async fn get(State(pool): State<PgPool>, Path(id): Path<Uuid>) -> ApiResult<Json<Movement>> {
    Ok(ok(find(&mut *pool.acquire().await?, id).await?))
}

/// PUT, PATCH e DELETE su un movimento: sempre 405.
pub async fn immutable(Path(id): Path<Uuid>) -> Response {
    let mut res = ApiError::new(
        StatusCode::METHOD_NOT_ALLOWED,
        "MOVEMENT_IMMUTABLE",
        "I movimenti confermati non si modificano né si eliminano: usare POST /movements/{id}/reversal",
    )
    .with(json!({ "movement_id": id }))
    .into_response();
    res.headers_mut().insert(header::ALLOW, HeaderValue::from_static("GET"));
    res
}

#[derive(Deserialize)]
pub struct ReversalRequest {
    reason: String,
    #[serde(default)]
    note: Option<String>,
}

pub async fn reverse(
    State(pool): State<PgPool>,
    Operator(operator): Operator,
    Path(id): Path<Uuid>,
    Body(b): Body<ReversalRequest>,
) -> ApiResult<Json<OperationResult>> {
    if b.reason.trim().len() < 3 {
        return Err(ApiError::validation("reason deve avere almeno 3 caratteri"));
    }
    let mut tx = pool.begin().await?;
    let m = find(&mut tx, id).await?;
    inventory::lock_pairs(&mut tx, &[(m.product_id, m.warehouse_id)]).await?;
    // Rilettura dopo il lock: uno storno concorrente sarebbe ora visibile.
    let m = find(&mut tx, id).await?;
    if m.kind == "RESERVATION" || m.kind == "RELEASE" || m.reverses_movement_id.is_some() {
        return Err(ApiError::unprocessable(
            "MOVEMENT_NOT_REVERSIBLE",
            "Non si stornano gli storni né i movimenti di impegno (usare prenotazione e rilascio dell'ordine)",
        )
        .with(json!({ "movement_id": id, "type": m.kind })));
    }
    if let Some(by) = m.reversed_by_movement_id {
        return Err(
            ApiError::conflict("MOVEMENT_ALREADY_REVERSED", "Il movimento è già stato stornato")
                .with(json!({ "movement_id": id, "reversed_by_movement_id": by })),
        );
    }

    let p = inventory::product(&mut tx, m.product_id).await?;
    let serial_ids = inventory::movement_serial_ids(&mut tx, id).await?;
    let warehouse = inventory::warehouse(&mut tx, m.warehouse_id).await?;
    let operation_id = Uuid::new_v4();
    let mut new = NewMovement {
        operation_id,
        kind: "",
        product_id: m.product_id,
        lot_id: m.lot_id,
        warehouse_id: m.warehouse_id,
        quantity: m.quantity,
        reason_code: "REVERSAL".into(),
        note: Some(match &b.note {
            Some(n) if !n.trim().is_empty() => format!("{} — {}", b.reason.trim(), n.trim()),
            _ => b.reason.trim().to_string(),
        }),
        sales_order_id: m.sales_order_id,
        sales_order_line_id: m.sales_order_line_id,
        purchase_order_id: m.purchase_order_id,
        purchase_order_line_id: m.purchase_order_line_id,
        external_ref: m.external_ref.clone(),
        operator,
        reverses_movement_id: Some(id),
        serial_ids: serial_ids.clone(),
        ..Default::default()
    };

    // Toglie merce da un'ubicazione: i seriali devono essere ancora lì, e il disponibile
    // del magazzino non può diventare negativo per effetto dello storno.
    async fn take_back(
        tx: &mut sqlx::PgConnection,
        p: &crate::model::Product,
        warehouse: &inventory::WarehouseRef,
        m: &Movement,
        location: Uuid,
        serial_ids: &[Uuid],
        check_available: bool,
    ) -> ApiResult<()> {
        let still_there: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM serials WHERE id = ANY($1) AND status = 'IN_STOCK' AND location_id = $2",
        )
        .bind(serial_ids)
        .bind(location)
        .fetch_one(&mut *tx)
        .await?;
        if still_there != serial_ids.len() as i64 {
            return Err(ApiError::conflict(
                "SERIAL_NOT_AVAILABLE",
                "Alcuni seriali del movimento non sono più nell'ubicazione",
            ));
        }
        if check_available {
            let f = inventory::figures(tx, m.product_id, m.warehouse_id).await?;
            if m.quantity > f.available {
                return Err(inventory::insufficient_availability(
                    p,
                    warehouse,
                    m.quantity,
                    f.available,
                ));
            }
        }
        inventory::remove_stock(tx, location, m.product_id, m.lot_id, m.quantity).await?;
        Ok(())
    }

    match (m.kind.as_str(), m.direction.as_deref()) {
        ("INBOUND", _) => {
            let loc = m.to_location_id.expect("carico con destinazione");
            take_back(&mut tx, &p, &warehouse, &m, loc, &serial_ids, true).await?;
            inventory::set_serials(&mut tx, &serial_ids, "SCRAPPED", None).await?;
            new.kind = "OUTBOUND";
            new.from_location_id = Some(loc);
            if let (Some(po), Some(line)) = (m.purchase_order_id, m.purchase_order_line_id) {
                purchase_orders::unreceive_line(&mut tx, po, line, m.quantity).await?;
            }
        }
        ("OUTBOUND", _) => {
            let loc_id = m.from_location_id.expect("scarico con origine");
            let loc = inventory::location(&mut tx, loc_id, true).await?;
            inventory::check_destination(&mut tx, &p, &loc, m.quantity).await?;
            inventory::add_stock(
                &mut tx,
                m.warehouse_id,
                loc_id,
                m.product_id,
                m.lot_id,
                m.quantity,
                Utc::now(),
            )
            .await?;
            inventory::set_serials(&mut tx, &serial_ids, "IN_STOCK", Some(loc_id)).await?;
            new.kind = "INBOUND";
            new.to_location_id = Some(loc_id);
            if let (Some(so), Some(line)) = (m.sales_order_id, m.sales_order_line_id) {
                sales_orders::unfulfill_line(&mut tx, so, line, m.quantity).await?;
            }
        }
        ("TRANSFER", _) => {
            let (from, to) = (
                m.from_location_id.expect("origine"),
                m.to_location_id.expect("destinazione"),
            );
            take_back(&mut tx, &p, &warehouse, &m, to, &serial_ids, false).await?;
            let dest = inventory::location(&mut tx, from, true).await?;
            inventory::check_destination(&mut tx, &p, &dest, m.quantity).await?;
            inventory::add_stock(
                &mut tx,
                m.warehouse_id,
                from,
                m.product_id,
                m.lot_id,
                m.quantity,
                Utc::now(),
            )
            .await?;
            inventory::set_serials(&mut tx, &serial_ids, "IN_STOCK", Some(from)).await?;
            new.kind = "TRANSFER";
            new.from_location_id = Some(to);
            new.to_location_id = Some(from);
        }
        ("ADJUSTMENT", Some("IN")) => {
            let loc = m.to_location_id.expect("rettifica in aumento con ubicazione");
            take_back(&mut tx, &p, &warehouse, &m, loc, &serial_ids, true).await?;
            inventory::set_serials(&mut tx, &serial_ids, "SCRAPPED", None).await?;
            new.kind = "ADJUSTMENT";
            new.direction = Some("OUT");
            new.from_location_id = Some(loc);
        }
        ("ADJUSTMENT", _) => {
            let loc = m.from_location_id.expect("rettifica in diminuzione con ubicazione");
            inventory::add_stock(
                &mut tx,
                m.warehouse_id,
                loc,
                m.product_id,
                m.lot_id,
                m.quantity,
                Utc::now(),
            )
            .await?;
            inventory::set_serials(&mut tx, &serial_ids, "IN_STOCK", Some(loc)).await?;
            new.kind = "ADJUSTMENT";
            new.direction = Some("IN");
            new.to_location_id = Some(loc);
        }
        _ => return Err(ApiError::internal()),
    }
    let new_id = inventory::insert_movement(&mut tx, new).await?;
    let result = finish(&mut tx, operation_id, &[new_id], &[(m.product_id, m.warehouse_id)]).await?;
    tx.commit().await?;
    Ok(created(result))
}
