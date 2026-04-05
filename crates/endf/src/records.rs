use crate::error::{EndfError, EndfResult};
use crate::fortran::{f64_to_fortstr, fortstr_to_f64, read_fort_floats, read_fort_int};
use crate::options::{ReadOpts, WriteOpts};

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Control record (MAT, MF, MT) extracted from every ENDF line.
#[derive(Clone, Debug, Default)]
pub struct CtrlRecord {
    pub mat: i32,
    pub mf: i32,
    pub mt: i32,
}

/// CONT/HEAD record: 2 floats + 4 integers.
#[derive(Clone, Debug, Default)]
pub struct ContRecord {
    pub c1: f64,
    pub c2: f64,
    pub l1: i64,
    pub l2: i64,
    pub n1: i64,
    pub n2: i64,
}

/// TEXT record: 66-character text field.
#[derive(Clone, Debug, Default)]
pub struct TextRecord {
    pub text: String,
}

/// DIR record: 4 integers (fields 3-6; fields 1-2 are blank).
#[derive(Clone, Debug, Default)]
pub struct DirRecord {
    pub l1: i64,
    pub l2: i64,
    pub n1: i64,
    pub n2: i64,
}

/// INTG record: 2 integers + variable-length integer array.
#[derive(Clone, Debug, Default)]
pub struct IntgRecord {
    pub ii: i64,
    pub jj: i64,
    pub kij: Vec<i64>,
}

