//! Tests for serde support, nofail mode, and CRLF handling.

use endf::parser::EndfParser;
use endf::value::{EndfKey, EndfValue};

// ---------------------------------------------------------------------------
// Feature 1: serde JSON roundtrip
// ---------------------------------------------------------------------------

#[test]
fn test_serde_json_roundtrip_scalars() {
    let values = vec![
        EndfValue::Int(42),
        EndfValue::Float(3.14),
        EndfValue::Str("hello".to_string()),
    ];
    for val in &values {
        let json = serde_json::to_string(val).expect("serialize");
        let back: EndfValue = serde_json::from_str(&json).expect("deserialize");
        match (val, &back) {
            (EndfValue::Int(a), EndfValue::Int(b)) => assert_eq!(a, b),
            (EndfValue::Float(a), EndfValue::Float(b)) => assert_eq!(a, b),
            (EndfValue::Str(a), EndfValue::Str(b)) => assert_eq!(a, b),
            _ => panic!("type mismatch after roundtrip"),
        }
    }
}

#[test]
fn test_serde_json_roundtrip_dict() {
    let mut dict = EndfValue::new_dict();
    dict.insert("ZA", EndfValue::Float(26056.0));
    dict.insert("AWR", EndfValue::Float(55.845));
    dict.insert(EndfKey::Int(1), EndfValue::Int(99));

    let json = serde_json::to_string(&dict).expect("serialize");
    let back: EndfValue = serde_json::from_str(&json).expect("deserialize");

    let d = back.as_dict().expect("should be dict");
    assert_eq!(d.get(&EndfKey::Str("ZA".into())).unwrap().as_float(), Some(26056.0));
    assert_eq!(d.get(&EndfKey::Str("AWR".into())).unwrap().as_float(), Some(55.845));
    assert_eq!(d.get(&EndfKey::Int(1)).unwrap().as_int(), Some(99));
}

#[test]
fn test_serde_json_roundtrip_list() {
    let list = EndfValue::List(vec![
        Some(EndfValue::Int(1)),
        None,
        Some(EndfValue::Float(2.5)),
    ]);

    let json = serde_json::to_string(&list).expect("serialize");
    let back: EndfValue = serde_json::from_str(&json).expect("deserialize");

    let l = back.as_list().expect("should be list");
    assert_eq!(l.len(), 3);
    assert_eq!(l[0].as_ref().unwrap().as_int(), Some(1));
    assert!(l[1].is_none());
    assert_eq!(l[2].as_ref().unwrap().as_float(), Some(2.5));
}

#[test]
fn test_serde_json_roundtrip_parsed_endf() {
    use endf::records::{self, ContRecord, CtrlRecord, Tab1Body};
    use endf::options::WriteOpts;

    let write_opts = WriteOpts::default();
    let ctrl = CtrlRecord { mat: 125, mf: 3, mt: 1 };

    let head_rec = ContRecord { c1: 26056.0, c2: 55.845, l1: 0, l2: 0, n1: 0, n2: 0 };
    let head_line = records::write_head(&head_rec, &ctrl, &write_opts);

    let tab1_head = ContRecord { c1: 0.0, c2: 0.0, l1: 0, l2: 0, n1: 1, n2: 3 };
    let tab1_head_line = records::write_cont(&tab1_head, &ctrl, &write_opts);

    let tab1_body = Tab1Body {
        nbt: vec![3], int: vec![2],
        x: vec![1.0e-5, 1.0e6, 2.0e7],
        y: vec![10.0, 20.0, 5.0],
    };
    let body_lines = records::write_tab1_body(&tab1_body, &ctrl, &write_opts);

    let send = records::write_send(125, 3, &write_opts);
    let fend = records::write_fend(125, &write_opts);
    let mend = records::write_mend(&write_opts);
    let tend = records::write_tend(&write_opts);

    let mut all_lines = vec![head_line, tab1_head_line];
    all_lines.extend(body_lines);
    all_lines.push(send);
    all_lines.push(fend);
    all_lines.push(mend);
    all_lines.push(tend);
    let endf_text = all_lines.join("\n");

    let parser = EndfParser::builder()
        .ignore_missing_tpid(true)
        .ignore_send_records(true)
        .nofail(true)
        .build()
        .expect("parser");

    let data = parser.parse(&endf_text).expect("parse");

    // Serialize to JSON and back
    let json = serde_json::to_string(&data).expect("serialize");
    let back: EndfValue = serde_json::from_str(&json).expect("deserialize");

    // Verify structure survived roundtrip
    let section = back.get_path("3/1").expect("3/1");
    assert_eq!(section.get("ZA").unwrap().as_float(), Some(26056.0));
    assert_eq!(section.get("AWR").unwrap().as_float(), Some(55.845));
}

// ---------------------------------------------------------------------------
// Feature 2: nofail mode
// ---------------------------------------------------------------------------

