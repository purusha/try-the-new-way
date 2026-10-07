//! Test di integrazione delle regole di business (spec 007) contro Postgres.
//! Ogni test riceve un database nuovo con le migrazioni applicate (`DATABASE_URL` da be/.env).

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Method, Request, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

struct Api {
    app: Router,
}

struct Res {
    status: StatusCode,
    body: Value,
    headers: axum::http::HeaderMap,
}

impl Res {
    fn code(&self) -> &str {
        self.body["code"].as_str().unwrap_or_default()
    }
}

impl Api {
    fn new(pool: PgPool) -> Self {
        Self { app: be::app(pool) }
    }

    async fn send(&self, method: Method, path: &str, body: Option<Value>, extra: &[(&str, &str)]) -> Res {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"))
            .header("x-operator", "tester");
        for (k, v) in extra {
            req = req.header(*k, *v);
        }
        let req = match body {
            Some(b) => req
                .header("content-type", "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        Res { status, body, headers }
    }

    async fn get(&self, path: &str) -> Res {
        self.send(Method::GET, path, None, &[]).await
    }

    async fn post(&self, path: &str, body: Value) -> Res {
        self.send(Method::POST, path, Some(body), &[]).await
    }

    /// POST che deve riuscire: restituisce il corpo.
    async fn ok(&self, path: &str, body: Value) -> Value {
        let r = self.post(path, body).await;
        assert!(r.status.is_success(), "POST {path} -> {} {}", r.status, r.body);
        r.body
    }
}

fn id(v: &Value) -> String {
    v["id"].as_str().expect("id").to_string()
}

/// Magazzino con una zona e due ubicazioni refrigerate (A-01 max 1000 kg, A-02 max 100 kg),
/// e un prodotto a lotto con scadenza (1 kg/pezzo).
struct Fixture {
    wh: String,
    a01: String,
    a02: String,
    product: String,
}

async fn fixture(api: &Api) -> Fixture {
    let wh = id(&api.ok("/warehouses", json!({ "code": "MI01", "name": "Milano" })).await);
    let zone = id(&api
        .ok(
            &format!("/warehouses/{wh}/zones"),
            json!({ "code": "A", "name": "Freddo" }),
        )
        .await);
    let loc = |code: &str, max: u32| json!({ "zone_id": zone, "code": code, "storage_type": "REFRIGERATED", "max_weight_kg": max });
    let a01 = id(&api.ok(&format!("/warehouses/{wh}/locations"), loc("A-01", 1000)).await);
    let a02 = id(&api.ok(&format!("/warehouses/{wh}/locations"), loc("A-02", 100)).await);
    let product = id(&api
        .ok(
            "/products",
            json!({ "sku": "LATTE", "name": "Latte", "uom": "PCS", "tracking": "LOT", "requires_expiry": true,
                    "min_stock": 10, "unit_weight_kg": 1, "required_storage_type": "REFRIGERATED" }),
        )
        .await);
    Fixture { wh, a01, a02, product }
}

async fn receive(api: &Api, f: &Fixture, location: &str, lot: &str, expiry: &str, qty: u32) -> Value {
    api.ok(
        "/operations/receipts",
        json!({ "warehouse_id": f.wh, "lines": [{ "product_id": f.product, "quantity": qty, "location_id": location,
                 "lot": { "lot_code": lot, "expiry_date": expiry } }] }),
    )
    .await
}

async fn availability(api: &Api, f: &Fixture) -> Value {
    api.get(&format!("/products/{}/availability", f.product)).await.body["totals"].clone()
}

async fn sales_order(api: &Api, f: &Fixture, qty: u32) -> Value {
    api.ok("/sales-orders", json!({ "customer_name": "Cliente", "warehouse_id": f.wh, "lines": [{ "product_id": f.product, "quantity": qty }] }))
        .await
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn reservation_reduces_available_and_cannot_exceed_it(pool: PgPool) {
    let api = Api::new(pool);
    let f = fixture(&api).await;
    receive(&api, &f, &f.a01, "L1", "2099-12-31", 100).await;

    let so = sales_order(&api, &f, 70).await;
    let r = api.ok(&format!("/sales-orders/{}/reserve", id(&so)), json!({})).await;
    assert_eq!(r["status"], "RESERVED");
    assert_eq!(r["movements"][0]["type"], "RESERVATION");
    let t = availability(&api, &f).await;
    assert_eq!(
        (t["physical"].as_i64(), t["reserved"].as_i64(), t["available"].as_i64()),
        (Some(100), Some(70), Some(30))
    );

    // Regola 1: né un altro impegno né uno scarico libero possono superare il disponibile (30).
    let so2 = sales_order(&api, &f, 31).await;
    let r = api
        .post(&format!("/sales-orders/{}/reserve", id(&so2)), json!({}))
        .await;
    assert_eq!(
        (r.status, r.code()),
        (StatusCode::CONFLICT, "INSUFFICIENT_AVAILABILITY")
    );
    assert_eq!(r.body["details"]["available"], 30);
    let r = api
        .post(
            "/operations/shipments",
            json!({ "warehouse_id": f.wh, "lines": [{ "product_id": f.product, "quantity": 31 }] }),
        )
        .await;
    assert_eq!(r.code(), "INSUFFICIENT_AVAILABILITY");

    // Con allow_partial si impegna quanto disponibile.
    let r = api
        .ok(
            &format!("/sales-orders/{}/reserve", id(&so2)),
            json!({ "allow_partial": true }),
        )
        .await;
    assert_eq!(r["status"], "PARTIALLY_RESERVED");
    assert_eq!(r["lines"][0]["quantity_reserved"], 30);

    // Rilascio e annullamento restituiscono disponibilità.
    api.ok(&format!("/sales-orders/{}/cancel", id(&so2)), json!({})).await;
    assert_eq!(availability(&api, &f).await["available"], 30);
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn fulfillment_follows_fefo_and_converts_reservation(pool: PgPool) {
    let api = Api::new(pool);
    let f = fixture(&api).await;
    receive(&api, &f, &f.a01, "LATE", "2099-12-31", 50).await;
    receive(&api, &f, &f.a02, "EARLY", "2099-06-30", 20).await;

    let so = sales_order(&api, &f, 30).await;
    let so_id = id(&so);
    api.ok(&format!("/sales-orders/{so_id}/reserve"), json!({})).await;
    let s = api
        .ok(
            "/operations/picking-suggestions",
            json!({ "warehouse_id": f.wh, "sales_order_id": so_id }),
        )
        .await;
    assert_eq!(s["lines"][0]["strategy_applied"], "FEFO");
    assert_eq!(s["lines"][0]["picks"][0]["lot_code"], "EARLY");

    let r = api
        .ok(&format!("/sales-orders/{so_id}/fulfill"), json!({ "mode": "AUTO" }))
        .await;
    assert_eq!(r["status"], "FULFILLED");
    let picks: Vec<(String, i64)> = r["movements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["lot_code"].as_str().unwrap().to_string(),
                m["quantity"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(picks, vec![("EARLY".to_string(), 20), ("LATE".to_string(), 10)]);
    assert_eq!(r["lines"][0]["quantity_reserved"], 0);
    assert_eq!(r["lines"][0]["quantity_fulfilled"], 30);
    let t = availability(&api, &f).await;
    assert_eq!((t["physical"].as_i64(), t["reserved"].as_i64()), (Some(40), Some(0)));

    // Si evade solo l'impegnato.
    let r = api.post(&format!("/sales-orders/{so_id}/fulfill"), json!({})).await;
    assert_eq!(r.code(), "ORDER_STATE_INVALID");
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn manual_pick_violating_fefo_requires_override(pool: PgPool) {
    let api = Api::new(pool);
    let f = fixture(&api).await;
    let late = receive(&api, &f, &f.a01, "LATE", "2099-12-31", 50).await;
    receive(&api, &f, &f.a02, "EARLY", "2099-06-30", 20).await;
    let late_lot = late["movements"][0]["lot_id"].as_str().unwrap().to_string();

    let body = |o: Value| {
        json!({ "warehouse_id": f.wh, "mode": "MANUAL", "fefo_override": o,
                "lines": [{ "product_id": f.product, "picks": [{ "location_id": f.a01, "lot_id": late_lot, "quantity": 5 }] }] })
    };
    let r = api.post("/operations/shipments", body(Value::Null)).await;
    assert_eq!((r.status, r.code()), (StatusCode::CONFLICT, "FEFO_VIOLATION"));
    assert_eq!(r.body["details"]["earlier_lots"][0]["lot_code"], "EARLY");
    assert_eq!(r.body["details"]["earlier_lots"][0]["quantity"], 20);

    let r = api
        .ok(
            "/operations/shipments",
            body(json!({ "reason": "Richiesta del cliente" })),
        )
        .await;
    assert_eq!(r["movements"][0]["fefo_override_reason"], "Richiesta del cliente");

    // Nessuna violazione se la stessa richiesta esaurisce anche il lotto anteriore.
    let early_lot = api
        .get(&format!("/lots?lot_code=EARLY&product_id={}", f.product))
        .await
        .body["items"][0]["id"]
        .clone();
    let r = api
        .post(
            "/operations/shipments",
            json!({ "warehouse_id": f.wh, "mode": "MANUAL", "lines": [{ "product_id": f.product, "picks": [
                { "location_id": f.a02, "lot_id": early_lot, "quantity": 20 },
                { "location_id": f.a01, "lot_id": late_lot, "quantity": 1 } ] }] }),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.body);
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn expired_lots_are_unusable(pool: PgPool) {
    let api = Api::new(pool);
    let f = fixture(&api).await;
    let old = receive(&api, &f, &f.a01, "OLD", "2020-01-01", 10).await;
    assert!(old["alerts"].as_array().unwrap().iter().any(|a| a["type"] == "EXPIRED"));
    receive(&api, &f, &f.a01, "NEW", "2099-01-01", 5).await;
    let t = availability(&api, &f).await;
    assert_eq!(
        (t["physical"].as_i64(), t["unusable"].as_i64(), t["available"].as_i64()),
        (Some(15), Some(10), Some(5))
    );

    let old_lot = old["movements"][0]["lot_id"].clone();
    let r = api
        .post(
            "/operations/shipments",
            json!({ "warehouse_id": f.wh, "mode": "MANUAL",
                    "lines": [{ "product_id": f.product, "picks": [{ "location_id": f.a01, "lot_id": old_lot, "quantity": 1 }] }] }),
        )
        .await;
    assert_eq!(r.code(), "LOT_EXPIRED");

    // Lo smaltimento con rettifica è ammesso.
    let r = api
        .ok(
            "/operations/adjustments",
            json!({ "location_id": f.a01, "product_id": f.product, "lot_id": old_lot, "counted_quantity": 0, "reason_code": "EXPIRED_DISPOSAL" }),
        )
        .await;
    assert_eq!(
        (
            r["movements"][0]["direction"].as_str(),
            r["movements"][0]["quantity"].as_i64()
        ),
        (Some("OUT"), Some(10))
    );
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn location_capacity_blocks_receipts_and_transfers(pool: PgPool) {
    let api = Api::new(pool);
    let f = fixture(&api).await;
    receive(&api, &f, &f.a02, "L1", "2099-01-01", 90).await;
    let r = api
        .post(
            "/operations/receipts",
            json!({ "warehouse_id": f.wh, "lines": [{ "product_id": f.product, "quantity": 11, "location_id": f.a02,
                     "lot": { "lot_code": "L2", "expiry_date": "2099-01-01" } }] }),
        )
        .await;
    assert_eq!(
        (r.status, r.code()),
        (StatusCode::CONFLICT, "LOCATION_CAPACITY_EXCEEDED")
    );
    assert_eq!(r.body["details"]["dimension"], "WEIGHT");
    assert_eq!(r.body["details"]["max"], 100);

    let lot = receive(&api, &f, &f.a01, "L3", "2099-01-01", 20).await["movements"][0]["lot_id"].clone();
    let r = api
        .post(
            "/operations/transfers",
            json!({ "lines": [{ "product_id": f.product, "lot_id": lot, "from_location_id": f.a01, "to_location_id": f.a02, "quantity": 11 }] }),
        )
        .await;
    assert_eq!(r.code(), "LOCATION_CAPACITY_EXCEEDED");
    // Un'operazione rifiutata non lascia tracce: la giacenza di origine è intatta.
    let stock = api.get(&format!("/stock?location_id={}", f.a01)).await;
    assert_eq!(stock.body["items"][0]["quantity"], 20);

    let loc = api
        .get(&format!("/warehouses/{}/locations?has_capacity=false", f.wh))
        .await;
    assert_eq!(loc.body["total"], 0);
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn movements_are_immutable_and_reversed_once(pool: PgPool) {
    let api = Api::new(pool.clone());
    let f = fixture(&api).await;
    let r = receive(&api, &f, &f.a01, "L1", "2099-01-01", 10).await;
    let mid = r["movements"][0]["id"].as_str().unwrap().to_string();

    for m in [Method::PUT, Method::PATCH, Method::DELETE] {
        let r = api.send(m, &format!("/movements/{mid}"), Some(json!({})), &[]).await;
        assert_eq!(
            (r.status, r.code()),
            (StatusCode::METHOD_NOT_ALLOWED, "MOVEMENT_IMMUTABLE")
        );
    }
    // Anche il database rifiuta modifiche ed eliminazioni.
    assert!(
        sqlx::query("UPDATE movements SET quantity = 1")
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(sqlx::query("DELETE FROM movements").execute(&pool).await.is_err());

    let r = api
        .ok(
            &format!("/movements/{mid}/reversal"),
            json!({ "reason": "Carico registrato per errore" }),
        )
        .await;
    assert_eq!(r["movements"][0]["type"], "OUTBOUND");
    assert_eq!(r["movements"][0]["reverses_movement_id"], mid.as_str());
    assert_eq!(availability(&api, &f).await["physical"], 0);
    let r = api
        .post(&format!("/movements/{mid}/reversal"), json!({ "reason": "di nuovo" }))
        .await;
    assert_eq!(r.code(), "MOVEMENT_ALREADY_REVERSED");
    let original = api.get(&format!("/movements/{mid}")).await;
    assert!(original.body["reversed_by_movement_id"].is_string());
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn concurrent_reservations_never_oversell(pool: PgPool) {
    let api = Api::new(pool);
    let f = fixture(&api).await;
    receive(&api, &f, &f.a01, "L1", "2099-01-01", 100).await;
    let mut orders = Vec::new();
    for _ in 0..5 {
        orders.push(id(&sales_order(&api, &f, 30).await));
    }
    let paths: Vec<String> = orders.iter().map(|o| format!("/sales-orders/{o}/reserve")).collect();
    let results = futures::future::join_all(paths.iter().map(|p| api.post(p, json!({})))).await;
    let ok = results.iter().filter(|r| r.status == StatusCode::OK).count();
    assert_eq!(
        ok,
        3,
        "{:?}",
        results.iter().map(|r| r.code().to_string()).collect::<Vec<_>>()
    );
    assert!(
        results
            .iter()
            .filter(|r| r.status != StatusCode::OK)
            .all(|r| r.code() == "INSUFFICIENT_AVAILABILITY")
    );
    assert_eq!(availability(&api, &f).await["reserved"], 90);
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn serial_products_track_each_unit(pool: PgPool) {
    let api = Api::new(pool);
    let f = fixture(&api).await;
    let p = id(&api
        .ok(
            "/products",
            json!({ "sku": "TV", "name": "Televisore", "uom": "PCS", "tracking": "SERIAL" }),
        )
        .await);
    let r = api
        .post(
            "/operations/receipts",
            json!({ "warehouse_id": f.wh, "lines": [{ "product_id": p, "quantity": 3, "location_id": f.a01, "serials": ["SN1", "SN2"] }] }),
        )
        .await;
    assert_eq!(r.code(), "SERIAL_COUNT_MISMATCH");
    api.ok(
        "/operations/receipts",
        json!({ "warehouse_id": f.wh, "lines": [{ "product_id": p, "quantity": 3, "location_id": f.a01, "serials": ["SN1", "SN2", "SN3"] }] }),
    )
    .await;
    let r = api
        .post(
            "/operations/receipts",
            json!({ "warehouse_id": f.wh, "lines": [{ "product_id": p, "quantity": 1, "location_id": f.a01, "serials": ["SN2"] }] }),
        )
        .await;
    assert_eq!(r.code(), "SERIAL_ALREADY_EXISTS");

    let r = api
        .ok(
            "/operations/shipments",
            json!({ "warehouse_id": f.wh, "lines": [{ "product_id": p, "quantity": 2 }] }),
        )
        .await;
    assert_eq!(r["movements"][0]["serials"], json!(["SN1", "SN2"]));
    let s = api.get("/serials?serial_number=SN1").await.body["items"][0].clone();
    assert_eq!(s["status"], "SHIPPED");
    let t = api.get(&format!("/serials/{}/trace", id(&s))).await;
    assert_eq!(t.body["movements"].as_array().unwrap().len(), 2);
    assert!(t.body["shipment"].is_object());
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn idempotency_key_replays_and_rejects_reuse(pool: PgPool) {
    let api = Api::new(pool);
    let f = fixture(&api).await;
    let body = json!({ "warehouse_id": f.wh, "lines": [{ "product_id": f.product, "quantity": 5, "location_id": f.a01,
                       "lot": { "lot_code": "L1", "expiry_date": "2099-01-01" } }] });
    let key = [("idempotency-key", "k-1")];
    let first = api
        .send(Method::POST, "/operations/receipts", Some(body.clone()), &key)
        .await;
    let again = api
        .send(Method::POST, "/operations/receipts", Some(body.clone()), &key)
        .await;
    assert_eq!(first.status, StatusCode::CREATED);
    assert_eq!(again.status, StatusCode::CREATED);
    assert_eq!(again.headers.get("idempotent-replayed").unwrap(), "true");
    assert_eq!(first.body["operation_id"], again.body["operation_id"]);
    assert_eq!(availability(&api, &f).await["physical"], 5);

    let mut other = body.clone();
    other["lines"][0]["quantity"] = json!(6);
    let r = api.send(Method::POST, "/operations/receipts", Some(other), &key).await;
    assert_eq!(r.code(), "IDEMPOTENCY_KEY_REUSED");
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn validation_and_protocol_errors(pool: PgPool) {
    let api = Api::new(pool);
    let f = fixture(&api).await;
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/products")
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let res = api.app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let r = api
        .post("/products", json!({ "sku": "LATTE", "name": "Doppio", "uom": "PCS" }))
        .await;
    assert_eq!(r.code(), "DUPLICATE_SKU");
    let r = api
        .post("/products", json!({ "sku": "X", "name": "X", "uom": "BOX" }))
        .await;
    assert_eq!(
        (r.status, r.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "VALIDATION_ERROR")
    );
    let r = api
        .post("/operations/receipts", json!({ "warehouse_id": f.wh, "lines": [{ "product_id": f.product, "quantity": 1, "location_id": f.a01 }] }))
        .await;
    assert_eq!(r.code(), "LOT_REQUIRED");
    let r = api
        .post(
            "/operations/receipts",
            json!({ "warehouse_id": f.wh, "lines": [{ "product_id": f.product, "quantity": 1, "location_id": f.a01, "lot": { "lot_code": "NOEXP" } }] }),
        )
        .await;
    assert_eq!(r.code(), "EXPIRY_DATE_REQUIRED");
    let r = api
        .post(
            "/operations/adjustments",
            json!({ "location_id": f.a01, "product_id": f.product, "counted_quantity": 1, "reason_code": "OTHER" }),
        )
        .await;
    assert_eq!(r.code(), "ADJUSTMENT_REASON_REQUIRED");

    // ETag / If-Match.
    let p = api.get(&format!("/products/{}", f.product)).await;
    let etag = p.headers.get("etag").unwrap().to_str().unwrap().to_string();
    let ok = api
        .send(
            Method::PATCH,
            &format!("/products/{}", f.product),
            Some(json!({ "name": "Latte intero" })),
            &[("if-match", &etag)],
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK);
    let stale = api
        .send(
            Method::PATCH,
            &format!("/products/{}", f.product),
            Some(json!({ "name": "X" })),
            &[("if-match", &etag)],
        )
        .await;
    assert_eq!(
        (stale.status, stale.code()),
        (StatusCode::PRECONDITION_FAILED, "VERSION_CONFLICT")
    );

    // Archiviazione bloccata con giacenza.
    receive(&api, &f, &f.a01, "L1", "2099-01-01", 1).await;
    let r = api.post(&format!("/products/{}/archive", f.product), json!({})).await;
    assert_eq!(r.code(), "PRODUCT_HAS_STOCK");
}

#[sqlx::test(migrator = "be::MIGRATOR")]
async fn low_stock_alert_and_product_filters(pool: PgPool) {
    let api = Api::new(pool);
    let f = fixture(&api).await;
    let r = receive(&api, &f, &f.a01, "L1", "2099-01-01", 8).await;
    assert!(r["alerts"].as_array().unwrap().iter().any(|a| a["type"] == "LOW_STOCK"));
    let list = api.get("/products?below_min_stock=true").await;
    assert_eq!(list.body["total"], 1);
    assert_eq!(list.body["items"][0]["stock"]["below_min_stock"], true);
    let expiring = api.get("/products?expiring_within_days=30").await;
    assert_eq!(expiring.body["total"], 0);
    let alerts = api.get(&format!("/alerts?warehouse_id={}&type=LOW_STOCK", f.wh)).await;
    assert_eq!(alerts.body["items"].as_array().unwrap().len(), 1);
}
