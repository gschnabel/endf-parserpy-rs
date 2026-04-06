//! Tests for `strict_datatypes` on WriteOpts.
//!
//! Python semantics:
//! * Lenient (default, `strict_datatypes=False`):
//!   - `EndfValue::Int` in an int field → always accepted.
//!   - `EndfValue::Float(5.0)` in an int field → accepted (integer-valued).
//!   - `EndfValue::Float(5.7)` in an int field → `NonIntegerField` error.
//! * Strict (`strict_datatypes=True`):
//!   - `EndfValue::Int` → accepted.
//!   - Any `EndfValue::Float` → `StrictFloatInIntField` error, even `5.0`.
//!
//! These tests exercise the check through the full write pipeline
//! (EndfParser → engine → mappings → record writer), not just the
//! helper in isolation, to pin the end-to-end wiring.

use endf::error::EndfError;
use endf::interpreter::engine::Engine;
use endf::options::{ParseOpts, ReadOpts, WriteOpts};
use endf::recipe::catalogue::RecipeCatalogue;
use endf::value::EndfValue;

fn make_engine(strict: bool) -> Engine {
    let mut wopts = WriteOpts::default();
    wopts.strict_datatypes = strict;
    Engine::new(
        RecipeCatalogue::endf6().unwrap(),
        ParseOpts::default(),
        ReadOpts::default(),
        wopts,
    )
}

/// Minimal MF=3/MT=1 section dict. Caller can override `LR` to inject
/// Float-typed values in the integer field for testing.
fn make_mf3_dict(lr_value: EndfValue) -> EndfValue {
    let mut d = EndfValue::new_dict();
    d.insert("MAT", EndfValue::Int(125));
    d.insert("MF", EndfValue::Int(3));
    d.insert("MT", EndfValue::Int(1));
    d.insert("ZA", EndfValue::Float(26056.0));
    d.insert("AWR", EndfValue::Float(55.845));
    d.insert("QM", EndfValue::Float(0.0));
    d.insert("QI", EndfValue::Float(0.0));
    d.insert("LR", lr_value);
    d.insert("NR", EndfValue::Int(1));
    d.insert("NP", EndfValue::Int(2));

    let mut xstable = EndfValue::new_dict();
    xstable.insert("NBT", EndfValue::List(vec![Some(EndfValue::Int(2))]));
    xstable.insert("INT", EndfValue::List(vec![Some(EndfValue::Int(2))]));
    xstable.insert(
        "E",
        EndfValue::List(vec![
            Some(EndfValue::Float(1e-5)),
            Some(EndfValue::Float(2e7)),
        ]),
    );
    xstable.insert(
        "xs",
        EndfValue::List(vec![
            Some(EndfValue::Float(10.0)),
            Some(EndfValue::Float(1.0)),
        ]),
    );
    d.insert("xstable", xstable);
    d
}

// ---------------------------------------------------------------------------
// Lenient mode (default)
// ---------------------------------------------------------------------------

#[test]
fn lenient_accepts_int_in_int_field() {
    let engine = make_engine(false);
    let data = make_mf3_dict(EndfValue::Int(0));
    let result = engine.write_section(3, 1, data);
    assert!(result.is_ok(), "Int in int field must succeed in lenient mode");
}

#[test]
fn lenient_accepts_integer_valued_float_in_int_field() {
    let engine = make_engine(false);
    let data = make_mf3_dict(EndfValue::Float(0.0));
    let result = engine.write_section(3, 1, data);
    assert!(
        result.is_ok(),
        "Float(0.0) in int field must succeed in lenient mode"
    );
}

#[test]
fn lenient_rejects_non_integer_float_in_int_field() {
    let engine = make_engine(false);
    let data = make_mf3_dict(EndfValue::Float(0.5));
    let result = engine.write_section(3, 1, data);
    match result {
        Err(EndfError::NonIntegerField { field, value }) => {
            assert!(
                field.contains("LR") || field.contains("L2"),
                "error field should name the recipe slot, got '{}'",
                field
            );
            assert_eq!(value, 0.5);
        }
        Err(other) => panic!("expected NonIntegerField, got {:?}", other),
        Ok(_) => panic!("Float(0.5) in int field must fail in lenient mode"),
    }
}

// ---------------------------------------------------------------------------
// Strict mode
// ---------------------------------------------------------------------------

#[test]
fn strict_accepts_int_in_int_field() {
    let engine = make_engine(true);
    let data = make_mf3_dict(EndfValue::Int(0));
    let result = engine.write_section(3, 1, data);
    assert!(result.is_ok(), "Int in int field must succeed in strict mode");
}

#[test]
fn strict_rejects_integer_valued_float_in_int_field() {
    let engine = make_engine(true);
    let data = make_mf3_dict(EndfValue::Float(0.0));
    let result = engine.write_section(3, 1, data);
    match result {
        Err(EndfError::StrictFloatInIntField { field, value }) => {
            assert!(
                field.contains("LR") || field.contains("L2"),
                "error field should name the recipe slot, got '{}'",
                field
            );
            assert_eq!(value, 0.0);
        }
        Err(other) => panic!("expected StrictFloatInIntField, got {:?}", other),
        Ok(_) => panic!("Float(0.0) in int field must fail in strict mode"),
    }
}

#[test]
fn strict_rejects_non_integer_float_in_int_field() {
    let engine = make_engine(true);
    let data = make_mf3_dict(EndfValue::Float(0.5));
    let result = engine.write_section(3, 1, data);
    match result {
        Err(EndfError::StrictFloatInIntField { field, value }) => {
            assert!(
                field.contains("LR") || field.contains("L2"),
                "error field should name the recipe slot, got '{}'",
                field
            );
            assert_eq!(value, 0.5);
        }
        Err(other) => panic!("expected StrictFloatInIntField, got {:?}", other),
        Ok(_) => panic!("Float(0.5) in int field must fail in strict mode"),
    }
}
