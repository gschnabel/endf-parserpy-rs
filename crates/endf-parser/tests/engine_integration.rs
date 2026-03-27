//! Integration tests for the interpreter engine and public parser API.

use endf_parser::interpreter::engine::Engine;
use endf_parser::options::{ParseOpts, ReadOpts, WriteOpts};
use endf_parser::recipe::catalogue::RecipeCatalogue;
use endf_parser::records::{self, ContRecord, CtrlRecord, Tab1Body};
use endf_parser::value::EndfValue;

fn default_engine() -> Engine {
    Engine::new(
        RecipeCatalogue::endf6().unwrap(),
        ParseOpts::default(),
        ReadOpts::default(),
        WriteOpts::default(),
    )
}

// ---------------------------------------------------------------------------
// Engine-level: parse an MF3 section (HEAD + TAB1 + SEND)
// ---------------------------------------------------------------------------

#[test]
fn test_parse_mf3_section() {
    let engine = default_engine();
    let write_opts = WriteOpts::default();
    let ctrl = CtrlRecord {
        mat: 125,
        mf: 3,
        mt: 1,
    };

    // HEAD line: ZA=26056.0, AWR=55.845, L1=0, L2=0, N1=0, N2=0
    let head_rec = ContRecord {
        c1: 26056.0,
        c2: 55.845,
        l1: 0,
        l2: 0,
        n1: 0,
        n2: 0,
    };
    let head_line = records::write_head(&head_rec, &ctrl, &write_opts);

    // TAB1 header: QM=0.0, QI=0.0, L1=0, LR=0, NR=1, NP=3
    let tab1_head = ContRecord {
        c1: 0.0,
        c2: 0.0,
        l1: 0,
        l2: 0,
        n1: 1,
        n2: 3,
    };
    let tab1_head_line = records::write_cont(&tab1_head, &ctrl, &write_opts);

    // TAB1 body: NR=1, NP=3
    let tab1_body = Tab1Body {
        nbt: vec![3],
        int: vec![2], // lin-lin
        x: vec![1.0e-5, 1.0e6, 2.0e7],
        y: vec![10.0, 20.0, 5.0],
    };
    let body_lines = records::write_tab1_body(&tab1_body, &ctrl, &write_opts);

    let mut lines = vec![head_line, tab1_head_line];
    lines.extend(body_lines);

    let result = engine.parse_section(3, 1, lines).unwrap();

    // Check that the parsed data contains the expected variables
    assert_eq!(result.get("ZA").unwrap().as_float(), Some(26056.0));
    assert_eq!(result.get("AWR").unwrap().as_float(), Some(55.845));
    assert_eq!(result.get("MAT").unwrap().as_int(), Some(125));
    assert_eq!(result.get("MF").unwrap().as_int(), Some(3));
    assert_eq!(result.get("MT").unwrap().as_int(), Some(1));
    assert_eq!(result.get("QM").unwrap().as_float(), Some(0.0));
    assert_eq!(result.get("QI").unwrap().as_float(), Some(0.0));
    assert_eq!(result.get("LR").unwrap().as_int(), Some(0));
    // NR and NP are NOT stored as named variables for TAB1 records;
    // they are inferred from the table arrays (NBT.len() and X.len()).

    // Check the xstable section data
    let xstable = result.get("xstable").unwrap();
    assert!(xstable.is_dict(), "xstable should be a dict");

    // Check NBT and INT arrays (stored as List, 0-indexed)
    let nbt = xstable.get("NBT").unwrap();
    assert!(nbt.is_list(), "NBT should be a list");
    let nbt_list = nbt.as_list().unwrap();
    assert_eq!(nbt_list[0].as_ref().unwrap().as_int(), Some(3));

    let int = xstable.get("INT").unwrap();
    assert!(int.is_list(), "INT should be a list");
    let int_list = int.as_list().unwrap();
    assert_eq!(int_list[0].as_ref().unwrap().as_int(), Some(2));

    // Check E and xs arrays (stored as List, 0-indexed)
    let e_arr = xstable.get("E").unwrap();
    assert!(e_arr.is_list(), "E should be a list");
    let e_list = e_arr.as_list().unwrap();
    assert_eq!(e_list.len(), 3);
    assert_eq!(e_list[0].as_ref().unwrap().as_float(), Some(1.0e-5));
    assert_eq!(e_list[2].as_ref().unwrap().as_float(), Some(2.0e7));

    let xs_arr = xstable.get("xs").unwrap();
    assert!(xs_arr.is_list(), "xs should be a list");
    let xs_list = xs_arr.as_list().unwrap();
    assert_eq!(xs_list.len(), 3);
    assert_eq!(xs_list[0].as_ref().unwrap().as_float(), Some(10.0));
    assert_eq!(xs_list[1].as_ref().unwrap().as_float(), Some(20.0));
    assert_eq!(xs_list[2].as_ref().unwrap().as_float(), Some(5.0));
}

// ---------------------------------------------------------------------------
// Engine-level: write an MF3 section and verify it round-trips
// ---------------------------------------------------------------------------

#[test]
fn test_write_mf3_section() {
    let engine = default_engine();

    // Build data dictionary manually
    let mut datadic = EndfValue::new_dict();
    datadic.insert("MAT", EndfValue::Int(125));
    datadic.insert("MF", EndfValue::Int(3));
    datadic.insert("MT", EndfValue::Int(1));
    datadic.insert("ZA", EndfValue::Float(26056.0));
    datadic.insert("AWR", EndfValue::Float(55.845));
    datadic.insert("QM", EndfValue::Float(0.0));
    datadic.insert("QI", EndfValue::Float(0.0));
    datadic.insert("LR", EndfValue::Int(0));
    datadic.insert("NR", EndfValue::Int(1));
    datadic.insert("NP", EndfValue::Int(2));

    let mut xstable = EndfValue::new_dict();
    xstable.insert(
        "NBT",
        EndfValue::List(vec![Some(EndfValue::Int(2))]),
    );
    xstable.insert(
        "INT",
        EndfValue::List(vec![Some(EndfValue::Int(2))]),
    );
    xstable.insert(
        "E",
        EndfValue::List(vec![
            Some(EndfValue::Float(1.0e-5)),
            Some(EndfValue::Float(2.0e7)),
        ]),
    );
    xstable.insert(
        "xs",
        EndfValue::List(vec![
            Some(EndfValue::Float(100.0)),
            Some(EndfValue::Float(1.0)),
        ]),
    );

    datadic.insert("xstable", xstable);

    let lines = engine.write_section(3, 1, datadic).unwrap();

    // Should produce at least 3 lines (HEAD + TAB1 header + body)
    assert!(lines.len() >= 3, "expected >= 3 lines, got {}", lines.len());

    // Re-parse the written lines and verify round-trip
    let reparsed = engine.parse_section(3, 1, lines).unwrap();
    assert_eq!(reparsed.get("ZA").unwrap().as_float(), Some(26056.0));
}

// ---------------------------------------------------------------------------
// Public API: EndfParser builder
// ---------------------------------------------------------------------------

#[test]
fn test_parser_builder() {
    use endf_parser::parser::EndfParser;
    // Verify that the builder can create a parser
    let parser = EndfParser::builder()
        .ignore_zero_mismatch(true)
        .accept_spaces(true)
        .build()
        .unwrap();
    // Parser should be usable (we just test it doesn't panic on creation)
    drop(parser);
}
