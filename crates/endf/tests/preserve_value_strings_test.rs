//! Tests for `preserve_value_strings` option.
//!
//! Python semantics: when enabled, every float value read from the ENDF
//! text carries the original field string, and the writer emits that
//! string verbatim (right-justified to width) instead of reformatting
//! the numeric value. This achieves byte-exact float roundtrip.
//!
//! Tests verify:
//! 1. Default (off): parsed floats are `EndfValue::Float`, no originals.
//! 2. On: parsed floats are `EndfValue::PreservedFloat` with correct originals.
//! 3. Round-trip with preserve on: write→parse cycle is byte-exact for
//!    float fields.
//! 4. Non-standard formatting survives roundtrip (e.g., `1.23+8` vs the
//!    formatter's `1.230000+8`).

use endf::interpreter::engine::Engine;
use endf::options::{ParseOpts, ReadOpts, WriteOpts};
use endf::parser::EndfParser;
use endf::recipe::catalogue::RecipeCatalogue;
use endf::value::{EndfKey, EndfValue};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a minimal MF=3 ENDF text section from raw records so we have
/// complete control over the float field formatting.
fn build_mf3_text(
    za_str: &str,
    awr_str: &str,
    qm_str: &str,
    qi_str: &str,
    x_strs: &[&str],
    y_strs: &[&str],
) -> String {
    let w = 11;
    let mat = 125;
    let ctrl = format!("{:>4}{:>2}{:>2}", mat, 3, 1);

    // HEAD line: ZA, AWR, 0, 0, 0, 0
    let head = format!(
        "{:>w$}{:>w$}{:>w$}{:>w$}{:>w$}{:>w$}{}",
        za_str,
        awr_str,
        0,
        0,
        0,
        0,
        ctrl,
        w = w
    );

    // TAB1 header: QM, QI, 0, LR=0, NR=1, NP=len
    let np = x_strs.len();
    let tab1_head = format!(
        "{:>w$}{:>w$}{:>w$}{:>w$}{:>w$}{:>w$}{}",
        qm_str,
        qi_str,
        0,
        0,
        1,
        np,
        ctrl,
        w = w
    );

    // Interpolation line: NBT=np, INT=2 (lin-lin)
    let interp = format!(
        "{:>w$}{:>w$}{:>w$}{:>w$}{:>w$}{:>w$}{}",
        np, 2, "", "", "", "", ctrl,
        w = w,
    );

    // Data line(s): interleaved x[0], y[0], x[1], y[1], ...
    let mut data_fields = Vec::new();
    for (x, y) in x_strs.iter().zip(y_strs.iter()) {
        data_fields.push(format!("{:>w$}", x, w = w));
        data_fields.push(format!("{:>w$}", y, w = w));
    }
    // Pad to multiple of 6 fields
    while data_fields.len() % 6 != 0 {
        data_fields.push(format!("{:>w$}", "", w = w));
    }
    let mut data_lines = Vec::new();
    for chunk in data_fields.chunks(6) {
        data_lines.push(format!("{}{}", chunk.join(""), ctrl));
    }

    let mut lines = vec![head, tab1_head, interp];
    lines.extend(data_lines);
    lines.join("\n")
}

fn make_engine(preserve: bool) -> Engine {
    let mut ropts = ReadOpts::default();
    ropts.preserve_value_strings = preserve;
    Engine::new(
        RecipeCatalogue::endf6().unwrap(),
        ParseOpts::default(),
        ropts,
        WriteOpts::default(),
    )
}

// ---------------------------------------------------------------------------
// Test 1: default (off) — no PreservedFloat variants
// ---------------------------------------------------------------------------

