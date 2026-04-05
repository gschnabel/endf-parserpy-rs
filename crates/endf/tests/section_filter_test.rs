//! Integration tests for `SectionFilter` / `MfMtSelector` and the
//! `*_filtered` parse/write methods on `EndfParser`.
//!
//! Semantics mirror Python's `exclude=` / `include=` kwargs on
//! `EndfParserPy.parse` and `EndfParserPy.write`:
//!
//! * Parse side: filtered sections come back as raw `EndfValue::Str`
//!   line blocks, the same representation used for sections lacking a
//!   recipe. Non-filtered sections are still recipe-parsed normally.
//! * Write side: filtered sections are omitted from the output entirely.
//!   FEND is only emitted for an MF that had at least one MT actually
//!   written (so excluding every MT of an MF leaves no dangling FEND).
//!
//! The test input is built in-memory from hand-constructed EndfValue
//! dicts — no external ENDF fixture files are required, so this test
//! runs anywhere the crate compiles.

use endf::parser::{EndfParser, MfMtSelector, SectionFilter};
use endf::value::{EndfKey, EndfValue};

// ---------------------------------------------------------------------------
// Test fixture construction
// ---------------------------------------------------------------------------

/// Build a minimal MF=3 cross-section section: HEAD + TAB1 with a
/// two-point linear table. All required recipe variables are present.
fn make_mf3_section(mat: i32, mt: i32) -> EndfValue {
    let mut datadic = EndfValue::new_dict();
    datadic.insert("MAT", EndfValue::Int(mat as i64));
    datadic.insert("MF", EndfValue::Int(3));
    datadic.insert("MT", EndfValue::Int(mt as i64));
    datadic.insert("ZA", EndfValue::Float(26056.0));
    datadic.insert("AWR", EndfValue::Float(55.845));
    datadic.insert("QM", EndfValue::Float(0.0));
    datadic.insert("QI", EndfValue::Float(0.0));
    datadic.insert("LR", EndfValue::Int(0));
    datadic.insert("NR", EndfValue::Int(1));
    datadic.insert("NP", EndfValue::Int(2));

    let mut xstable = EndfValue::new_dict();
    xstable.insert("NBT", EndfValue::List(vec![Some(EndfValue::Int(2))]));
    xstable.insert("INT", EndfValue::List(vec![Some(EndfValue::Int(2))]));
    xstable.insert(
        "E",
        EndfValue::List(vec![
            Some(EndfValue::Float(1.0e-5)),
            Some(EndfValue::Float(2.0e7)),
        ]),
    );
    // Give each MT a distinguishable cross-section magnitude so the
    // round-tripped parsed values can be checked against the original.
    let xs_magnitude = mt as f64 * 10.0;
    xstable.insert(
        "xs",
        EndfValue::List(vec![
            Some(EndfValue::Float(xs_magnitude)),
            Some(EndfValue::Float(xs_magnitude + 1.0)),
        ]),
    );
    datadic.insert("xstable", xstable);
    datadic
}

/// Build a multi-MF ENDF dict usable by both parse and write filter tests:
/// - MF=0/MT=0: TPID string
/// - MF=3/MT=1: total cross section
/// - MF=3/MT=2: elastic cross section
const TEST_MAT: i32 = 125;

fn make_fixture() -> EndfValue {
    // MF=0 / MT=0 (TPID)
    let mut tpid_section = EndfValue::new_dict();
    tpid_section.insert(
        EndfKey::Str("TPID".into()),
        EndfValue::Str(
            "  SECTION_FILTER_TEST  125                                              "
                .into(),
        ),
    );
    let mut mf0 = EndfValue::new_dict();
    mf0.insert(EndfKey::Int(0), tpid_section);

    // MF=3 / MT=1 and MT=2
    let mut mf3 = EndfValue::new_dict();
    mf3.insert(EndfKey::Int(1), make_mf3_section(TEST_MAT, 1));
    mf3.insert(EndfKey::Int(2), make_mf3_section(TEST_MAT, 2));

    let mut data = EndfValue::new_dict();
    data.insert(EndfKey::Int(0), mf0);
    data.insert(EndfKey::Int(3), mf3);
    data
}

fn default_parser() -> EndfParser {
    EndfParser::builder().build().expect("default parser build")
}

