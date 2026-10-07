//! Allocazione dei prelievi FEFO/FIFO e verifica della regola FEFO (spec 007, regola 2).

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Strategy {
    #[default]
    Auto,
    Fefo,
    Fifo,
}

impl Strategy {
    /// `AUTO` diventa FEFO se il prodotto ha scadenze (richieste o presenti nei lotti), altrimenti FIFO.
    pub fn resolve(self, requires_expiry: bool, candidates: &[Candidate]) -> Strategy {
        match self {
            Strategy::Auto if requires_expiry || candidates.iter().any(|c| c.expiry_date.is_some()) => Strategy::Fefo,
            Strategy::Auto => Strategy::Fifo,
            other => other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Strategy::Auto => "AUTO",
            Strategy::Fefo => "FEFO",
            Strategy::Fifo => "FIFO",
        }
    }
}

/// Una riga di giacenza da cui si può prelevare.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub location_id: Uuid,
    pub location_code: String,
    pub lot_id: Option<Uuid>,
    pub lot_code: Option<String>,
    pub expiry_date: Option<NaiveDate>,
    pub received_at: DateTime<Utc>,
    pub quantity: Decimal,
    /// Falso per lotti scaduti o bloccati: non si prelevano mai in automatico.
    pub usable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Allocation {
    pub candidate: usize,
    pub quantity: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AllocationResult {
    pub picks: Vec<Allocation>,
    pub allocated: Decimal,
    pub shortfall: Decimal,
}

/// Ordine di prelievo: FEFO per scadenza crescente (senza scadenza in fondo), FIFO per data di
/// ricevimento. A parità, ricevimento più vecchio e poi codice ubicazione, per un risultato stabile.
pub fn order(candidates: &[Candidate], strategy: Strategy) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..candidates.len()).filter(|&i| candidates[i].usable).collect();
    idx.sort_by(|&a, &b| {
        let (ca, cb) = (&candidates[a], &candidates[b]);
        let by_expiry = match (ca.expiry_date, cb.expiry_date) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        };
        let primary = if strategy == Strategy::Fifo {
            std::cmp::Ordering::Equal
        } else {
            by_expiry
        };
        primary
            .then(ca.received_at.cmp(&cb.received_at))
            .then(ca.location_code.cmp(&cb.location_code))
    });
    idx
}

/// Alloca `quantity` sulle giacenze utilizzabili. Se `whole_units` (prodotti a seriale) si prelevano
/// solo quantità intere.
pub fn allocate(
    candidates: &[Candidate],
    quantity: Decimal,
    strategy: Strategy,
    whole_units: bool,
) -> AllocationResult {
    let mut remaining = quantity;
    let mut picks = Vec::new();
    for i in order(candidates, strategy) {
        if remaining <= Decimal::ZERO {
            break;
        }
        let mut available = candidates[i].quantity;
        if whole_units {
            available = available.floor();
        }
        let take = available.min(remaining);
        if take > Decimal::ZERO {
            picks.push(Allocation {
                candidate: i,
                quantity: take,
            });
            remaining -= take;
        }
    }
    AllocationResult {
        picks,
        allocated: quantity - remaining,
        shortfall: remaining,
    }
}

/// Lotto a scadenza anteriore che un prelievo manuale sta saltando.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EarlierLot {
    pub lot_id: Uuid,
    pub lot_code: String,
    pub expiry_date: NaiveDate,
    /// Quantità del lotto che resterebbe in giacenza dopo i prelievi della richiesta.
    pub quantity: Decimal,
}

/// Verifica FEFO per un prelievo manuale del lotto con scadenza `picked_expiry`: restituisce i lotti
/// utilizzabili dello stesso prodotto e magazzino con scadenza anteriore che resterebbero in
/// giacenza dopo i prelievi della stessa richiesta (`taken`: quantità prelevata per candidato).
pub fn fefo_violations(
    candidates: &[Candidate],
    taken: &[Decimal],
    picked_expiry: Option<NaiveDate>,
) -> Vec<EarlierLot> {
    let Some(picked) = picked_expiry else {
        // Un lotto senza scadenza viene dopo tutti quelli con scadenza: salta ogni lotto datato.
        return earlier_than(candidates, taken, |_| true);
    };
    earlier_than(candidates, taken, |d| d < picked)
}

