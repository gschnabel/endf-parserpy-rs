use std::collections::BTreeMap;

use crate::error::{EndfError, EndfResult};
use crate::options::{ReadOpts, WriteOpts};
use crate::records::{read_ctrl, CtrlRecord};

/// Read the control record from `line`, returning `CtrlRecord{0,0,0}` on
/// any parse failure (mirrors the Python `nofail=True` behaviour).
pub fn read_ctrl_nofail(line: &str, opts: &ReadOpts) -> CtrlRecord {
    read_ctrl(line, opts).unwrap_or_default()
}

/// Validate that a line is a proper SEND record (all six data fields zero
/// and MT == 0).  This mirrors the Python `read_send` check.
fn validate_send_record(line: &str, ofs: usize, opts: &ReadOpts) -> EndfResult<()> {
    // The Python code calls `read_cont` then checks C1..N2 == 0 and MT == 0.
    // Here we only need the control fields (MT check) plus a coarse check
    // that the first 6*width characters are blank or zero.
    let width = opts.width;
    let data_end = width * 6;
    let padded = format!("{:<80}", line);
    let data_part = &padded[..data_end];
    // Each of the six fields must parse as zero (or be blank).
    for i in 0..6 {
        let field = data_part[i * width..(i + 1) * width].trim();
        if field.is_empty() {
            continue; // blank ⇒ zero
        }
        // Try integer first, then float.
        let is_zero = field
            .parse::<i64>()
            .map(|v| v == 0)
            .unwrap_or_else(|_| field.parse::<f64>().map(|v| v == 0.0).unwrap_or(false));
        if !is_zero {
            return Err(EndfError::NotSectionEndMsg {
                message: format!(
                    "field {} is '{}', expected 0 in SEND record at line {}",
                    i + 1,
                    field,
                    ofs
                ),
            });
        }
    }
    let ctrl = read_ctrl(line, opts)?;
    if ctrl.mt != 0 {
        return Err(EndfError::NotSectionEndMsg {
            message: format!(
                "MT={} (expected 0) in SEND record at line {}",
                ctrl.mt, ofs
            ),
        });
    }
    Ok(())
}