/// TAB1 table body: interpolation info + x/y data.
#[derive(Clone, Debug, Default)]
pub struct Tab1Body {
    pub nbt: Vec<i64>,
    pub int: Vec<i64>,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

/// TAB2 table body: interpolation info only.
#[derive(Clone, Debug, Default)]
pub struct Tab2Body {
    pub nbt: Vec<i64>,
    pub int: Vec<i64>,
}

// ---------------------------------------------------------------------------
// Control record helpers
// ---------------------------------------------------------------------------

/// Read the control record (MAT, MF, MT) from an ENDF line.
///
/// MAT occupies positions `[6*width .. 6*width+4]`, MF `[6*width+4 .. 6*width+6]`,
/// MT `[6*width+6 .. 6*width+9]`.
pub fn read_ctrl(line: &str, opts: &ReadOpts) -> EndfResult<CtrlRecord> {
    let ofs = opts.width * 6;
    let padded = format!("{:<80}", line);
    let mat_str = &padded[ofs..ofs + 4];
    let mf_str = &padded[ofs + 4..ofs + 6];
    let mt_str = &padded[ofs + 6..ofs + 9];
    Ok(CtrlRecord {
        mat: if mat_str.trim().is_empty() {
            0
        } else {
            read_fort_int(mat_str)? as i32
        },
        mf: if mf_str.trim().is_empty() {
            0
        } else {
            read_fort_int(mf_str)? as i32
        },
        mt: if mt_str.trim().is_empty() {
            0
        } else {
            read_fort_int(mt_str)? as i32
        },
    })
}

/// Format a control record as a 9-character string (4+2+3).
pub fn write_ctrl(ctrl: &CtrlRecord) -> String {
    format!("{:>4}{:>2}{:>3}", ctrl.mat, ctrl.mf, ctrl.mt)
}

// ---------------------------------------------------------------------------
// CONT / HEAD record
// ---------------------------------------------------------------------------

/// Read a CONT (or HEAD) record: 2 floats + 4 integers + control fields.
pub fn read_cont(line: &str, opts: &ReadOpts) -> EndfResult<(ContRecord, CtrlRecord)> {
    let w = opts.width;
    let padded = format!("{:<80}", line);
    let c1 = fortstr_to_f64(&padded[0..w], opts)?;
    let c2 = fortstr_to_f64(&padded[w..2 * w], opts)?;
    let l1 = read_fort_int(&padded[2 * w..3 * w])?;
    let l2 = read_fort_int(&padded[3 * w..4 * w])?;
    let n1 = read_fort_int(&padded[4 * w..5 * w])?;
    let n2 = read_fort_int(&padded[5 * w..6 * w])?;
    let ctrl = read_ctrl(&padded, opts)?;
    Ok((ContRecord { c1, c2, l1, l2, n1, n2 }, ctrl))
}

/// Write a CONT (or HEAD) record as a single line.
pub fn write_cont(rec: &ContRecord, ctrl: &CtrlRecord, opts: &WriteOpts) -> String {
    let c1 = f64_to_fortstr(rec.c1, opts);
    let c2 = f64_to_fortstr(rec.c2, opts);
    let w = opts.width;
    format!(
        "{}{}{:>w$}{:>w$}{:>w$}{:>w$}{}",
        c1,
        c2,
        rec.l1,
        rec.l2,
        rec.n1,
        rec.n2,
        write_ctrl(ctrl),
        w = w
    )
}

/// Read a HEAD record (identical layout to CONT).
pub fn read_head(line: &str, opts: &ReadOpts) -> EndfResult<(ContRecord, CtrlRecord)> {
    read_cont(line, opts)
}

/// Write a HEAD record (identical layout to CONT).
pub fn write_head(rec: &ContRecord, ctrl: &CtrlRecord, opts: &WriteOpts) -> String {
    write_cont(rec, ctrl, opts)
}

// ---------------------------------------------------------------------------
// TEXT record
// ---------------------------------------------------------------------------

/// Read a TEXT record: the first `6*width` characters form the text field.
pub fn read_text(line: &str, opts: &ReadOpts) -> EndfResult<(TextRecord, CtrlRecord)> {
    let text_width = opts.width * 6;
    let padded = format!("{:<80}", line);
    let text = padded[..text_width].to_string();
    let ctrl = read_ctrl(&padded, opts)?;
    Ok((TextRecord { text }, ctrl))
}

/// Write a TEXT record as a single line.
pub fn write_text(rec: &TextRecord, ctrl: &CtrlRecord, opts: &WriteOpts) -> String {
    let text_width = opts.width * 6;
    let padded_text = format!("{:<width$}", rec.text, width = text_width);
    format!("{}{}", &padded_text[..text_width], write_ctrl(ctrl))
}

// ---------------------------------------------------------------------------
// DIR record
// ---------------------------------------------------------------------------

/// Read a DIR record: fields 1-2 are blank, fields 3-6 are integers.
pub fn read_dir(line: &str, opts: &ReadOpts) -> EndfResult<(DirRecord, CtrlRecord)> {
    let w = opts.width;
    let padded = format!("{:<80}", line);
    let l1 = read_fort_int(&padded[2 * w..3 * w])?;
    let l2 = read_fort_int(&padded[3 * w..4 * w])?;
    let n1 = read_fort_int(&padded[4 * w..5 * w])?;
    let n2 = read_fort_int(&padded[5 * w..6 * w])?;
    let ctrl = read_ctrl(&padded, opts)?;
    Ok((DirRecord { l1, l2, n1, n2 }, ctrl))
}

/// Write a DIR record as a single line.
pub fn write_dir(rec: &DirRecord, ctrl: &CtrlRecord, opts: &WriteOpts) -> String {
    let w = opts.width;
    let blank = " ".repeat(w);
    format!(
        "{}{}{:>w$}{:>w$}{:>w$}{:>w$}{}",
        blank,
        blank,
        rec.l1,
        rec.l2,
        rec.n1,
        rec.n2,
        write_ctrl(ctrl),
        w = w
    )
}

// ---------------------------------------------------------------------------
// INTG record
// ---------------------------------------------------------------------------

/// Read an INTG record.
///
/// `ndigit` controls the digit width (2..=6). Fields II and JJ occupy
/// columns 0-4 and 5-9 respectively; KIJ values follow in fixed-width
/// fields of `ndigit` characters separated by 1-char spacers.
pub fn read_intg(line: &str, ndigit: usize, opts: &ReadOpts) -> EndfResult<(IntgRecord, CtrlRecord)> {
    let padded = format!("{:<80}", line);
    let ii = read_fort_int(&padded[0..5])?;
    let jj = read_fort_int(&padded[5..10])?;
    let step = ndigit + 1;
    let start = if ndigit <= 5 { 11 } else { 10 };
    let field_width = ndigit + 1;
    let mut kij = Vec::new();
    let mut pos = start;
    while pos + field_width <= 66 {
        let s = &padded[pos..pos + field_width];
        if s.trim().is_empty() {
            kij.push(0);
        } else {
            kij.push(read_fort_int(s)?);
        }
        pos += step;
    }
    let ctrl = read_ctrl(&padded, opts)?;
    Ok((IntgRecord { ii, jj, kij }, ctrl))
}

/// Write an INTG record as a single line.
pub fn write_intg(
    rec: &IntgRecord,
    ctrl: &CtrlRecord,
    ndigit: usize,
    _opts: &WriteOpts,
) -> String {
    let field_width = ndigit + 1;
    let mut s = format!("{:>5}{:>5}", rec.ii, rec.jj);
    let spacer = if ndigit <= 5 { " " } else { "" };
    s.push_str(spacer);
    for val in &rec.kij {
        s.push_str(&format!("{:>w$}", val, w = field_width));
    }
    // Pad data portion to 66 characters.
    while s.len() < 66 {
        s.push(' ');
    }
    format!("{}{}", &s[..66], write_ctrl(ctrl))
}

// ---------------------------------------------------------------------------
// Multi-line number reading/writing
// ---------------------------------------------------------------------------

/// Read `count` float values packed 6-per-line starting at line `ofs`.
///
/// Returns the values and the new line offset. Callers that need integers
/// (TAB1/TAB2 interpolation tables) cast the results with `as i64`; there
/// is intentionally no `to_int` mode here because the Rust API keeps the
/// canonical representation as `f64`.
pub fn read_endf_numbers(
    lines: &[&str],
    count: usize,
    ofs: usize,
    opts: &ReadOpts,
) -> EndfResult<(Vec<f64>, usize)> {
    let mut vals = Vec::with_capacity(count);
    let mut current_ofs = ofs;
    let mut remaining = count;
    while remaining > 0 {
        if current_ofs >= lines.len() {
            return Err(EndfError::UnexpectedEndOfInput { line: current_ofs });
        }
        let n = remaining.min(6);
        let mut line_vals = read_fort_floats(lines[current_ofs], n, opts)?;
        vals.append(&mut line_vals);
        remaining -= n;
        current_ofs += 1;
    }
    Ok((vals, current_ofs))
}

/// Write float values packed 6-per-line, each line terminated by a control record.
///
/// When `to_int` is true the values are formatted as right-justified integers
/// instead of Fortran floats.
pub fn write_endf_numbers(
    vals: &[f64],
    ctrl: &CtrlRecord,
    to_int: bool,
    opts: &WriteOpts,
) -> Vec<String> {
    let mut result_lines = Vec::new();
    let w = opts.width;

    for chunk in vals.chunks(6) {
        let mut line = String::new();
        for val in chunk {
            if to_int {
                line.push_str(&format!("{:>w$}", *val as i64, w = w));
            } else {
                line.push_str(&f64_to_fortstr(*val, opts));
            }
        }
        // Pad the last (or only) chunk to full data width.
        let data_width = w * 6;
        while line.len() < data_width {
            line.push_str(&" ".repeat(w));
        }
        line.push_str(&write_ctrl(ctrl));
        result_lines.push(line);
    }
    result_lines
}

/// Interleave two equal-length slices `a` and `b` into a single `Vec<f64>`
/// in the order `a[0], b[0], a[1], b[1], ...`, applying `to_f64` to each
/// element. Used to build the flat interpolation / x-y buffers for TAB1 /
/// TAB2 records. A closure is used instead of `Into<f64>` because `i64`
/// does not implement `Into<f64>` (would be lossy for large values, but
/// ENDF interpolation counts fit comfortably in f64).
#[inline]
fn interleave_pairs<T: Copy>(a: &[T], b: &[T], to_f64: impl Fn(T) -> f64) -> Vec<f64> {
    debug_assert_eq!(a.len(), b.len());
    let mut out = Vec::with_capacity(a.len() * 2);
    for (av, bv) in a.iter().zip(b.iter()) {
        out.push(to_f64(*av));
        out.push(to_f64(*bv));
    }
    out
}

// ---------------------------------------------------------------------------
// TAB1 record
// ---------------------------------------------------------------------------

/// Read the body of a TAB1 record (interpolation table + x/y data).
pub fn read_tab1_body(
    lines: &[&str],
    ofs: usize,
    nr: usize,
    np: usize,
    opts: &ReadOpts,
) -> EndfResult<(Tab1Body, usize)> {
    // Read 2*NR interleaved integers (NBT, INT pairs).
    let (interp_vals, ofs2) = read_endf_numbers(lines, 2 * nr, ofs, opts)?;
    let mut nbt = Vec::with_capacity(nr);
    let mut int = Vec::with_capacity(nr);
    for i in 0..nr {
        nbt.push(interp_vals[2 * i] as i64);
        int.push(interp_vals[2 * i + 1] as i64);
    }
    // Read 2*NP interleaved floats (X, Y pairs).
    let (xy_vals, ofs3) = read_endf_numbers(lines, 2 * np, ofs2, opts)?;
    let mut x = Vec::with_capacity(np);
    let mut y = Vec::with_capacity(np);
    for i in 0..np {
        x.push(xy_vals[2 * i]);
        y.push(xy_vals[2 * i + 1]);
    }
    Ok((Tab1Body { nbt, int, x, y }, ofs3))
}

/// Write the body of a TAB1 record as multiple lines.
pub fn write_tab1_body(body: &Tab1Body, ctrl: &CtrlRecord, opts: &WriteOpts) -> Vec<String> {
    let interleaved_int = interleave_pairs(&body.nbt, &body.int, |v| v as f64);
    let mut lines = write_endf_numbers(&interleaved_int, ctrl, true, opts);
    let interleaved_xy = interleave_pairs(&body.x, &body.y, |v| v);
    lines.extend(write_endf_numbers(&interleaved_xy, ctrl, false, opts));
    lines
}

/// Read a full TAB1 record (header CONT line + body).
pub fn read_tab1(
    lines: &[&str],
    ofs: usize,
    opts: &ReadOpts,
) -> EndfResult<(ContRecord, Tab1Body, CtrlRecord, usize)> {
    let line = lines.get(ofs).ok_or(EndfError::UnexpectedEndOfInput { line: ofs })?;
    let (cont, ctrl) = read_cont(line, opts)?;
    let nr = cont.n1 as usize;
    let np = cont.n2 as usize;
    let (body, new_ofs) = read_tab1_body(lines, ofs + 1, nr, np, opts)?;
    Ok((cont, body, ctrl, new_ofs))
}

// ---------------------------------------------------------------------------
// TAB2 record
// ---------------------------------------------------------------------------

/// Read the body of a TAB2 record (interpolation table only).
pub fn read_tab2_body(
    lines: &[&str],
    ofs: usize,
    nr: usize,
    opts: &ReadOpts,
) -> EndfResult<(Tab2Body, usize)> {
    let (interp_vals, ofs2) = read_endf_numbers(lines, 2 * nr, ofs, opts)?;
    let mut nbt = Vec::with_capacity(nr);
    let mut int = Vec::with_capacity(nr);
    for i in 0..nr {
        nbt.push(interp_vals[2 * i] as i64);
        int.push(interp_vals[2 * i + 1] as i64);
    }
    Ok((Tab2Body { nbt, int }, ofs2))
}

/// Write the body of a TAB2 record as multiple lines.
pub fn write_tab2_body(body: &Tab2Body, ctrl: &CtrlRecord, opts: &WriteOpts) -> Vec<String> {
    let interleaved = interleave_pairs(&body.nbt, &body.int, |v| v as f64);
    write_endf_numbers(&interleaved, ctrl, true, opts)
}

/// Read a full TAB2 record (header CONT line + body).
pub fn read_tab2(
    lines: &[&str],
    ofs: usize,
    opts: &ReadOpts,
) -> EndfResult<(ContRecord, Tab2Body, CtrlRecord, usize)> {
    let line = lines.get(ofs).ok_or(EndfError::UnexpectedEndOfInput { line: ofs })?;
    let (cont, ctrl) = read_cont(line, opts)?;
    let nr = cont.n1 as usize;
    let (body, new_ofs) = read_tab2_body(lines, ofs + 1, nr, opts)?;
    Ok((cont, body, ctrl, new_ofs))
}

// ---------------------------------------------------------------------------
// LIST record
// ---------------------------------------------------------------------------

/// Read a LIST record (header CONT line + N1 float values).
pub fn read_list(
    lines: &[&str],
    ofs: usize,
    opts: &ReadOpts,
) -> EndfResult<(ContRecord, Vec<f64>, CtrlRecord, usize)> {
    let line = lines.get(ofs).ok_or(EndfError::UnexpectedEndOfInput { line: ofs })?;
    let (cont, ctrl) = read_cont(line, opts)?;
    let npl = cont.n1 as usize;
    if npl == 0 {
        return Ok((cont, Vec::new(), ctrl, ofs + 1));
    }
    let (vals, new_ofs) = read_endf_numbers(lines, npl, ofs + 1, opts)?;
    Ok((cont, vals, ctrl, new_ofs))
}

// ---------------------------------------------------------------------------
// Section-boundary detection and writing
// ---------------------------------------------------------------------------

/// Check whether a line is a SEND record (all data fields zero, MT=0).
pub fn is_send(line: &str, opts: &ReadOpts) -> bool {
    if let Ok((cont, ctrl)) = read_cont(line, opts) {
        ctrl.mt == 0
            && cont.c1 == 0.0
            && cont.c2 == 0.0
            && cont.l1 == 0
            && cont.l2 == 0
            && cont.n1 == 0
            && cont.n2 == 0
    } else {
        false
    }
}

/// Helper: build a zero-field string for a single data slot.
fn zero_field(is_float: bool, zero_as_blank: bool, width: usize, opts: &WriteOpts) -> String {
    if zero_as_blank {
        " ".repeat(width)
    } else if is_float {
        f64_to_fortstr(0.0, opts)
    } else {
        format!("{:>w$}", 0, w = width)
    }
}

/// Write a boundary record (SEND/FEND/MEND/TEND): six zero fields plus
/// the control record and an optional line number.
fn write_boundary_record(ctrl: CtrlRecord, linenum: &str, opts: &WriteOpts) -> String {
    let w = opts.width;
    let zf = zero_field(true, opts.zero_as_blank, w, opts);
    let zi = zero_field(false, opts.zero_as_blank, w, opts);
    let ctrl_str = write_ctrl(&ctrl);
    format!("{}{}{}{}{}{}{}{}", zf, zf, zi, zi, zi, zi, ctrl_str, linenum)
}

/// Write a SEND (section-end) record.
pub fn write_send(mat: i32, mf: i32, opts: &WriteOpts) -> String {
    let linenum = if opts.include_linenum { "99999" } else { "" };
    write_boundary_record(CtrlRecord { mat, mf, mt: 0 }, linenum, opts)
}

/// Write a FEND (file-end) record.
pub fn write_fend(mat: i32, opts: &WriteOpts) -> String {
    let linenum = if opts.include_linenum { "    0" } else { "" };
    write_boundary_record(CtrlRecord { mat, mf: 0, mt: 0 }, linenum, opts)
}

/// Write a MEND (material-end) record.
pub fn write_mend(opts: &WriteOpts) -> String {
    let linenum = if opts.include_linenum { "    0" } else { "" };
    write_boundary_record(CtrlRecord { mat: 0, mf: 0, mt: 0 }, linenum, opts)
}

/// Write a TEND (tape-end) record.
pub fn write_tend(opts: &WriteOpts) -> String {
    let linenum = if opts.include_linenum { "    0" } else { "" };
    write_boundary_record(CtrlRecord { mat: -1, mf: 0, mt: 0 }, linenum, opts)
}

/// Check if a line is blank (all whitespace).
pub fn is_blank_line(line: &str) -> bool {
    line.trim().is_empty()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{ReadOpts, WriteOpts};

    fn default_read() -> ReadOpts {
        ReadOpts::default()
    }

    fn default_write() -> WriteOpts {
        WriteOpts::default()
    }

    // ── Control record ───────────────────────────────────────────────

    #[test]
    fn test_read_write_ctrl() {
        let opts = default_read();
        // Build a line with data in columns 66..75
        let mut line = " ".repeat(66);
        line.push_str("1234 5 67");
        line.push_str("     "); // line number
        let ctrl = read_ctrl(&line, &opts).unwrap();
        assert_eq!(ctrl.mat, 1234);
        assert_eq!(ctrl.mf, 5);
        assert_eq!(ctrl.mt, 67);
        let written = write_ctrl(&ctrl);
        assert_eq!(written, "1234 5 67");
    }

    #[test]
    fn test_read_ctrl_blank_fields() {
        let opts = default_read();
        let line = " ".repeat(80);
        let ctrl = read_ctrl(&line, &opts).unwrap();
        assert_eq!(ctrl.mat, 0);
        assert_eq!(ctrl.mf, 0);
        assert_eq!(ctrl.mt, 0);
    }

    // ── CONT record ─────────────────────────────────────────────────

    #[test]
    fn test_cont_roundtrip() {
        let wopts = default_write();
        let ropts = default_read();
        let rec = ContRecord {
            c1: 1.5,
            c2: -3.14,
            l1: 10,
            l2: 20,
            n1: 30,
            n2: 40,
        };
        let ctrl = CtrlRecord {
            mat: 125,
            mf: 3,
            mt: 1,
        };
        let line = write_cont(&rec, &ctrl, &wopts);
        let (rec2, ctrl2) = read_cont(&line, &ropts).unwrap();
        assert!((rec2.c1 - 1.5).abs() < 1e-6);
        assert!((rec2.c2 - (-3.14)).abs() < 1e-4);
        assert_eq!(rec2.l1, 10);
        assert_eq!(rec2.l2, 20);
        assert_eq!(rec2.n1, 30);
        assert_eq!(rec2.n2, 40);
        assert_eq!(ctrl2.mat, 125);
        assert_eq!(ctrl2.mf, 3);
        assert_eq!(ctrl2.mt, 1);
    }

    // ── TEXT record ─────────────────────────────────────────────────

    #[test]
    fn test_text_roundtrip() {
        let wopts = default_write();
        let ropts = default_read();
        let rec = TextRecord {
            text: "Hello ENDF world".to_string(),
        };
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 1,
            mt: 451,
        };
        let line = write_text(&rec, &ctrl, &wopts);
        let (rec2, ctrl2) = read_text(&line, &ropts).unwrap();
        assert!(rec2.text.starts_with("Hello ENDF world"));
        assert_eq!(ctrl2.mat, 100);
        assert_eq!(ctrl2.mf, 1);
        assert_eq!(ctrl2.mt, 451);
    }