fn earlier_than(
    candidates: &[Candidate],
    taken: &[Decimal],
    is_earlier: impl Fn(NaiveDate) -> bool,
) -> Vec<EarlierLot> {
    let mut out: Vec<EarlierLot> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let (Some(expiry), Some(lot_id)) = (c.expiry_date, c.lot_id) else {
            continue;
        };
        if !c.usable || !is_earlier(expiry) {
            continue;
        }
        let left = c.quantity - taken.get(i).copied().unwrap_or_default();
        if left <= Decimal::ZERO {
            continue;
        }
        match out.iter_mut().find(|e| e.lot_id == lot_id) {
            Some(e) => e.quantity += left,
            None => out.push(EarlierLot {
                lot_id,
                lot_code: c.lot_code.clone().unwrap_or_default(),
                expiry_date: expiry,
                quantity: left,
            }),
        }
    }
    out.sort_by_key(|e| e.expiry_date);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rust_decimal_macros::dec;

    fn cand(code: &str, lot: &str, expiry: Option<&str>, received_day: u32, qty: Decimal, usable: bool) -> Candidate {
        Candidate {
            location_id: Uuid::new_v4(),
            location_code: code.into(),
            lot_id: Some(Uuid::from_u128(lot.bytes().map(u128::from).sum())),
            lot_code: Some(lot.into()),
            expiry_date: expiry.map(|d| d.parse().unwrap()),
            received_at: Utc.with_ymd_and_hms(2026, 1, received_day, 0, 0, 0).unwrap(),
            quantity: qty,
            usable,
        }
    }

    #[test]
    fn fefo_takes_earliest_expiry_first_even_if_received_later() {
        let c = vec![
            cand("A-01", "L1", Some("2026-12-31"), 1, dec!(10), true),
            cand("A-02", "L2", Some("2026-11-30"), 5, dec!(4), true),
        ];
        let r = allocate(&c, dec!(6), Strategy::Fefo, false);
        assert_eq!(
            r.picks,
            vec![
                Allocation {
                    candidate: 1,
                    quantity: dec!(4)
                },
                Allocation {
                    candidate: 0,
                    quantity: dec!(2)
                }
            ]
        );
        assert_eq!(r.shortfall, dec!(0));
    }

    #[test]
    fn fifo_ignores_expiry_and_uses_receipt_date() {
        let c = vec![
            cand("A-01", "L1", Some("2026-12-31"), 1, dec!(10), true),
            cand("A-02", "L2", Some("2026-11-30"), 5, dec!(4), true),
        ];
        let r = allocate(&c, dec!(6), Strategy::Fifo, false);
        assert_eq!(
            r.picks,
            vec![Allocation {
                candidate: 0,
                quantity: dec!(6)
            }]
        );
    }

    #[test]
    fn auto_resolves_by_presence_of_expiry() {
        let dated = vec![cand("A", "L1", Some("2026-12-31"), 1, dec!(1), true)];
        let mut undated = dated.clone();
        undated[0].expiry_date = None;
        assert_eq!(Strategy::Auto.resolve(false, &dated), Strategy::Fefo);
        assert_eq!(Strategy::Auto.resolve(false, &undated), Strategy::Fifo);
        assert_eq!(Strategy::Auto.resolve(true, &undated), Strategy::Fefo);
        assert_eq!(Strategy::Fifo.resolve(true, &dated), Strategy::Fifo);
    }

    #[test]
    fn unusable_lots_are_skipped_and_shortfall_reported() {
        let c = vec![
            cand("A-01", "EXPIRED", Some("2026-01-01"), 1, dec!(100), false),
            cand("A-02", "L2", Some("2026-11-30"), 5, dec!(4), true),
        ];
        let r = allocate(&c, dec!(10), Strategy::Fefo, false);
        assert_eq!(
            r.picks,
            vec![Allocation {
                candidate: 1,
                quantity: dec!(4)
            }]
        );
        assert_eq!(r.allocated, dec!(4));
        assert_eq!(r.shortfall, dec!(6));
    }

    #[test]
    fn whole_units_for_serial_products() {
        let c = vec![
            cand("A-01", "L1", None, 1, dec!(2.5), true),
            cand("A-02", "L2", None, 2, dec!(3), true),
        ];
        let r = allocate(&c, dec!(4), Strategy::Fifo, true);
        assert_eq!(
            r.picks,
            vec![
                Allocation {
                    candidate: 0,
                    quantity: dec!(2)
                },
                Allocation {
                    candidate: 1,
                    quantity: dec!(2)
                }
            ]
        );
    }

    #[test]
    fn fefo_violation_when_earlier_lot_left_in_stock() {
        let c = vec![
            cand("A-01", "LATE", Some("2026-12-31"), 1, dec!(10), true),
            cand("A-02", "EARLY", Some("2026-11-30"), 5, dec!(4), true),
        ];
        let v = fefo_violations(&c, &[dec!(5), dec!(0)], c[0].expiry_date);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].lot_code, "EARLY");
        assert_eq!(v[0].quantity, dec!(4));
    }

    #[test]
    fn no_violation_when_request_also_empties_earlier_lot() {
        let c = vec![
            cand("A-01", "LATE", Some("2026-12-31"), 1, dec!(10), true),
            cand("A-02", "EARLY", Some("2026-11-30"), 5, dec!(4), true),
        ];
        assert!(fefo_violations(&c, &[dec!(2), dec!(4)], c[0].expiry_date).is_empty());
    }

    #[test]
    fn expired_or_blocked_earlier_lots_do_not_count() {
        let c = vec![
            cand("A-01", "LATE", Some("2026-12-31"), 1, dec!(10), true),
            cand("A-02", "OLD", Some("2026-01-01"), 5, dec!(4), false),
        ];
        assert!(fefo_violations(&c, &[dec!(1), dec!(0)], c[0].expiry_date).is_empty());
    }
}