/// Parser used to reparse write-side filter outputs for structural
/// verification. Filters can legitimately strip the TPID (e.g., an
/// `include` that doesn't mention MF=0), so the reparser must tolerate
/// a missing TPID — otherwise we'd be testing `split_sections` strictness
/// rather than the filter itself.
fn permissive_parser() -> EndfParser {
    EndfParser::builder()
        .ignore_missing_tpid(true)
        .build()
        .expect("permissive parser build")
}

// ---------------------------------------------------------------------------
// SectionFilter unit tests
// ---------------------------------------------------------------------------

/// Mirrors Python's `EndfParserPy.should_skip_section`: when `exclude`
/// is non-`None`, `include` is ignored entirely. This is a dedicated
/// unit test because the quirk is easy to get wrong (it's branching
/// on `is_some`, not on emptiness), and the parse/write integration
/// tests below would not catch a regression if the bug were symmetric
/// across exclude and include.
#[test]
fn section_filter_exclude_wins_over_include() {
    let filter = SectionFilter {
        exclude: Some(vec![MfMtSelector::Mf(3)]),
        include: Some(vec![MfMtSelector::MfMt(3, 1)]),
    };
    // exclude wins: MF=3 is skipped regardless of what include says.
    assert!(filter.should_skip(3, 1));
    assert!(filter.should_skip(3, 2));
    // Non-matching MFs: exclude doesn't match, and because exclude is
    // Some, include is ignored → skip is false.
    assert!(!filter.should_skip(1, 451));
}

#[test]
fn section_filter_empty_exclude_still_shadows_include() {
    // Degenerate case: exclude is Some but empty. Per Python's
    // `exclude is None` branch, include is still ignored and nothing
    // is skipped.
    let filter = SectionFilter {
        exclude: Some(vec![]),
        include: Some(vec![MfMtSelector::MfMt(3, 1)]),
    };
    assert!(!filter.should_skip(3, 1));
    assert!(!filter.should_skip(3, 2));
    assert!(!filter.should_skip(1, 451));
}

#[test]
fn section_filter_default_is_noop() {
    let filter = SectionFilter::default();
    assert!(!filter.should_skip(1, 451));
    assert!(!filter.should_skip(3, 1));
    assert!(!filter.should_skip(0, 0));
}

#[test]
fn section_filter_include_skips_unlisted() {
    let filter = SectionFilter::including([MfMtSelector::MfMt(3, 1)]);
    assert!(!filter.should_skip(3, 1));
    assert!(filter.should_skip(3, 2));
    assert!(filter.should_skip(1, 451));
}

#[test]
fn section_filter_mf_selector_matches_all_mts() {
    let filter = SectionFilter::excluding([MfMtSelector::Mf(3)]);
    assert!(filter.should_skip(3, 1));
    assert!(filter.should_skip(3, 2));
    assert!(filter.should_skip(3, 18));
    assert!(!filter.should_skip(1, 451));
    assert!(!filter.should_skip(4, 2));
}

// ---------------------------------------------------------------------------
// Parse-side filter tests
// ---------------------------------------------------------------------------

/// `parse_filtered` with `exclude = [Mf(3)]`: MF=3 sections must come
/// back as raw `EndfValue::Str` blocks, while any other MF must still
/// be recipe-parsed.
#[test]
fn parse_filtered_exclude_whole_mf() {
    let parser = default_parser();
    let fixture = make_fixture();

    // Serialise the fixture once (unfiltered) so we have a real ENDF
    // text payload to feed into the filtered parser.
    let text = parser.write(&fixture).expect("baseline write");

    let filter = SectionFilter::excluding([MfMtSelector::Mf(3)]);
    let result = parser
        .parse_filtered(&text, &filter)
        .expect("parse_filtered should succeed");

    // MF=0 must be parsed normally (no filter applied).
    let mf0 = result.get(EndfKey::Int(0)).expect("MF0 present");
    assert!(mf0.is_dict(), "MF0 must be a dict (TPID branch)");

    // MF=3 MT=1 and MT=2 must both be raw strings.
    let mf3 = result
        .get(EndfKey::Int(3))
        .expect("MF3 present")
        .as_dict()
        .expect("MF3 is a dict");
    for mt in [1_i64, 2_i64] {
        let section = mf3
            .get(&EndfKey::Int(mt))
            .unwrap_or_else(|| panic!("MF3/MT{} present", mt));
        match section {
            EndfValue::Str(raw) => {
                // Sanity: the raw block must at least contain the MT column.
                assert!(
                    !raw.is_empty(),
                    "MF3/MT{} raw block should not be empty",
                    mt
                );
            }
            other => panic!(
                "MF3/MT{} expected EndfValue::Str after filter-exclude, got {:?}",
                mt, other
            ),
        }
    }
}

