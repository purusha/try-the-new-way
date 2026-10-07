//! Enum del contratto e righe lette dal database.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::error::ApiError;

macro_rules! str_enum {
    ($name:ident { $($var:ident => $s:literal),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
        pub enum $name { $(#[serde(rename = $s)] $var),* }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$var => $s),* }
            }
        }
    };
}

str_enum!(StorageType { Ambient => "AMBIENT", Refrigerated => "REFRIGERATED", Frozen => "FROZEN", Hazardous => "HAZARDOUS", Bulk => "BULK" });
str_enum!(Uom { Pcs => "PCS", Kg => "KG", M => "M", L => "L" });
str_enum!(Tracking { None => "NONE", Lot => "LOT", Serial => "SERIAL" });
str_enum!(ProductStatus { Active => "ACTIVE", Inactive => "INACTIVE" });
str_enum!(LotStatus { Active => "ACTIVE", Blocked => "BLOCKED" });
str_enum!(SerialStatus { InStock => "IN_STOCK", Shipped => "SHIPPED", Scrapped => "SCRAPPED" });
str_enum!(MovementType {
    Inbound => "INBOUND", Outbound => "OUTBOUND", Transfer => "TRANSFER",
    Adjustment => "ADJUSTMENT", Reservation => "RESERVATION", Release => "RELEASE",
});
str_enum!(AdjustmentReason {
    Count => "COUNT", Damage => "DAMAGE", Loss => "LOSS", Found => "FOUND",
    ExpiredDisposal => "EXPIRED_DISPOSAL", Other => "OTHER",
});
str_enum!(SalesOrderStatus {
    Open => "OPEN", PartiallyReserved => "PARTIALLY_RESERVED", Reserved => "RESERVED",
    PartiallyFulfilled => "PARTIALLY_FULFILLED", Fulfilled => "FULFILLED", Cancelled => "CANCELLED",
});
str_enum!(PurchaseOrderStatus { Open => "OPEN", PartiallyReceived => "PARTIALLY_RECEIVED", Received => "RECEIVED", Cancelled => "CANCELLED" });
str_enum!(AlertType { LowStock => "LOW_STOCK", Expiring => "EXPIRING", Expired => "EXPIRED", ReservationAtRisk => "RESERVATION_AT_RISK" });

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PickMode {
    #[default]
    Auto,
    Manual,
}

/// Per i PATCH: distingue un campo assente (`None`) da un `null` esplicito (`Some(None)`).
pub fn nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// Quantità > 0 con al massimo 3 decimali (NUMERIC(18,3)).
pub fn check_quantity(q: Decimal, field: &str) -> Result<(), ApiError> {
    if q <= Decimal::ZERO {
        return Err(ApiError::validation(format!("{field} deve essere maggiore di zero")));
    }
    check_scale(q, field)
}

pub fn check_scale(q: Decimal, field: &str) -> Result<(), ApiError> {
    if q.normalize().scale() > 3 {
        return Err(ApiError::validation(format!("{field} ammette al massimo 3 decimali")));
    }
    if q.abs() >= Decimal::from(1_000_000_000_000_000i64) {
        return Err(ApiError::validation(format!("{field} fuori intervallo")));
    }
    Ok(())
}

pub fn non_empty(value: &str, field: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::validation(format!("{field} è obbligatorio")));
    }
    Ok(())
}

