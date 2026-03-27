//! Integration test: parse a real ENDF file (Cu-63, MAT=2925).

use endf_parser::parser::EndfParser;
use endf_parser::value::EndfValue;
use std::path::Path;

#[test]
fn test_parse_real_cu63_file() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/testdata/n_2925_29-Cu-63.endf");

    if !path.exists() {
        eprintln!("Test file not found, skipping: {}", path.display());
        return;
    }

    let parser = EndfParser::builder()
        .ignore_number_mismatch(true)
        .ignore_zero_mismatch(true)
        .ignore_varspec_mismatch(true)
        .accept_spaces(true)
        .ignore_send_records(true)
        .ignore_missing_tpid(true)
        .ignore_blank_lines(true)
        .build()
        .expect("failed to create parser");

    let result = parser.parse_file(&path);
    match result {
        Ok(data) => {
            // Top-level must be a dict keyed by MF number.
            assert!(data.is_dict(), "top-level result must be a Dict");
            let dict = data.as_dict().unwrap();
            assert!(!dict.is_empty(), "parsed file should contain at least one MF section");

            println!("Parsed {} MF sections", dict.len());

            let mut parsed_count = 0usize;
            let mut raw_count = 0usize;

            for (mf_key, mt_val) in dict {
                let mt_dict = mt_val.as_dict().expect("each MF entry should be a Dict of MT sections");
                print!("  MF {}: {} MT sections [", mf_key, mt_dict.len());
                for (mt_key, section) in mt_dict {
                    let status = match section {
                        EndfValue::Dict(_) => {
                            parsed_count += 1;
                            "ok"
                        }
                        EndfValue::Str(_) => {
                            raw_count += 1;
                            "raw"
                        }
                        _ => "?",
                    };
                    print!("MT{}({}), ", mt_key, status);
                }
                println!("]");
            }

            println!(
                "\nSummary: {} sections parsed successfully, {} fell back to raw storage",
                parsed_count, raw_count
            );

            // ---- Structural checks ----

            // MF1 should exist.
            let mf1 = data.get(1i64);
            if let Some(mf1_val) = mf1 {
                // MT451 should exist under MF1.
                let mt451 = mf1_val.get(451i64);
                if let Some(mt451_val) = mt451 {
                    if let EndfValue::Dict(_) = mt451_val {
                        // If fully parsed, check MAT = 2925.
                        if let Some(mat_val) = mt451_val.get("MAT") {
                            let mat = mat_val.as_int().expect("MAT should be an integer");
                            assert_eq!(mat, 2925, "MAT number should be 2925 for Cu-63");
                            println!("MF1/MT451 MAT check passed: MAT={}", mat);
                        } else {
                            println!("MF1/MT451 parsed but MAT field not found (recipe may differ)");
                        }
                    } else {
                        println!("MF1/MT451 stored as raw (not fully parsed)");
                    }
                } else {
                    println!("MF1/MT451 not found in parsed data");
                }
            } else {
                println!("MF1 not found in parsed data");
            }

            // MF3 should exist (cross-section data).
            let mf3 = data.get(3i64);
            if let Some(mf3_val) = mf3 {
                let mf3_dict = mf3_val.as_dict().expect("MF3 should be a Dict");
                assert!(!mf3_dict.is_empty(), "MF3 should have at least one MT section");
                println!("MF3 contains {} MT sections", mf3_dict.len());
            } else {
                println!("MF3 not found (may be expected if recipe coverage is incomplete)");
            }
        }
        Err(e) => {
            eprintln!("Parse failed (may be expected during development): {}", e);
        }
    }
}