/// `parse_filtered` with `include = [MfMt(3, 1)]`: only MF=3/MT=1 must
/// be recipe-parsed. MT=2 (same MF, different MT) must come back raw.
/// MF=0 is NOT in the include list, so it must also come back raw.
#[test]
fn parse_filtered_include_single_section() {
    let parser = default_parser();
    let fixture = make_fixture();
    let text = parser.write(&fixture).expect("baseline write");

    let filter = SectionFilter::including([MfMtSelector::MfMt(3, 1)]);
    let result = parser
        .parse_filtered(&text, &filter)
        .expect("parse_filtered should succeed");

    // MF=3/MT=1: parsed, must have recognisable recipe-populated keys.
    let mt1 = result
        .get(EndfKey::Int(3))
        .and_then(|mf| mf.get(EndfKey::Int(1)))
        .expect("MF3/MT1 present");
    assert!(mt1.is_dict(), "MF3/MT1 must be Dict after include filter");
    assert_eq!(
        mt1.get("ZA").and_then(|v| v.as_float()),
        Some(26056.0),
        "MF3/MT1 should carry its parsed ZA field"
    );

    // MF=3/MT=2: NOT in include → raw.
    let mt2 = result
        .get(EndfKey::Int(3))
        .and_then(|mf| mf.get(EndfKey::Int(2)))
        .expect("MF3/MT2 present");
    assert!(
        matches!(mt2, EndfValue::Str(_)),
        "MF3/MT2 must be Str when include=[MfMt(3,1)]"
    );

    // MF=0/MT=0: NOT in include → raw.
    let mf0_mt0 = result
        .get(EndfKey::Int(0))
        .and_then(|mf| mf.get(EndfKey::Int(0)))
        .expect("MF0/MT0 present");
    assert!(
        matches!(mf0_mt0, EndfValue::Str(_)),
        "MF0/MT0 must be Str when include=[MfMt(3,1)] (TPID not in include)"
    );
}

// ---------------------------------------------------------------------------
// Write-side filter tests
// ---------------------------------------------------------------------------

/// `write_filtered` with `exclude = [Mf(3)]`: MF=3 must be absent from
/// the output entirely, and (because it had no emitted MTs) the MF=3
/// FEND must also be absent. We verify by reparsing the filtered text
/// unfiltered and checking structural absence.
#[test]
fn write_filtered_exclude_whole_mf_omits_mf_and_its_fend() {
    let parser = default_parser();
    let fixture = make_fixture();

    let filter = SectionFilter::excluding([MfMtSelector::Mf(3)]);
    let filtered_text = parser
        .write_filtered(&fixture, &filter)
        .expect("write_filtered should succeed");

    // Structural absence via reparse: MF=3 key must not appear at all.
    // Use the permissive reparser — the filtered output may lack
    // structural trailers (MEND) when every material is excluded.
    let reparsed = permissive_parser()
        .parse(&filtered_text)
        .expect("reparse of filtered output");
    assert!(
        reparsed.get(EndfKey::Int(3)).is_none(),
        "MF=3 must not appear in the filtered-reparsed tree"
    );
    // MF=0 should still be present.
    assert!(reparsed.get(EndfKey::Int(0)).is_some(), "MF=0 should survive filter");

    // Raw-text check: no MF=3 FEND record. An MF=3 FEND line has the
    // ctrl pattern "  {mat:4d}  0  0" with mat in columns 67-70 and
    // "  0" in the MF column 71-72. A simpler and stricter check: the
    // filtered output should contain exactly one FEND line (for no MFs
    // other than 3, since the fixture only has MF=0 and MF=3, and MF=0
    // has no FEND) — i.e., zero FEND lines total. Python and Rust both
    // use the pattern "<mat> 0  0" at the ctrl columns for FEND, which
    // differs from SEND ("<mat> <mf>  0") and MEND ("  0 0  0").
    //
    // For simplicity we just verify the full file reparses cleanly and
    // that no MF=3 line exists at all. A raw byte search is additional
    // defence-in-depth.
    let mat_str = format!("{:>4}", TEST_MAT); // "125 " → " 125"
    let mf3_ctrl_marker = format!("{} 3", mat_str);
    assert!(
        !filtered_text.contains(&mf3_ctrl_marker),
        "filtered output must not contain any 'MAT  3' control column \
         (FEND, SEND or data lines under MF=3):\n{}",
        filtered_text
    );
}