/// Split an ENDF file (given as a slice of lines) into a nested map
/// `MF → MT → Vec<line>`.
///
/// The returned lines are the raw ENDF text lines for each section,
/// **excluding** the SEND record that terminates the section.
///
/// The TPID (tape header) is stored under MF=0, MT=0.
///
/// Behaviour is controlled by the flags in [`ReadOpts`]:
/// * `ignore_blank_lines` – skip blank lines instead of raising an error.
/// * `ignore_missing_tpid` – tolerate a first line whose MF/MT are not
///   both 0.
/// * `ignore_send_records` – skip all section-end / file-end / material-end /
///   tape-end records and collect only "regular" data lines.  No structural
///   validation is performed.
pub fn split_sections(
    lines: &[&str],
    opts: &ReadOpts,
) -> EndfResult<BTreeMap<i32, BTreeMap<i32, Vec<String>>>> {
    let ignore_blank_lines = opts.ignore_blank_lines;
    let ignore_send_records = opts.ignore_send_records;
    let ignore_missing_tpid = opts.ignore_missing_tpid;

    let mut ofs: usize = 0;

    // Skip leading blank lines (or error).
    while ofs < lines.len() && lines[ofs].trim().is_empty() {
        if !ignore_blank_lines {
            return Err(EndfError::BlankLine { line: ofs });
        }
        ofs += 1;
    }
    if ofs >= lines.len() {
        return Err(EndfError::UnexpectedEndOfInput { line: ofs });
    }

    let mut mfdic: BTreeMap<i32, BTreeMap<i32, Vec<String>>> = BTreeMap::new();

    // --- TPID handling ---------------------------------------------------
    let th = read_ctrl(lines[ofs], opts)?;
    // `next_ofs` is the first line index the main loop should process.
    let next_ofs: usize;
    if th.mf != 0 || th.mt != 0 {
        if !ignore_missing_tpid {
            return Err(EndfError::UnexpectedControlRecordMsg {
                message: format!(
                    "tape head (TPID) must contain MF=0, MT=0 in control record \
                     but contains MAT={}, MF={}, MT={}.",
                    th.mat, th.mf, th.mt
                ),
            });
        }
        // No valid TPID – the current line is a regular record, so the
        // main loop must process it (i.e. start at `ofs`).
        next_ofs = ofs;
    } else {
        // Valid TPID line – store it under MF=0, MT=0.
        mfdic
            .entry(th.mf)
            .or_default()
            .entry(th.mt)
            .or_default()
            .push(lines[ofs].to_string());
        next_ofs = ofs + 1;
    }

    // sec_level: TAPE=0, MAT=1, MF=2, MT=3; -1 means past TEND
    let mut sec_level: i32 = 0;
    let mut last_mat: i32 = 0;
    let mut last_mf: i32 = 0;
    let mut last_mt: i32 = 0;

    let last_line_idx = lines.len().saturating_sub(1);

    // The Python loop: `while ofs < len(lines)-1: ofs += 1; ...`
    // processes all lines from (initial_ofs+1) through (len-1) inclusive.
    // With our `next_ofs` that translates to next_ofs..=last_line_idx.
    for idx in next_ofs..=last_line_idx {
        ofs = idx;
        let line = lines[ofs];

        if line.trim().is_empty() {
            if sec_level == -1 {
                continue;
            }
            if ignore_blank_lines {
                continue;
            } else {
                return Err(EndfError::BlankLine { line: ofs });
            }
        }

        if sec_level == -1 {
            return Err(EndfError::UnexpectedControlRecordMsg {
                message: "Already encountered Tape End (TEND) record. \
                          Nothing else is allowed to follow afterwards."
                    .into(),
            });
        }

        let d = read_ctrl(line, opts)?;
        let mat = d.mat;
        let mf = d.mf;
        let mt = d.mt;
        let is_regular = mat != 0 && mf != 0 && mt != 0;

        // Consistency checks for regular records.
        if is_regular && !ignore_send_records {
            if sec_level >= 3 && last_mt != mt {
                return Err(EndfError::UnexpectedControlRecordMsg {
                    message: control_error("MT", mt, last_mt, ofs),
                });
            }
            if sec_level >= 2 && last_mf != mf {
                return Err(EndfError::UnexpectedControlRecordMsg {
                    message: control_error("MF", mf, last_mf, ofs),
                });
            }
            if sec_level >= 1 && last_mat != mat {
                return Err(EndfError::UnexpectedControlRecordMsg {
                    message: control_error("MAT", mat, last_mat, ofs),
                });
            }
        }

        if is_regular {
            mfdic
                .entry(mf)
                .or_default()
                .entry(mt)
                .or_default()
                .push(line.to_string());
            sec_level = 3;
            last_mat = mat;
            last_mf = mf;
            last_mt = mt;
            continue;
        }

        if ignore_send_records {
            continue;
        }

        // --- Section-end record handling ---------------------------------
        if sec_level >= 2 && mat != last_mat {
            return Err(EndfError::UnexpectedControlRecordMsg {
                message: send_error("MAT", mat, last_mat, ofs),
            });
        }
        if sec_level == 1 && mat != 0 {
            return Err(EndfError::UnexpectedControlRecordMsg {
                message: send_error("MAT", mat, 0, ofs),
            });
        }
        if sec_level >= 3 && mf != last_mf {
            return Err(EndfError::UnexpectedControlRecordMsg {
                message: send_error("MF", mf, last_mf, ofs),
            });
        }
        if sec_level < 3 && mf != 0 {
            return Err(EndfError::UnexpectedControlRecordMsg {
                message: send_error("MF", mf, 0, ofs),
            });
        }
        if sec_level == 0 && mat != -1 {
            return Err(EndfError::UnexpectedControlRecordMsg {
                message: send_error("MAT", mat, -1, ofs),
            });
        }

        sec_level -= 1;

        // Validate that the data fields are all zero.
        validate_send_record(line, ofs, opts)?;
    }

    if !ignore_send_records {
        if sec_level >= 1 {
            let (sectype, secnum) = match sec_level {
                1 => ("MAT", last_mat),
                2 => ("MF", last_mf),
                _ => ("MT", last_mt),
            };
            return Err(EndfError::UnexpectedEndOfInputMsg {
                message: eof_error(sectype, secnum),
            });
        } else if sec_level == 0 {
            return Err(EndfError::UnexpectedEndOfInputMsg {
                message: "Tape End (TEND) record missing".into(),
            });
        }
    }

    Ok(mfdic)
}

// ---- error-message helpers (mirror the Python closures) -----------------

fn control_error(sectype: &str, secnum: i32, expsecnum: i32, ofs: usize) -> String {
    format!(
        "Currently in {sectype}={expsecnum} section but encountered \
         {sectype}={secnum} in control record of line {ofs}."
    )
}

fn send_error(sectype: &str, secnum: i32, expsecnum: i32, ofs: usize) -> String {
    format!(
        "Expecting a Section End (SEND/FEND/MEND) record with \
         {sectype}={expsecnum} but encountered {sectype}={secnum} \
         in control record of line {ofs}."
    )
}

fn eof_error(sectype: &str, secnum: i32) -> String {
    format!(
        "Reached the End-Of-File but still in an open \
         {sectype}={secnum} section. Required Section End \
         records are missing"
    )
}

// =========================================================================
// Line-number stamping
// =========================================================================