    // ── DIR record ──────────────────────────────────────────────────

    #[test]
    fn test_dir_roundtrip() {
        let wopts = default_write();
        let ropts = default_read();
        let rec = DirRecord {
            l1: 1,
            l2: 2,
            n1: 3,
            n2: 4,
        };
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 1,
            mt: 451,
        };
        let line = write_dir(&rec, &ctrl, &wopts);
        let (rec2, ctrl2) = read_dir(&line, &ropts).unwrap();
        assert_eq!(rec2.l1, 1);
        assert_eq!(rec2.l2, 2);
        assert_eq!(rec2.n1, 3);
        assert_eq!(rec2.n2, 4);
        assert_eq!(ctrl2.mat, 100);
    }

    // ── INTG record ─────────────────────────────────────────────────

    #[test]
    fn test_intg_roundtrip() {
        let wopts = default_write();
        let ropts = default_read();
        let rec = IntgRecord {
            ii: 5,
            jj: 10,
            kij: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
        };
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 1,
            mt: 451,
        };
        let line = write_intg(&rec, &ctrl, 5, &wopts);
        let (rec2, _ctrl2) = read_intg(&line, 5, &ropts).unwrap();
        assert_eq!(rec2.ii, 5);
        assert_eq!(rec2.jj, 10);
        // The number of KIJ elements depends on available space.
        for (i, v) in rec.kij.iter().enumerate() {
            if i < rec2.kij.len() {
                assert_eq!(rec2.kij[i], *v);
            }
        }
    }

    // ── Multi-line numbers ──────────────────────────────────────────

    #[test]
    fn test_endf_numbers_roundtrip() {
        let wopts = default_write();
        let ropts = default_read();
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 3,
            mt: 1,
        };
        let vals: Vec<f64> = (1..=14).map(|i| i as f64 * 1.1).collect();
        let lines = write_endf_numbers(&vals, &ctrl, false, &wopts);
        // 14 values -> 3 lines (6+6+2)
        assert_eq!(lines.len(), 3);
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let (parsed, new_ofs) = read_endf_numbers(&line_refs, 14, 0, &ropts).unwrap();
        assert_eq!(new_ofs, 3);
        assert_eq!(parsed.len(), 14);
        for (i, (orig, got)) in vals.iter().zip(parsed.iter()).enumerate() {
            assert!(
                (orig - got).abs() < 1e-4,
                "field {}: orig={}, got={}",
                i,
                orig,
                got
            );
        }
    }

    #[test]
    fn test_endf_numbers_int_roundtrip() {
        let wopts = default_write();
        let ropts = default_read();
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 3,
            mt: 1,
        };
        let vals: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0];
        let lines = write_endf_numbers(&vals, &ctrl, true, &wopts);
        assert_eq!(lines.len(), 2);
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let (parsed, _) = read_endf_numbers(&line_refs, 7, 0, &ropts).unwrap();
        for (orig, got) in vals.iter().zip(parsed.iter()) {
            assert_eq!(*orig, *got);
        }
    }

    // ── TAB1 record ─────────────────────────────────────────────────

    #[test]
    fn test_tab1_roundtrip() {
        let wopts = default_write();
        let ropts = default_read();
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 3,
            mt: 1,
        };
        let cont = ContRecord {
            c1: 0.0,
            c2: 0.0,
            l1: 0,
            l2: 0,
            n1: 1,  // NR
            n2: 3,  // NP
        };
        let body = Tab1Body {
            nbt: vec![3],
            int: vec![2],
            x: vec![1.0, 2.0, 3.0],
            y: vec![10.0, 20.0, 30.0],
        };

        // Build lines: header + body
        let header = write_cont(&cont, &ctrl, &wopts);
        let body_lines = write_tab1_body(&body, &ctrl, &wopts);
        let mut all_lines = vec![header];
        all_lines.extend(body_lines);
        let line_refs: Vec<&str> = all_lines.iter().map(|s| s.as_str()).collect();

        let (cont2, body2, ctrl2, _) = read_tab1(&line_refs, 0, &ropts).unwrap();
        assert_eq!(cont2.n1, 1);
        assert_eq!(cont2.n2, 3);
        assert_eq!(body2.nbt, vec![3]);
        assert_eq!(body2.int, vec![2]);
        assert_eq!(body2.x.len(), 3);
        assert_eq!(body2.y.len(), 3);
        assert!((body2.x[0] - 1.0).abs() < 1e-6);
        assert!((body2.y[2] - 30.0).abs() < 1e-4);
        assert_eq!(ctrl2.mat, 100);
    }

    // ── TAB2 record ─────────────────────────────────────────────────

    #[test]
    fn test_tab2_roundtrip() {
        let wopts = default_write();
        let ropts = default_read();
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 3,
            mt: 1,
        };
        let cont = ContRecord {
            c1: 0.0,
            c2: 0.0,
            l1: 0,
            l2: 0,
            n1: 2,  // NR
            n2: 0,
        };
        let body = Tab2Body {
            nbt: vec![5, 10],
            int: vec![2, 4],
        };

        let header = write_cont(&cont, &ctrl, &wopts);
        let body_lines = write_tab2_body(&body, &ctrl, &wopts);
        let mut all_lines = vec![header];
        all_lines.extend(body_lines);
        let line_refs: Vec<&str> = all_lines.iter().map(|s| s.as_str()).collect();

        let (cont2, body2, _, _) = read_tab2(&line_refs, 0, &ropts).unwrap();
        assert_eq!(cont2.n1, 2);
        assert_eq!(body2.nbt, vec![5, 10]);
        assert_eq!(body2.int, vec![2, 4]);
    }

    // ── LIST record ─────────────────────────────────────────────────

    #[test]
    fn test_list_roundtrip() {
        let wopts = default_write();
        let ropts = default_read();
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 3,
            mt: 1,
        };
        let cont = ContRecord {
            c1: 0.0,
            c2: 0.0,
            l1: 0,
            l2: 0,
            n1: 8,
            n2: 0,
        };
        let vals: Vec<f64> = (1..=8).map(|i| i as f64).collect();

        let header = write_cont(&cont, &ctrl, &wopts);
        let body_lines = write_endf_numbers(&vals, &ctrl, false, &wopts);
        let mut all_lines = vec![header];
        all_lines.extend(body_lines);
        let line_refs: Vec<&str> = all_lines.iter().map(|s| s.as_str()).collect();

        let (cont2, vals2, _, _) = read_list(&line_refs, 0, &ropts).unwrap();
        assert_eq!(cont2.n1, 8);
        assert_eq!(vals2.len(), 8);
        for (orig, got) in vals.iter().zip(vals2.iter()) {
            assert!((orig - got).abs() < 1e-6);
        }
    }

    #[test]
    fn test_list_empty() {
        let ropts = default_read();
        let wopts = default_write();
        let ctrl = CtrlRecord {
            mat: 100,
            mf: 3,
            mt: 1,
        };
        let cont = ContRecord {
            c1: 0.0,
            c2: 0.0,
            l1: 0,
            l2: 0,
            n1: 0,
            n2: 0,
        };
        let header = write_cont(&cont, &ctrl, &wopts);
        let all_lines = vec![header];
        let line_refs: Vec<&str> = all_lines.iter().map(|s| s.as_str()).collect();
        let (_, vals, _, new_ofs) = read_list(&line_refs, 0, &ropts).unwrap();
        assert!(vals.is_empty());
        assert_eq!(new_ofs, 1);
    }

    // ── Section boundary records ────────────────────────────────────

    #[test]
    fn test_is_send() {
        let opts = default_read();
        let wopts = default_write();
        let send_line = write_send(125, 3, &wopts);
        assert!(is_send(&send_line, &opts));
    }

    #[test]
    fn test_write_send_format() {
        let wopts = default_write();
        let line = write_send(125, 3, &wopts);
        // Should contain MAT=125, MF=3, MT=0 in the control area
        let ropts = default_read();
        let ctrl = read_ctrl(&line, &ropts).unwrap();
        assert_eq!(ctrl.mat, 125);
        assert_eq!(ctrl.mf, 3);
        assert_eq!(ctrl.mt, 0);
        // Should end with 99999
        assert!(line.ends_with("99999"));
    }

    #[test]
    fn test_write_fend() {
        let wopts = default_write();
        let line = write_fend(125, &wopts);
        let ropts = default_read();
        let ctrl = read_ctrl(&line, &ropts).unwrap();
        assert_eq!(ctrl.mat, 125);
        assert_eq!(ctrl.mf, 0);
        assert_eq!(ctrl.mt, 0);
    }

    #[test]
    fn test_write_mend() {
        let wopts = default_write();
        let line = write_mend(&wopts);
        let ropts = default_read();
        let ctrl = read_ctrl(&line, &ropts).unwrap();
        assert_eq!(ctrl.mat, 0);
        assert_eq!(ctrl.mf, 0);
        assert_eq!(ctrl.mt, 0);
    }

    #[test]
    fn test_write_tend() {
        let wopts = default_write();
        let line = write_tend(&wopts);
        let ropts = default_read();
        let ctrl = read_ctrl(&line, &ropts).unwrap();
        assert_eq!(ctrl.mat, -1);
        assert_eq!(ctrl.mf, 0);
        assert_eq!(ctrl.mt, 0);
    }

    // ── Blank line detection ────────────────────────────────────────

    #[test]
    fn test_is_blank_line() {
        assert!(is_blank_line(""));
        assert!(is_blank_line("   "));
        assert!(!is_blank_line("  x  "));
    }
}
