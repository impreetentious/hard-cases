//! Points that are nearly feasible are not feasible.
//!
//! The incumbent is the number every prune in the tree is measured against, and
//! a prune is irreversible. A candidate accepted because it sits inside a
//! tolerance would let the search discard regions containing the answer, and
//! the certificate it eventually writes would be a correct proof of a false
//! statement. There is no fence anywhere in the screening and this is where
//! that is established.
//!
//! The points below are constructed to be exactly the shape a floating search
//! produces: right to within any reasonable tolerance and wrong.

use hc_exact::{BigInt, Rational};
use hc_io::mps::{Form, read};
use hc_lp::exact::Canonical;
use hc_mip::incumbent::{Rejected, extend, screen};

const MODEL: &str = concat!(
    "NAME I\n",
    "ROWS\n",
    " N obj\n",
    " L r0\n",
    " E r1\n",
    "COLUMNS\n",
    " MARKER 'MARKER' 'INTORG'\n",
    " x obj -1 r0 1\n",
    " x r1 1\n",
    " y obj -1 r0 1\n",
    " MARKER 'MARKER' 'INTEND'\n",
    "RHS\n",
    " rhs r0 4 r1 2\n",
    "BOUNDS\n",
    " UI b x 5\n",
    " UI b y 5\n",
    "ENDATA\n",
);

fn canonical() -> Canonical {
    Canonical::of(&read(MODEL.as_bytes(), Form::Free).expect("the source parses"))
}

fn whole(value: i64) -> Rational {
    Rational::from_i64(value)
}

/// A value one part in ten million away from an integer, which is inside every
/// tolerance this project names and outside the integers.
fn nearly(value: i64) -> Rational {
    let scale = Rational::new(BigInt::from_i64(1), BigInt::from_i64(10_000_000))
        .expect("a nonzero denominator");
    whole(value).add(&scale)
}

fn point(x: Rational, y: Rational) -> Vec<Rational> {
    extend(&canonical(), &[x, y]).expect("two variables")
}

#[test]
fn trap_incumbent_rounding() {
    let canonical = canonical();

    // The genuine article: x = 2 from the equality, y = 2 from the room left.
    screen(&canonical, &point(whole(2), whole(2))).expect("x = 2, y = 2 is feasible");

    // A value a hair off an integer is not an integer. Which rule catches it
    // depends on where the departure lands — the equality it breaks, the bound
    // it crosses, or the integrality it misses — and the point of the trap is
    // that one of them always does.
    assert!(
        screen(&canonical, &point(nearly(2), whole(2))).is_err(),
        "a near-integer candidate was accepted"
    );

    // And where the equality still holds, the integrality does not.
    let almost = point(whole(2), nearly(2));
    assert!(
        matches!(
            screen(&canonical, &almost),
            Err(Rejected::Fractional { .. } | Rejected::Residual { .. } | Rejected::Bounds { .. })
        ),
        "a fractional component was accepted"
    );
}

#[test]
fn a_point_a_hair_outside_a_bound_is_outside_it() {
    let canonical = canonical();
    let scale = Rational::new(BigInt::from_i64(1), BigInt::from_i64(1_000_000_000))
        .expect("a nonzero denominator");
    let over = whole(5).add(&scale);
    // x is fixed at 2 by the equality, so this is about y's upper bound of 5.
    assert!(screen(&canonical, &point(whole(2), over)).is_err());
}

#[test]
fn the_screening_has_no_tolerance_anywhere_in_it() {
    let canonical = canonical();
    // A sequence of ever-smaller departures. Every one of them is refused,
    // which is what "no fence" means: there is no size below which a wrong
    // point becomes acceptable.
    for exponent in 1..12 {
        let mut denominator = BigInt::from_i64(1);
        for _ in 0..exponent {
            denominator = denominator.mul(&BigInt::from_i64(10));
        }
        let nudge = Rational::new(BigInt::from_i64(1), denominator).expect("a nonzero denominator");
        let candidate = point(whole(2), whole(2).add(&nudge));
        assert!(
            screen(&canonical, &candidate).is_err(),
            "a departure of one part in ten to the {exponent} was accepted"
        );
    }
}
