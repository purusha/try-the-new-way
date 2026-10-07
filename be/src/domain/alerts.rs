//! Alert di sotto-scorta, scadenza e impegni a rischio (spec 007, regola 3).

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Alert {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub severity: &'static str,
    pub warehouse_id: Option<Uuid>,
    pub warehouse_code: Option<String>,
    pub product_id: Uuid,
    pub sku: String,
    pub lot_id: Option<Uuid>,
    pub lot_code: Option<String>,
    pub expiry_date: Option<NaiveDate>,
    pub quantity: Option<Decimal>,
    pub threshold: Option<Decimal>,
    pub message: String,
}

/// Situazione di un prodotto in un magazzino, base per gli alert di quantità.
#[derive(Debug, Clone)]
pub struct Figures {
    pub warehouse_id: Uuid,
    pub warehouse_code: String,
    pub product_id: Uuid,
    pub sku: String,
    pub min_stock: Decimal,
    pub available: Decimal,
}

pub fn quantity_alerts(f: &Figures) -> Vec<Alert> {
    let base = |kind, severity, threshold, message| Alert {
        kind,
        severity,
        warehouse_id: Some(f.warehouse_id),
        warehouse_code: Some(f.warehouse_code.clone()),
        product_id: f.product_id,
        sku: f.sku.clone(),
        lot_id: None,
        lot_code: None,
        expiry_date: None,
        quantity: Some(f.available),
        threshold,
        message,
    };
    let mut out = Vec::new();
    if f.available < Decimal::ZERO {
        out.push(base(
            "RESERVATION_AT_RISK",
            "CRITICAL",
            None,
            format!(
                "{} in {}: gli impegni superano le giacenze utilizzabili di {}",
                f.sku,
                f.warehouse_code,
                (-f.available).normalize()
            ),
        ));
    }
    if f.min_stock > Decimal::ZERO && f.available < f.min_stock {
        out.push(base(
            "LOW_STOCK",
            "WARNING",
            Some(f.min_stock),
            format!(
                "{} in {}: disponibile {} sotto la scorta minima {}",
                f.sku,
                f.warehouse_code,
                f.available.normalize(),
                f.min_stock.normalize()
            ),
        ));
    }
    out
}

/// Lotto con giacenza in un magazzino, base per gli alert di scadenza.
#[derive(Debug, Clone)]
pub struct LotOnHand {
    pub warehouse_id: Uuid,
    pub warehouse_code: String,
    pub product_id: Uuid,
    pub sku: String,
    pub lot_id: Uuid,
    pub lot_code: String,
    pub expiry_date: NaiveDate,
    pub warning_days: i32,
    pub quantity: Decimal,
}

pub fn expiry_alert(l: &LotOnHand, today: NaiveDate) -> Option<Alert> {
    let days = (l.expiry_date - today).num_days();
    let (kind, severity, message) = if days < 0 {
        (
            "EXPIRED",
            "CRITICAL",
            format!("Lotto {} di {} scaduto il {}", l.lot_code, l.sku, l.expiry_date),
        )
    } else if days <= i64::from(l.warning_days) {
        (
            "EXPIRING",
            "WARNING",
            format!(
                "Lotto {} di {} scade il {} (tra {days} giorni)",
                l.lot_code, l.sku, l.expiry_date
            ),
        )
    } else {
        return None;
    };
    Some(Alert {
        kind,
        severity,
        warehouse_id: Some(l.warehouse_id),
        warehouse_code: Some(l.warehouse_code.clone()),
        product_id: l.product_id,
        sku: l.sku.clone(),
        lot_id: Some(l.lot_id),
        lot_code: Some(l.lot_code.clone()),
        expiry_date: Some(l.expiry_date),
        quantity: Some(l.quantity),
        threshold: None,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn figures(min: Decimal, available: Decimal) -> Figures {
        Figures {
            warehouse_id: Uuid::nil(),
            warehouse_code: "MI01".into(),
            product_id: Uuid::nil(),
            sku: "SKU".into(),
            min_stock: min,
            available,
        }
    }

    #[test]
    fn low_stock_only_when_threshold_set() {
        assert!(quantity_alerts(&figures(dec!(0), dec!(0))).is_empty());
        assert!(quantity_alerts(&figures(dec!(10), dec!(10))).is_empty());
        let a = quantity_alerts(&figures(dec!(10), dec!(9)));
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].kind, "LOW_STOCK");
    }

    #[test]
    fn negative_available_is_reservation_at_risk() {
        let kinds: Vec<_> = quantity_alerts(&figures(dec!(5), dec!(-2)))
            .iter()
            .map(|a| a.kind)
            .collect();
        assert_eq!(kinds, vec!["RESERVATION_AT_RISK", "LOW_STOCK"]);
    }

    #[test]
    fn expiry_windows() {
        let today: NaiveDate = "2026-10-07".parse().unwrap();
        let lot = |d: &str| LotOnHand {
            warehouse_id: Uuid::nil(),
            warehouse_code: "MI01".into(),
            product_id: Uuid::nil(),
            sku: "SKU".into(),
            lot_id: Uuid::nil(),
            lot_code: "L".into(),
            expiry_date: d.parse().unwrap(),
            warning_days: 30,
            quantity: dec!(1),
        };
        assert_eq!(expiry_alert(&lot("2026-10-06"), today).unwrap().kind, "EXPIRED");
        assert_eq!(expiry_alert(&lot("2026-10-07"), today).unwrap().kind, "EXPIRING");
        assert_eq!(expiry_alert(&lot("2026-11-06"), today).unwrap().kind, "EXPIRING");
        assert!(expiry_alert(&lot("2026-11-07"), today).is_none());
    }
}