#[test]
fn preserve_off_produces_only_float_variants() {
    let engine = make_engine(false);
    let text = build_mf3_text(
        " 2.605600+4",
        " 5.584500+1",
        " 0.000000+0",
        " 0.000000+0",
        &[" 1.000000-5", " 2.000000+7"],
        &[" 1.000000+1", " 5.000000+0"],
    );
    let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let data = engine.parse_section(3, 1, lines).unwrap();

    // ZA and AWR are header float fields. With preserve off, they must NOT
    // be PreservedFloat. They may be Int (via from_f64 for integer-valued
    // floats like ZA=26056.0) or Float — both are fine; only
    // PreservedFloat would indicate a bug.
    assert!(
        !matches!(data.get("ZA").unwrap(), EndfValue::PreservedFloat(_, _)),
        "ZA should not be PreservedFloat when preserve is off, got {:?}",
        data.get("ZA")
    );
    assert!(
        !matches!(data.get("AWR").unwrap(), EndfValue::PreservedFloat(_, _)),
        "AWR should not be PreservedFloat when preserve is off"
    );

    // Body x values
    let xstable = data.get("xstable").unwrap();
    let e_arr = xstable.get("E").unwrap().as_list().unwrap();
    for (i, item) in e_arr.iter().enumerate() {
        let v = item.as_ref().unwrap();
        assert!(
            !matches!(v, EndfValue::PreservedFloat(_, _)),
            "E[{}] should not be PreservedFloat when preserve is off, got {:?}",
            i, v
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2: preserve on — PreservedFloat with correct originals
// ---------------------------------------------------------------------------

#[test]
fn preserve_on_produces_preserved_float_with_originals() {
    let engine = make_engine(true);
    let text = build_mf3_text(
        " 2.605600+4",
        " 5.584500+1",
        " 0.000000+0",
        " 0.000000+0",
        &[" 1.000000-5", " 2.000000+7"],
        &[" 1.000000+1", " 5.000000+0"],
    );
    let lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
    let data = engine.parse_section(3, 1, lines).unwrap();

    // Header float fields: ZA, AWR
    match data.get("ZA").unwrap() {
        EndfValue::PreservedFloat(v, orig) => {
            assert!((v - 26056.0).abs() < 0.1, "ZA value");
            assert_eq!(orig, "2.605600+4", "ZA original string");
        }
        other => panic!("ZA should be PreservedFloat, got {:?}", other),
    }
    match data.get("AWR").unwrap() {
        EndfValue::PreservedFloat(v, orig) => {
            assert!((v - 55.845).abs() < 0.001, "AWR value");
            assert_eq!(orig, "5.584500+1", "AWR original string");
        }
        other => panic!("AWR should be PreservedFloat, got {:?}", other),
    }

    // QM, QI (from TAB1 header C1, C2)
    match data.get("QM").unwrap() {
        EndfValue::PreservedFloat(v, orig) => {
            assert_eq!(*v, 0.0, "QM value");
            assert_eq!(orig, "0.000000+0", "QM original string");
        }
        other => panic!("QM should be PreservedFloat, got {:?}", other),
    }

    // Body float values: E (x) array
    let xstable = data.get("xstable").unwrap();
    let e_arr = xstable.get("E").unwrap().as_list().unwrap();
    match e_arr[0].as_ref().unwrap() {
        EndfValue::PreservedFloat(v, orig) => {
            assert!((v - 1e-5).abs() < 1e-10, "E[0] value");
            assert_eq!(orig, "1.000000-5", "E[0] original string");
        }
        other => panic!("E[0] should be PreservedFloat, got {:?}", other),
    }

    // Body float values: xs (y) array
    let xs_arr = xstable.get("xs").unwrap().as_list().unwrap();
    match xs_arr[0].as_ref().unwrap() {
        EndfValue::PreservedFloat(v, orig) => {
            assert_eq!(*v, 10.0, "xs[0] value");
            assert_eq!(orig, "1.000000+1", "xs[0] original string");
        }
        other => panic!("xs[0] should be PreservedFloat, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Test 3: byte-exact roundtrip with preserve on
// ---------------------------------------------------------------------------

/// Parse with preserve on → write → the output must be byte-identical
/// to the input for all float fields. (Integer fields and ctrl columns
/// may differ due to reformatting, so we compare only the float-field
/// substrings.)
#[test]
fn preserve_roundtrip_is_byte_exact_for_float_fields() {
    let za_str = " 2.605600+4";
    let awr_str = " 5.584500+1";
    let qm_str = " 0.000000+0";
    let qi_str = " 0.000000+0";
    let x_strs = [" 1.000000-5", " 2.000000+7"];
    let y_strs = [" 1.000000+1", " 5.000000+0"];

    let original_text = build_mf3_text(za_str, awr_str, qm_str, qi_str, &x_strs, &y_strs);

    let engine = make_engine(true);
    let lines: Vec<String> = original_text.lines().map(|s| s.to_string()).collect();
    let data = engine.parse_section(3, 1, lines).unwrap();
    let written_lines = engine.write_section(3, 1, data).unwrap();
    let written_text = written_lines.join("\n");

    let orig_lines: Vec<&str> = original_text.lines().collect();
    let writ_lines: Vec<&str> = written_text.lines().collect();
    let w = 11;

    // Compare HEAD line float fields (C1=ZA, C2=AWR: first 2*w chars)
    assert_eq!(
        &orig_lines[0][..2 * w],
        &writ_lines[0][..2 * w],
        "HEAD float fields (ZA, AWR) must be byte-exact"
    );

    // Compare TAB1 header float fields (C1=QM, C2=QI: first 2*w chars)
    assert_eq!(
        &orig_lines[1][..2 * w],
        &writ_lines[1][..2 * w],
        "TAB1 header float fields (QM, QI) must be byte-exact"
    );

    // Compare TAB1 body data line(s) (x/y values: first 4*w chars)
    assert_eq!(
        &orig_lines[3][..4 * w],
        &writ_lines[3][..4 * w],
        "TAB1 body float fields (x[0], y[0], x[1], y[1]) must be byte-exact"
    );
}

// ---------------------------------------------------------------------------
// Test 4: non-standard formatting survives roundtrip
// ---------------------------------------------------------------------------

/// Use a deliberately non-standard float formatting (fewer digits than
/// the formatter would produce) and verify it survives a preserve-on
/// roundtrip unchanged.
#[test]
fn preserve_roundtrip_non_standard_formatting() {
    // These strings are valid ENDF floats but NOT what f64_to_fortstr
    // would produce (it would output " 1.230000+4" etc.). They are
    // already right-justified (no trailing spaces) so that the
    // trim→right-justify roundtrip is identity.
    let za_str = "   1.2300+4";
    let awr_str = "     55.845";  // no exponent, right-justified

    let original_text = build_mf3_text(
        za_str,
        awr_str,
        " 0.000000+0",
        " 0.000000+0",
        &["     1.0-05"],  // unusual exponent format
        &["        10."],  // trailing dot, right-justified
    );

    let engine = make_engine(true);
    let lines: Vec<String> = original_text.lines().map(|s| s.to_string()).collect();
    let data = engine.parse_section(3, 1, lines).unwrap();
    let written_lines = engine.write_section(3, 1, data).unwrap();
    let written_text = written_lines.join("\n");

    let orig_lines: Vec<&str> = original_text.lines().collect();
    let writ_lines: Vec<&str> = written_text.lines().collect();
    let w = 11;

    // HEAD float fields must preserve original non-standard strings
    assert_eq!(
        &orig_lines[0][..w],
        &writ_lines[0][..w],
        "ZA non-standard format must survive"
    );
    assert_eq!(
        &orig_lines[0][w..2 * w],
        &writ_lines[0][w..2 * w],
        "AWR non-standard format must survive"
    );
}

// ---------------------------------------------------------------------------
// Test 5: full-file roundtrip via EndfParser with preserve on
// ---------------------------------------------------------------------------

/// Build a complete multi-section ENDF "file" with known float strings,
/// parse and write via EndfParser with preserve_value_strings, and
/// verify the float fields survive byte-exact.
#[test]
fn full_file_preserve_roundtrip() {
    let parser = EndfParser::builder()
        .preserve_value_strings(true)
        .build()
        .unwrap();

    // Build a fixture via a non-preserving write, then re-parse with
    // preserve and re-write — the second write must be byte-identical
    // to the first.
    let parser_nopres = EndfParser::builder().build().unwrap();

    let mut data = EndfValue::new_dict();

    // MF=0 TPID
    let mut mf0 = EndfValue::new_dict();
    let mut tpid_sec = EndfValue::new_dict();
    tpid_sec.insert(
        EndfKey::Str("TPID".into()),
        EndfValue::Str(
            "  PRESERVE_VALUE_STRINGS_TEST                                         ".into(),
        ),
    );
    mf0.insert(EndfKey::Int(0), tpid_sec);
    data.insert(EndfKey::Int(0), mf0);

    // MF=3/MT=1 (built in memory like previous tests)
    let mut d = EndfValue::new_dict();
    d.insert("MAT", EndfValue::Int(125));
    d.insert("MF", EndfValue::Int(3));
    d.insert("MT", EndfValue::Int(1));
    d.insert("ZA", EndfValue::Float(26056.0));
    d.insert("AWR", EndfValue::Float(55.845));
    d.insert("QM", EndfValue::Float(0.0));
    d.insert("QI", EndfValue::Float(0.0));
    d.insert("LR", EndfValue::Int(0));

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

    let mut mf3 = EndfValue::new_dict();
    mf3.insert(EndfKey::Int(1), d);
    data.insert(EndfKey::Int(3), mf3);

    // Step 1: Write with default (non-preserve) parser → canonical text.
    let text1 = parser_nopres.write(&data).unwrap();

    // Step 2: Re-parse with preserve on → preserved data.
    let preserved_data = parser.parse(&text1).unwrap();

    // Step 3: Re-write the preserved data → text2 must match text1.
    let text2 = parser.write(&preserved_data).unwrap();

    assert_eq!(
        text1, text2,
        "Full-file roundtrip with preserve_value_strings must be byte-exact.\n\
         Differences in first mismatch:\n{}\nvs\n{}",
        text1.lines().zip(text2.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(i, (a, b))| format!("line {}: {:?} vs {:?}", i, a, b))
            .unwrap_or_else(|| "none (length mismatch?)".into()),
        ""
    );
}