/// `write_filtered` with `include = [MfMt(3, 1)]`: only MF=3/MT=1
/// should appear in the output. MF=3/MT=2 must be omitted, and MF=0
/// must also be omitted (it's not in the include list).
#[test]
fn write_filtered_include_single_section_omits_everything_else() {
    let parser = default_parser();
    let fixture = make_fixture();

    let filter = SectionFilter::including([MfMtSelector::MfMt(3, 1)]);
    let filtered_text = parser
        .write_filtered(&fixture, &filter)
        .expect("write_filtered should succeed");

    // The include filter leaves only MF=3/MT=1, which means the output
    // has no TPID and needs the permissive reparser.
    let reparsed = permissive_parser()
        .parse(&filtered_text)
        .expect("reparse of filtered output");

    // MF=0 should NOT be present (excluded by include filter).
    assert!(
        reparsed.get(EndfKey::Int(0)).is_none()
            || reparsed
                .get(EndfKey::Int(0))
                .and_then(|m| m.as_dict())
                .map(|d| d.is_empty())
                .unwrap_or(true),
        "MF=0 must be absent (or empty) when include=[MfMt(3,1)]"
    );

    // MF=3 must be present with only MT=1.
    let mf3 = reparsed
        .get(EndfKey::Int(3))
        .and_then(|v| v.as_dict())
        .expect("MF3 present");
    assert!(
        mf3.contains_key(&EndfKey::Int(1)),
        "MF3/MT1 must be in filtered output"
    );
    assert!(
        !mf3.contains_key(&EndfKey::Int(2)),
        "MF3/MT2 must NOT be in filtered output when include=[MfMt(3,1)]"
    );

    // And the surviving MT=1 must have the expected round-tripped data.
    let mt1 = mf3.get(&EndfKey::Int(1)).unwrap();
    assert_eq!(
        mt1.get("ZA").and_then(|v| v.as_float()),
        Some(26056.0),
        "MT1 ZA must round-trip through the filtered write"
    );
}

/// Regression test for the "FEND only when at least one MT emitted" fix:
/// if every MT under an MF is filtered out, the MF's FEND record must
/// also be suppressed. Without this fix, the dangling FEND would be
/// emitted before the MEND/TEND trailers and corrupt the output
/// structure on reparse.
#[test]
fn write_filtered_suppresses_fend_when_all_mts_excluded() {
    let parser = default_parser();
    let fixture = make_fixture();

    // Exclude every MT under MF=3 individually (not via Mf(3)) to pin
    // the exact "both MTs filtered, MF nominally still present in the
    // input dict" case.
    let filter = SectionFilter::excluding([
        MfMtSelector::MfMt(3, 1),
        MfMtSelector::MfMt(3, 2),
    ]);
    let filtered_text = parser
        .write_filtered(&fixture, &filter)
        .expect("write_filtered should succeed");

    // The filtered output must reparse cleanly; a stray FEND between
    // the last emitted section and the MEND/TEND would usually trip
    // `split_sections`. Use the permissive reparser: with every
    // material excluded, the output collapses to TPID + TEND (MEND is
    // suppressed because no material was written).
    let reparsed = permissive_parser()
        .parse(&filtered_text)
        .expect("reparse of fully-MF3-filtered output");
    assert!(
        reparsed.get(EndfKey::Int(3)).is_none(),
        "MF3 must not reappear after filtering out every MT"
    );
    // MF=0 should still be present.
    assert!(reparsed.get(EndfKey::Int(0)).is_some());
}