#[test]
fn test_nofail_false_propagates_error() {
    use endf::records::{self, ContRecord, CtrlRecord};
    use endf::options::WriteOpts;

    let write_opts = WriteOpts::default();
    let ctrl = CtrlRecord { mat: 125, mf: 3, mt: 1 };

    let head_rec = ContRecord { c1: 26056.0, c2: 55.845, l1: 0, l2: 0, n1: 0, n2: 0 };
    let head_line = records::write_head(&head_rec, &ctrl, &write_opts);

    // NR=99, NP=99 but no body lines -> will cause engine error
    let tab1_head = ContRecord { c1: 0.0, c2: 0.0, l1: 0, l2: 0, n1: 99, n2: 99 };
    let tab1_head_line = records::write_cont(&tab1_head, &ctrl, &write_opts);

    let send = records::write_send(125, 3, &write_opts);
    let fend = records::write_fend(125, &write_opts);
    let mend = records::write_mend(&write_opts);
    let tend = records::write_tend(&write_opts);

    let all_lines = vec![head_line, tab1_head_line, send, fend, mend, tend];
    let endf_text = all_lines.join("\n");

    // With nofail=false (default), parse should fail
    let parser = EndfParser::builder()
        .ignore_missing_tpid(true)
        .ignore_send_records(true)
        .nofail(false)
        .build()
        .expect("parser");

    let result = parser.parse(&endf_text);
    assert!(result.is_err(), "nofail=false should propagate section parse errors");
}

#[test]
fn test_nofail_true_stores_raw_on_failure() {
    use endf::records::{self, ContRecord, CtrlRecord};
    use endf::options::WriteOpts;

    let write_opts = WriteOpts::default();
    let ctrl = CtrlRecord { mat: 125, mf: 3, mt: 1 };

    let head_rec = ContRecord { c1: 26056.0, c2: 55.845, l1: 0, l2: 0, n1: 0, n2: 0 };
    let head_line = records::write_head(&head_rec, &ctrl, &write_opts);

    let tab1_head = ContRecord { c1: 0.0, c2: 0.0, l1: 0, l2: 0, n1: 99, n2: 99 };
    let tab1_head_line = records::write_cont(&tab1_head, &ctrl, &write_opts);

    let send = records::write_send(125, 3, &write_opts);
    let fend = records::write_fend(125, &write_opts);
    let mend = records::write_mend(&write_opts);
    let tend = records::write_tend(&write_opts);

    let all_lines = vec![head_line, tab1_head_line, send, fend, mend, tend];
    let endf_text = all_lines.join("\n");

    // With nofail=true, parse should succeed and store raw string
    let parser = EndfParser::builder()
        .ignore_missing_tpid(true)
        .ignore_send_records(true)
        .nofail(true)
        .build()
        .expect("parser");

    let result = parser.parse(&endf_text).expect("nofail=true should not fail");
    let section = result.get_path("3/1").expect("section 3/1 should exist");
    assert!(section.as_str().is_some(), "failed section should be stored as raw string");
}

// ---------------------------------------------------------------------------
// Feature 3: CRLF line endings
// ---------------------------------------------------------------------------

#[test]
fn test_crlf_line_endings() {
    use endf::records::{self, ContRecord, CtrlRecord, Tab1Body};
    use endf::options::WriteOpts;

    let write_opts = WriteOpts::default();
    let ctrl = CtrlRecord { mat: 125, mf: 3, mt: 1 };

    let head_rec = ContRecord { c1: 26056.0, c2: 55.845, l1: 0, l2: 0, n1: 0, n2: 0 };
    let head_line = records::write_head(&head_rec, &ctrl, &write_opts);

    let tab1_head = ContRecord { c1: 0.0, c2: 0.0, l1: 0, l2: 0, n1: 1, n2: 3 };
    let tab1_head_line = records::write_cont(&tab1_head, &ctrl, &write_opts);

    let tab1_body = Tab1Body {
        nbt: vec![3], int: vec![2],
        x: vec![1.0e-5, 1.0e6, 2.0e7],
        y: vec![10.0, 20.0, 5.0],
    };
    let body_lines = records::write_tab1_body(&tab1_body, &ctrl, &write_opts);

    let send = records::write_send(125, 3, &write_opts);
    let fend = records::write_fend(125, &write_opts);
    let mend = records::write_mend(&write_opts);
    let tend = records::write_tend(&write_opts);

    let mut all_lines = vec![head_line, tab1_head_line];
    all_lines.extend(body_lines);
    all_lines.push(send);
    all_lines.push(fend);
    all_lines.push(mend);
    all_lines.push(tend);

    // Join with LF first to parse reference
    let lf_text = all_lines.join("\n");

    // Join with CRLF
    let crlf_text = all_lines.join("\r\n");

    let parser = EndfParser::builder()
        .ignore_missing_tpid(true)
        .ignore_send_records(true)
        .nofail(true)
        .build()
        .expect("parser");

    let data_lf = parser.parse(&lf_text).expect("LF parse");
    let data_crlf = parser.parse(&crlf_text).expect("CRLF parse");

    // Both should produce the same parsed section
    let section_lf = data_lf.get_path("3/1").expect("3/1 from LF");
    let section_crlf = data_crlf.get_path("3/1").expect("3/1 from CRLF");

    assert_eq!(
        section_lf.get("ZA").unwrap().as_float(),
        section_crlf.get("ZA").unwrap().as_float(),
    );
    assert_eq!(
        section_lf.get("AWR").unwrap().as_float(),
        section_crlf.get("AWR").unwrap().as_float(),
    );
    assert_eq!(
        section_lf.get("MAT").unwrap().as_int(),
        section_crlf.get("MAT").unwrap().as_int(),
    );
}