/// Add 5-digit line numbers to every line in `lines`.
///
/// If `mf != 0`, the counter starts at 1 (first line gets `"    1"`);
/// otherwise it starts at 0.  Numbers wrap at 99999.
///
/// The function first truncates each line at position `6*width + 9`
/// (i.e. after the MAT+MF+MT control fields) and then appends the
/// 5-digit line number.
///
/// If `opts.include_linenum` is `false` the lines are only truncated
/// (no number appended).
pub fn add_linenumbers(lines: &mut Vec<String>, mf: i32, opts: &WriteOpts) {
    let linenum_field_start = opts.width * 6 + 9;
    let linenum_width = 5;
    let linenum_max: usize = 99999; // 10^5 - 1
    let start_ofs: usize = if mf != 0 { 1 } else { 0 };

    for (i, line) in lines.iter_mut().enumerate() {
        // Truncate to the control-field boundary.
        let trunc_end = linenum_field_start.min(line.len());
        let mut new_line = line[..trunc_end].to_string();
        // Pad if shorter than expected.
        while new_line.len() < linenum_field_start {
            new_line.push(' ');
        }
        if opts.include_linenum {
            let num = (i % linenum_max) + start_ofs;
            new_line.push_str(&format!("{:>width$}", num, width = linenum_width));
        }
        *line = new_line;
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a standard-width ENDF line (66 data chars + 9 ctrl chars).
    fn make_line(data: &str, mat: i32, mf: i32, mt: i32) -> String {
        let padded_data = format!("{:<66}", data);
        let ctrl = format!("{:>4}{:>2}{:>3}", mat, mf, mt);
        format!("{}{}", padded_data, ctrl)
    }

    /// SEND record: all-blank data, MT=0.
    fn send_line(mat: i32, mf: i32) -> String {
        make_line("", mat, mf, 0)
    }

    /// FEND record: all-blank data, MF=0, MT=0.
    fn fend_line(mat: i32) -> String {
        make_line("", mat, 0, 0)
    }

    /// MEND record.
    fn mend_line() -> String {
        make_line("", 0, 0, 0)
    }

    /// TEND record.
    fn tend_line() -> String {
        make_line("", -1, 0, 0)
    }

    // ---- split_sections -------------------------------------------------

    #[test]
    fn test_split_minimal_file() {
        // TPID + one section (1 data line) + SEND + FEND + MEND + TEND
        let tpid = make_line("TAPE HEADER", 125, 0, 0);
        let data = make_line(" 1.0+0 2.0+0          1          2          3          4", 125, 3, 1);
        let send = send_line(125, 3);
        let fend = fend_line(125);
        let mend = mend_line();
        let tend = tend_line();

        let all: Vec<&str> = vec![&tpid, &data, &send, &fend, &mend, &tend];
        let opts = ReadOpts::default();
        let result = split_sections(&all, &opts).unwrap();

        // TPID stored under MF=0, MT=0
        assert!(result.contains_key(&0));
        assert!(result[&0].contains_key(&0));
        assert_eq!(result[&0][&0].len(), 1);

        // Data line stored under MF=3, MT=1
        assert!(result.contains_key(&3));
        assert!(result[&3].contains_key(&1));
        assert_eq!(result[&3][&1].len(), 1);
        assert!(result[&3][&1][0].contains("1.0+0"));
    }

    #[test]
    fn test_split_sections_multiple_mt() {
        let tpid = make_line("TAPE", 100, 0, 0);
        let d1 = make_line("data1", 100, 3, 1);
        let d2 = make_line("data2", 100, 3, 1);
        let send1 = send_line(100, 3);
        let d3 = make_line("data3", 100, 3, 2);
        let send2 = send_line(100, 3);
        let fend = fend_line(100);
        let mend = mend_line();
        let tend = tend_line();

        let all: Vec<&str> = vec![&tpid, &d1, &d2, &send1, &d3, &send2, &fend, &mend, &tend];
        let opts = ReadOpts::default();
        let result = split_sections(&all, &opts).unwrap();

        assert_eq!(result[&3][&1].len(), 2);
        assert_eq!(result[&3][&2].len(), 1);
    }

    #[test]
    fn test_split_ignore_send_records() {
        // Only data lines, no structural records (except TPID).
        let tpid = make_line("TAPE", 100, 0, 0);
        let d1 = make_line("data1", 100, 3, 1);
        let d2 = make_line("data2", 100, 3, 2);
        let d3 = make_line("data3", 100, 1, 451);

        let all: Vec<&str> = vec![&tpid, &d1, &d2, &d3];
        let opts = ReadOpts {
            ignore_send_records: true,
            ..Default::default()
        };
        let result = split_sections(&all, &opts).unwrap();

        assert_eq!(result[&3][&1].len(), 1);
        assert_eq!(result[&3][&2].len(), 1);
        assert_eq!(result[&1][&451].len(), 1);
    }

    #[test]
    fn test_split_ignore_missing_tpid() {
        let d1 = make_line("data", 100, 3, 1);
        let send = send_line(100, 3);
        let fend = fend_line(100);
        let mend = mend_line();
        let tend = tend_line();

        let all: Vec<&str> = vec![&d1, &send, &fend, &mend, &tend];
        let opts = ReadOpts {
            ignore_missing_tpid: true,
            ..Default::default()
        };
        let result = split_sections(&all, &opts).unwrap();

        assert!(!result.contains_key(&0));
        assert_eq!(result[&3][&1].len(), 1);
    }

    #[test]
    fn test_split_blank_line_error() {
        let tpid = make_line("TAPE", 100, 0, 0);
        let blank = "";
        let tend = tend_line();

        let all: Vec<&str> = vec![&tpid, &blank, &tend];
        let opts = ReadOpts::default();
        let result = split_sections(&all, &opts);
        assert!(result.is_err());
    }

    #[test]
    fn test_split_ignore_blank_lines() {
        let tpid = make_line("TAPE", 100, 0, 0);
        let blank = "";
        let d1 = make_line("data", 100, 3, 1);
        let send = send_line(100, 3);
        let fend = fend_line(100);
        let mend = mend_line();
        let tend = tend_line();

        let all: Vec<&str> = vec![&tpid, &blank, &d1, &send, &fend, &mend, &tend];
        let opts = ReadOpts {
            ignore_blank_lines: true,
            ..Default::default()
        };
        let result = split_sections(&all, &opts).unwrap();
        assert_eq!(result[&3][&1].len(), 1);
    }

    #[test]
    fn test_split_missing_tend_error() {
        let tpid = make_line("TAPE", 100, 0, 0);
        let d1 = make_line("data", 100, 3, 1);
        let send = send_line(100, 3);
        let fend = fend_line(100);
        let mend = mend_line();
        // No TEND!
        let all: Vec<&str> = vec![&tpid, &d1, &send, &fend, &mend];
        let opts = ReadOpts::default();
        let result = split_sections(&all, &opts);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("TEND"));
    }

    #[test]
    fn test_split_after_tend_error() {
        let tpid = make_line("TAPE", 100, 0, 0);
        let d1 = make_line("data", 100, 3, 1);
        let send = send_line(100, 3);
        let fend = fend_line(100);
        let mend = mend_line();
        let tend = tend_line();
        let extra = make_line("extra", 100, 3, 1);

        let all: Vec<&str> = vec![&tpid, &d1, &send, &fend, &mend, &tend, &extra];
        let opts = ReadOpts::default();
        let result = split_sections(&all, &opts);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("TEND"));
    }

    // ---- add_linenumbers ------------------------------------------------

    #[test]
    fn test_add_linenumbers_mf_nonzero() {
        let mut lines = vec![
            make_line("first", 100, 3, 1),
            make_line("second", 100, 3, 1),
            make_line("third", 100, 3, 1),
        ];
        let opts = WriteOpts::default();
        add_linenumbers(&mut lines, 3, &opts);

        // MF != 0 ⇒ starts at 1
        assert!(lines[0].ends_with("    1"));
        assert!(lines[1].ends_with("    2"));
        assert!(lines[2].ends_with("    3"));
    }

    #[test]
    fn test_add_linenumbers_mf_zero() {
        let mut lines = vec![
            make_line("TPID", 100, 0, 0),
            make_line("another", 100, 0, 0),
        ];
        let opts = WriteOpts::default();
        add_linenumbers(&mut lines, 0, &opts);

        // MF == 0 ⇒ starts at 0
        assert!(lines[0].ends_with("    0"));
        assert!(lines[1].ends_with("    1"));
    }

    #[test]
    fn test_add_linenumbers_no_linenum() {
        let mut lines = vec![make_line("data", 100, 3, 1)];
        let opts = WriteOpts {
            include_linenum: false,
            ..Default::default()
        };
        add_linenumbers(&mut lines, 3, &opts);

        // Line should be truncated at 75 chars (66 + 9) with no number.
        assert_eq!(lines[0].len(), 75);
    }

    #[test]
    fn test_add_linenumbers_wrapping() {
        // Verify that line numbers wrap at 99999.
        let base = make_line("d", 1, 1, 1);
        let mut lines: Vec<String> = (0..100_001).map(|_| base.clone()).collect();
        let opts = WriteOpts::default();
        add_linenumbers(&mut lines, 1, &opts);

        // First line: (0 % 99999) + 1 = 1
        assert!(lines[0].ends_with("    1"));
        // Line at index 99998: (99998 % 99999) + 1 = 99999
        assert!(lines[99998].ends_with("99999"));
        // Line at index 99999: (99999 % 99999) + 1 = 0 + 1 = 1
        assert!(lines[99999].ends_with("    1"));
    }
}