// ---------------------------------------------------------------- Righe

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Product {
    pub id: Uuid,
    pub sku: String,
    pub ean: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub uom: String,
    pub unit_price: Decimal,
    pub currency: String,
    pub min_stock: Decimal,
    pub tracking: String,
    pub requires_expiry: bool,
    pub expiry_warning_days: i32,
    pub unit_weight_kg: Decimal,
    pub unit_volume_m3: Decimal,
    pub required_storage_type: Option<String>,
    pub status: String,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Product {
    pub fn is_active(&self) -> bool {
        self.status == "ACTIVE"
    }
    pub fn tracks_lots(&self) -> bool {
        self.tracking == "LOT"
    }
    pub fn tracks_serials(&self) -> bool {
        self.tracking == "SERIAL"
    }
    /// Il lotto è obbligatorio per `LOT`, e per `SERIAL` quando serve la scadenza.
    pub fn lot_required(&self) -> bool {
        self.tracks_lots() || (self.tracks_serials() && self.requires_expiry)
    }
    pub fn lot_allowed(&self) -> bool {
        self.tracking != "NONE"
    }
}

#[macro_export]
macro_rules! product_select {
    () => {
        "SELECT p.id, p.sku, p.ean, p.name, p.description, p.category, p.uom, p.unit_price, p.currency,
                p.min_stock, p.tracking, p.requires_expiry, p.expiry_warning_days, p.unit_weight_kg,
                p.unit_volume_m3, p.required_storage_type, p.status, p.version, p.created_at, p.updated_at
         FROM products p "
    };
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Warehouse {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub address: Option<String>,
    pub active: bool,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Zone {
    pub id: Uuid,
    pub warehouse_id: Uuid,
    pub code: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Lot {
    pub id: Uuid,
    pub product_id: Uuid,
    pub sku: String,
    pub lot_code: String,
    pub production_date: Option<NaiveDate>,
    pub expiry_date: Option<NaiveDate>,
    pub received_at: DateTime<Utc>,
    pub status: String,
    pub expired: bool,
    pub days_to_expiry: Option<i32>,
    pub on_hand: Decimal,
    pub created_at: DateTime<Utc>,
}

impl Lot {
    pub fn is_blocked(&self) -> bool {
        self.status == "BLOCKED"
    }
}

#[macro_export]
macro_rules! lot_select {
    () => {
        "SELECT l.id, l.product_id, p.sku, l.lot_code, l.production_date, l.expiry_date, l.received_at, l.status,
                (l.expiry_date IS NOT NULL AND l.expiry_date < CURRENT_DATE) AS expired,
                (l.expiry_date - CURRENT_DATE) AS days_to_expiry,
                COALESCE((SELECT sum(s.quantity) FROM stock s WHERE s.lot_id = l.id), 0) AS on_hand,
                l.created_at
         FROM lots l JOIN products p ON p.id = l.product_id "
    };
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Serial {
    pub id: Uuid,
    pub product_id: Uuid,
    pub sku: String,
    pub serial_number: String,
    pub lot_id: Option<Uuid>,
    pub lot_code: Option<String>,
    pub location_id: Option<Uuid>,
    pub location_code: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[macro_export]
macro_rules! serial_select {
    () => {
        "SELECT sr.id, sr.product_id, p.sku, sr.serial_number, sr.lot_id, l.lot_code, sr.location_id,
                loc.code AS location_code, sr.status, sr.created_at
         FROM serials sr
         JOIN products p ON p.id = sr.product_id
         LEFT JOIN lots l ON l.id = sr.lot_id
         LEFT JOIN locations loc ON loc.id = sr.location_id "
    };
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StockItem {
    pub id: Uuid,
    pub warehouse_id: Uuid,
    pub location_id: Uuid,
    pub location_code: String,
    pub zone_id: Uuid,
    pub product_id: Uuid,
    pub sku: String,
    pub product_name: String,
    pub uom: String,
    pub lot_id: Option<Uuid>,
    pub lot_code: Option<String>,
    pub expiry_date: Option<NaiveDate>,
    pub lot_status: Option<String>,
    pub quantity: Decimal,
    pub received_at: DateTime<Utc>,
    pub usable: bool,
    pub serials: Vec<String>,
}

#[macro_export]
macro_rules! stock_select {
    () => {
        "SELECT s.id, s.warehouse_id, s.location_id, loc.code AS location_code, loc.zone_id, s.product_id,
                p.sku, p.name AS product_name, p.uom, s.lot_id, l.lot_code, l.expiry_date, l.status AS lot_status,
                s.quantity, s.received_at,
                NOT (l.id IS NOT NULL AND (l.status = 'BLOCKED' OR l.expiry_date < CURRENT_DATE)) AS usable,
                COALESCE((SELECT array_agg(sr.serial_number ORDER BY sr.serial_number) FROM serials sr
                          WHERE sr.product_id = s.product_id AND sr.location_id = s.location_id
                            AND sr.lot_id IS NOT DISTINCT FROM s.lot_id AND sr.status = 'IN_STOCK'),
                         '{}') AS serials
         FROM stock s
         JOIN locations loc ON loc.id = s.location_id
         JOIN products p ON p.id = s.product_id
         LEFT JOIN lots l ON l.id = s.lot_id "
    };
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Movement {
    pub id: Uuid,
    pub number: i64,
    pub operation_id: Uuid,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub kind: String,
    pub direction: Option<String>,
    pub product_id: Uuid,
    pub sku: String,
    pub lot_id: Option<Uuid>,
    pub lot_code: Option<String>,
    pub warehouse_id: Uuid,
    pub from_location_id: Option<Uuid>,
    pub from_location_code: Option<String>,
    pub to_location_id: Option<Uuid>,
    pub to_location_code: Option<String>,
    pub quantity: Decimal,
    pub serials: Vec<String>,
    pub reason_code: String,
    pub note: Option<String>,
    pub sales_order_id: Option<Uuid>,
    pub sales_order_line_id: Option<Uuid>,
    pub sales_order_number: Option<String>,
    pub purchase_order_id: Option<Uuid>,
    pub purchase_order_line_id: Option<Uuid>,
    pub purchase_order_number: Option<String>,
    pub external_ref: Option<String>,
    pub operator: String,
    pub occurred_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub reverses_movement_id: Option<Uuid>,
    pub reversed_by_movement_id: Option<Uuid>,
    pub fefo_override_reason: Option<String>,
}

#[macro_export]
macro_rules! movement_select {
    () => {
        "SELECT m.id, m.number, m.operation_id, m.type, m.direction, m.product_id, p.sku, m.lot_id, lt.lot_code,
                m.warehouse_id, m.from_location_id, fl.code AS from_location_code, m.to_location_id,
                tl.code AS to_location_code, m.quantity,
                COALESCE((SELECT array_agg(sr.serial_number ORDER BY sr.serial_number)
                          FROM movement_serials ms JOIN serials sr ON sr.id = ms.serial_id
                          WHERE ms.movement_id = m.id), '{}') AS serials,
                m.reason_code, m.note, m.sales_order_id, m.sales_order_line_id, so.number AS sales_order_number,
                m.purchase_order_id, m.purchase_order_line_id, po.number AS purchase_order_number,
                m.external_ref, m.operator, m.occurred_at, m.created_at, m.reverses_movement_id,
                (SELECT r.id FROM movements r WHERE r.reverses_movement_id = m.id) AS reversed_by_movement_id,
                m.fefo_override_reason
         FROM movements m
         JOIN products p ON p.id = m.product_id
         LEFT JOIN lots lt ON lt.id = m.lot_id
         LEFT JOIN locations fl ON fl.id = m.from_location_id
         LEFT JOIN locations tl ON tl.id = m.to_location_id
         LEFT JOIN sales_orders so ON so.id = m.sales_order_id
         LEFT JOIN purchase_orders po ON po.id = m.purchase_order_id "
    };
}
