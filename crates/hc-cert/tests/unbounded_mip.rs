//! An unbounded mixed-integer model, proved unbounded over its own integer
//! points rather than over a relaxation of them.
//!
//! A relaxation that runs away establishes nothing about the model it came
//! from. The integer points inside that region may be bounded, and there may be
//! none at all — so a linear ray is not a mixed-integer ray, and a certificate
//! that offered one would be claiming something it had not shown.
//!
//! What closes the distance is a lattice step: a positive integer `q` for which
//! every integer component of `q d` is whole. From an integral feasible point
//! `x0`, every `x0 + n q d` is then feasible and integral for every nonnegative
//! integer `n`, and the objective falls without limit along them.
//!
//! The instance here is chosen so the step is not one. Its recession direction
//! holds a fraction on an integer column, and a certificate that shipped the
//! direction without clearing that denominator would describe a walk that
//! leaves the lattice on its first step.

use hc_cert::hcc::{Certificate, ClaimKind, Record};
use hc_cert::mip::assemble_mip;
use hc_core::Model;
use hc_exact::Rational;
use hc_io::mps::{Form, read};
use hc_mip::search::{Options, run};
use hc_verify::vclose::{Verdict, verify};
use hc_verify::vhcc::Claim;

/// Minimise `-x - y` over `2x = 3y`, both integer and unbounded above.
///
/// The integer points are exactly `(3k, 2k)`, so the objective reaches `-5k`
/// for every `k` and the model is unbounded over its own lattice. The
/// relaxation's recession direction leaves `x` at one and `y` at two thirds,
/// which is the fraction the step has to clear: `q = 3`, and `q d = (3, 2)`
/// steps from one integer point to the next.
const UNBOUNDED: &str = "NAME UNB
ROWS
 N obj
 E r0
COLUMNS
 MARKER 'MARKER' 'INTORG'
 x obj -1.0 r0 2.0
 y obj -1.0 r0 -3.0
 MARKER 'MARKER' 'INTEND'
RHS
 rhs r0 0.0
BOUNDS
 PL b x
 PL b y
ENDATA
";

fn model() -> Model {
    read(UNBOUNDED.as_bytes(), Form::Free).expect("the instance parses")
}

fn certified() -> (Vec<u8>, Verdict) {
    let model = model();
    let search = run(&model, &Options::default());
    let certificate =
        assemble_mip(&model, UNBOUNDED.as_bytes(), &search).expect("the ray assembles");
    assert_eq!(certificate.claim, ClaimKind::Unbounded);

    let bytes = certificate.encode();
    let verdict = verify(&bytes, UNBOUNDED.as_bytes())
        .unwrap_or_else(|refusal| panic!("the verifier rejected an honest ray: {refusal}"));
    (bytes, verdict)
}

#[test]
fn an_unbounded_mixed_integer_model_certifies_and_verifies() {
    let (_, verdict) = certified();
    assert_eq!(verdict.claim, Claim::Unbounded);
}

#[test]
fn the_search_keeps_the_witness_the_claim_is_built_from() {
    let search = run(&model(), &Options::default());
    let witness = search
        .unbounded
        .as_ref()
        .expect("an unbounded search keeps its witness");
    assert!(
        search.tree.get(witness.node).is_some(),
        "the witness names a node the tree does not have"
    );
    assert!(
        search.incumbent.is_some(),
        "no integral point was found to travel from, so no ray could be built"
    );
}

/// The honest certificate's ray record: its direction and its step.
fn ray_of(certificate: &Certificate) -> (Vec<Rational>, Rational) {
    certificate
        .records
        .iter()
        .find_map(|record| match record {
            Record::Ray {
                direction, step, ..
            } => step.clone().map(|step| (direction.clone(), step)),
            _ => None,
        })
        .expect("the ray carries a direction and a step")
}

#[test]
fn the_step_clears_the_denominators_the_direction_holds() {
    let model = model();
    let search = run(&model, &Options::default());
    let certificate =
        assemble_mip(&model, UNBOUNDED.as_bytes(), &search).expect("the ray assembles");
    let (direction, step) = ray_of(&certificate);

    // Which column the simplex entered on decides whether the direction comes
    // out as (1, 2/3) or (3/2, 1), and either is the same ray. What cannot
    // differ is that one of them holds a fraction on an integer column, so the
    // step has real work to do and cannot be one.
    assert!(
        direction.iter().any(|value| !value.is_integer()),
        "the direction is already integral, so this instance no longer tests the step"
    );
    assert!(
        step.cmp_value(&Rational::one()).is_gt(),
        "the step is {step}, which clears no denominator"
    );
    for value in &direction {
        assert!(
            step.mul(value).is_integer(),
            "the stepped direction still holds {} on an integer column",
            step.mul(value)
        );
    }
}

/// A linear ray is not a mixed-integer ray, and the verifier must say so.
///
/// This is the whole distance between the two claims. The direction below is
/// the honest one and the base point is the honest one; only the step is wrong,
/// which is exactly the certificate a solver would produce if it treated an
/// unbounded relaxation as an unbounded model. Both forgeries have to be
/// refused, or the mixed-integer claim means nothing more than the linear one.
#[test]
fn trap_lp_ray_not_mip_ray() {
    let model = model();
    let search = run(&model, &Options::default());
    let honest = assemble_mip(&model, UNBOUNDED.as_bytes(), &search).expect("the ray assembles");

    for (name, forged, expected) in [
        ("no step at all", None, "HCC-RAY-06"),
        // A step of one is what a linear ray amounts to: travel the direction
        // as it stands. The honest step is greater than one precisely because
        // that walk leaves the lattice on its first move.
        ("a step of one", Some(Rational::one()), "HCC-RAY-07"),
    ] {
        let records = honest
            .records
            .iter()
            .map(|record| match record {
                Record::Ray {
                    base, direction, ..
                } => Record::Ray {
                    base: base.clone(),
                    direction: direction.clone(),
                    step: forged.clone(),
                },
                other => other.clone(),
            })
            .collect();
        let forged = Certificate {
            records,
            ..honest.clone()
        };
        let refusal = verify(&forged.encode(), UNBOUNDED.as_bytes())
            .expect_err(&format!("a ray with {name} must be refused"));
        assert_eq!(
            refusal.code.text(),
            expected,
            "a ray with {name} was refused, but not by the rule that should see it first"
        );
    }
}
