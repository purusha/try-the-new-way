//! Controllo di capienza delle ubicazioni in peso e volume (spec 007, regola 5).

use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Dimension {
    Weight,
    Volume,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Exceeded {
    pub dimension: Dimension,
    pub current: Decimal,
    pub incoming: Decimal,
    pub max: Decimal,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Load {
    pub weight_kg: Decimal,
    pub volume_m3: Decimal,
}

/// Verifica che `current + incoming` non superi i massimi (`None` = illimitato).
pub fn check(
    current: Load,
    incoming: Load,
    max_weight: Option<Decimal>,
    max_volume: Option<Decimal>,
) -> Result<(), Exceeded> {
    if let Some(max) = max_weight
        && current.weight_kg + incoming.weight_kg > max
    {
        return Err(Exceeded {
            dimension: Dimension::Weight,
            current: current.weight_kg,
            incoming: incoming.weight_kg,
            max,
        });
    }
    if let Some(max) = max_volume
        && current.volume_m3 + incoming.volume_m3 > max
    {
        return Err(Exceeded {
            dimension: Dimension::Volume,
            current: current.volume_m3,
            incoming: incoming.volume_m3,
            max,
        });
    }
    Ok(())
}

/// Percentuale di occupazione, `None` se la capienza è illimitata.
pub fn percent(current: Decimal, max: Option<Decimal>) -> Option<Decimal> {
    max.filter(|m| *m > Decimal::ZERO)
        .map(|m| (current * Decimal::ONE_HUNDRED / m).round_dp(1))
}

/// Piena se almeno una dimensione limitata ha raggiunto il massimo.
pub fn is_full(current: Load, max_weight: Option<Decimal>, max_volume: Option<Decimal>) -> bool {
    max_weight.is_some_and(|m| current.weight_kg >= m) || max_volume.is_some_and(|m| current.volume_m3 >= m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn load(w: Decimal, v: Decimal) -> Load {
        Load {
            weight_kg: w,
            volume_m3: v,
        }
    }

    #[test]
    fn within_limits() {
        assert!(
            check(
                load(dec!(800), dec!(1)),
                load(dec!(200), dec!(1)),
                Some(dec!(1000)),
                Some(dec!(2))
            )
            .is_ok()
        );
    }

    #[test]
    fn weight_exceeded() {
        let e = check(
            load(dec!(850), dec!(0)),
            load(dec!(247.2), dec!(0)),
            Some(dec!(1000)),
            None,
        )
        .unwrap_err();
        assert_eq!(e.dimension, Dimension::Weight);
        assert_eq!(e.max, dec!(1000));
    }

    #[test]
    fn volume_exceeded_while_weight_unlimited() {
        let e = check(
            load(dec!(5000), dec!(1.5)),
            load(dec!(10), dec!(0.6)),
            None,
            Some(dec!(2)),
        )
        .unwrap_err();
        assert_eq!(e.dimension, Dimension::Volume);
    }

    #[test]
    fn unlimited_location_never_full() {
        assert!(check(load(dec!(1e9), dec!(1e9)), load(dec!(1), dec!(1)), None, None).is_ok());
        assert!(!is_full(load(dec!(1e9), dec!(1e9)), None, None));
    }

    #[test]
    fn full_and_percent() {
        assert!(is_full(load(dec!(1000), dec!(0)), Some(dec!(1000)), Some(dec!(5))));
        assert_eq!(percent(dec!(250), Some(dec!(1000))), Some(dec!(25.0)));
        assert_eq!(percent(dec!(250), None), None);
    }
}
