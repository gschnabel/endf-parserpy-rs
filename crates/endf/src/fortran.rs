use crate::endf_float::EndfFloat;
use crate::error::{EndfError, EndfResult};
use crate::options::{ReadOpts, WriteOpts};

/// Convert a Fortran-style number string to f64.
///
/// ENDF files often omit the 'E' in scientific notation, e.g. "1.23456+7"
/// means 1.23456e7. This function handles that convention along with
/// blank strings (interpreted as 0.0) and optional space removal.
pub fn fortstr_to_f64(s: &str, opts: &ReadOpts) -> EndfResult<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(0.0);
    }

    // Use a stack buffer (max 24 bytes) to avoid heap allocation.
    // ENDF fields are at most 11 chars; with 'E' insertion, max 12.
    let mut buf = [0u8; 24];
    let mut len = 0usize;
    let mut inserted = false;

    for &b in trimmed.as_bytes() {
        // Skip spaces if accept_spaces is enabled
        if b == b' ' && opts.accept_spaces {
            continue;
        }
        // Insert 'E' before '+'/'-' when preceded by a digit (once only)
        if !inserted && len > 0 && (b == b'+' || b == b'-') && buf[len - 1].is_ascii_digit() {
            buf[len] = b'E';
            len += 1;
            inserted = true;
        }
        if len < buf.len() {
            buf[len] = b;
            len += 1;
        }
    }

    let valstr = unsafe { std::str::from_utf8_unchecked(&buf[..len]) };
    valstr.parse::<f64>().map_err(|_| EndfError::InvalidFloat {
        input: valstr.to_string(),
    })
}

/// Convert a Fortran-style number string to an `EndfFloat`, preserving the original string.
///
/// This is the `preserve_value_strings` counterpart of `fortstr_to_f64`.
/// The parsed numeric value is stored alongside the trimmed original string,
/// enabling lossless roundtrip formatting.
pub fn fortstr_to_endf_float(s: &str, opts: &ReadOpts) -> EndfResult<EndfFloat> {
    let value = fortstr_to_f64(s, opts)?;
    Ok(EndfFloat::new(value, Some(s.trim().to_string())))
}

/// Format an `EndfFloat` back to a fixed-width Fortran string.
///
/// If the `EndfFloat` carries an original string and that string fits (or can
/// be right-justified into) the target width, the original representation is
/// returned verbatim, achieving lossless roundtrip.  Otherwise falls back to
/// `f64_to_fortstr`.
pub fn endf_float_to_fortstr(val: &EndfFloat, opts: &WriteOpts) -> String {
    if let Some(orig) = val.original_string() {
        if orig.len() == opts.width {
            return orig.to_string();
        }
        // right-justify to width
        if orig.len() <= opts.width {
            return format!("{:>width$}", orig, width = opts.width);
        }
    }
    f64_to_fortstr(val.value(), opts)
}

/// Read a Fortran integer from a string field.
///
/// Blank strings are interpreted as 0.
pub fn read_fort_int(s: &str) -> EndfResult<i64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    trimmed.parse::<i64>().map_err(|_| EndfError::InvalidInteger {
        input: trimmed.to_string(),
    })
}

/// Remove leading zeros from the exponent and optionally strip the 'e' character.
///
/// Mirrors `_fortranify_expformstr` from the Python code.
fn fortranify_expformstr(numstr: &str, keep_e: bool) -> String {
    // numstr is lowercase, e.g. "1.234567e+02" or "1.234567e-02"
    let e_pos = match numstr.find('e') {
        Some(p) => p,
        None => return numstr.to_string(),
    };
    let mantissa = &numstr[..e_pos];
    let exp_part = &numstr[e_pos + 1..]; // e.g. "+02" or "-02"
    let sign_char = if exp_part.starts_with('+') || exp_part.starts_with('-') {
        &exp_part[..1]
    } else {
        "+"
    };
    let digits_part = exp_part.trim_start_matches(['+', '-']);
    let stripped = digits_part.trim_start_matches('0');
    let digits = if stripped.is_empty() { "0" } else { stripped };

    if keep_e {
        format!("{}E{}{}", mantissa, sign_char, digits)
    } else {
        format!("{}{}{}", mantissa, sign_char, digits)
    }
}

/// Format a float in scientific (exponential) notation, Fortran-style.
///
/// Follows the ENDF convention of omitting the 'E' character (unless `keep_e`
/// is set) and stripping leading zeros from the exponent.
pub fn float2expformstr(val: f64, opts: &WriteOpts) -> String {
    let width = opts.width;
    let abuse_signpos = opts.abuse_signpos;
    let keep_e = opts.keep_e;

    // Get number of digits in the exponent
    let initial = format!("{:.6e}", val);
    let e_idx = initial.find('e').unwrap();
    let exp_digits_str = &initial[e_idx + 2..]; // skip 'e' and sign
    let mut exp_len: usize = 1;
    for (i, c) in exp_digits_str.chars().enumerate() {
        if c != '0' {
            exp_len = exp_digits_str.len() - i;
            break;
        }
    }

    // Calculate available precision digits after decimal point
    // 4 = sign + leading digit + dot + exponent sign
    let mut prec = width as i32 - exp_len as i32 - 4;
    if abuse_signpos && val >= 0.0 {
        prec += 1;
    }
    if keep_e {
        prec -= 1;
    }
    if prec < 1 {
        prec = 1;
    }

    let numstr = format!("{:.prec$e}", val, prec = prec as usize);
    let numstr = fortranify_expformstr(&numstr, keep_e);
    let numstr_len = numstr.len();

    // Handle overflow cases (e.g. rounding causes exponent to grow)
    let result = if abuse_signpos {
        if numstr_len > width {
            let new_prec = (prec - 1).max(1) as usize;
            let s = format!("{:.prec$e}", val, prec = new_prec);
            fortranify_expformstr(&s, keep_e)
        } else {
            numstr
        }
    } else {
        if numstr_len > width || (val > 0.0 && numstr_len == width) {
            let new_prec = (prec - 1).max(1) as usize;
            let s = format!("{:.prec$e}", val, prec = new_prec);
            fortranify_expformstr(&s, keep_e)
        } else {
            numstr
        }
    };

    format!("{:>width$}", result, width = width)
}

/// Format a float in basic (non-exponential) notation.
///
/// Handles integer values, trailing zero stripping, optional leading zero
/// omission for values < 1, and sign position abuse.
pub fn float2basicnumstr(val: f64, opts: &WriteOpts) -> String {
    let width = opts.width;
    let abuse_signpos = opts.abuse_signpos;
    let skip_intzero = opts.skip_intzero;
    let intpart = val as i64;
    let len_intpart = if intpart == 0 {
        1
    } else {
        intpart.unsigned_abs().to_string().len()
    };
    let is_integer = intpart as f64 == val;

    let numstr = if is_integer {
        if intpart == 0 {
            "0".to_string()
        } else {
            format!("{}", val as i64)
        }
    } else {
        let mut effwidth = width as i32;
        if val < 0.0 || !abuse_signpos {
            effwidth -= 1;
        }
        let should_skip_zero = skip_intzero && intpart == 0;
        if should_skip_zero {
            effwidth += 1;
        }
        // -1 due to the decimal point
        let floatwidth = (effwidth - 1 - len_intpart as i32).max(0) as usize;
        let mut s = format!("{:.prec$}", val, prec = floatwidth);
        if s.contains('.') {
            if should_skip_zero {
                if let Some(dotpos) = s.find('.') {
                    // Remove the character just before the dot (the '0')
                    if dotpos > 0 {
                        s = format!("{}{}", &s[..dotpos - 1], &s[dotpos..]);
                    }
                }
            }
            // Strip trailing zeros, then trailing dot
            s = s.trim_end_matches('0').to_string();
            s = s.trim_end_matches('.').to_string();
            // Handle degenerate cases like "+", "-", ""
            if s == "+" || s == "-" || s.is_empty() {
                s = "0".to_string();
            }
        }
        s
    };

    let numstr = if val >= 0.0 && !abuse_signpos {
        format!(" {}", numstr)
    } else {
        numstr
    };

    format!("{:>width$}", numstr, width = width)
}

/// Master formatting function: choose between exponential and basic format.
///
/// When `prefer_noexp` is set, tries basic format and uses it if it fits
/// within the width and is at least as accurate as the exponential format.
pub fn f64_to_fortstr(val: f64, opts: &WriteOpts) -> String {
    let width = opts.width;
    let valstr_exp = float2expformstr(val, opts);

    if !opts.prefer_noexp {
        return valstr_exp;
    }

    let valstr_basic = float2basicnumstr(val, opts);
    if valstr_basic.len() > width {
        return valstr_exp;
    }

    // Compare accuracy: parse both back and see which is closer
    let read_opts = ReadOpts::default();
    let delta1 = match fortstr_to_f64(valstr_basic.trim(), &read_opts) {
        Ok(v) => (v - val).abs(),
        Err(_) => return valstr_exp,
    };
    let delta2 = match fortstr_to_f64(valstr_exp.trim(), &read_opts) {
        Ok(v) => (v - val).abs(),
        Err(_) => 0.0,
    };

    if delta2 < delta1 {
        return valstr_exp;
    }

    format!("{:>width$}", valstr_basic, width = width)
}

/// Read `n` float values from fixed-width fields in a line.
///
/// Each field is `opts.width` characters wide. Fields are extracted
/// left-to-right and parsed via `fortstr_to_f64`.
pub fn read_fort_floats(line: &str, n: usize, opts: &ReadOpts) -> EndfResult<Vec<f64>> {
    let width = opts.width;
    let mut vals = Vec::with_capacity(n);
    for i in 0..n {
        let start = i * width;
        let end = (start + width).min(line.len());
        let field = if start < line.len() {
            &line[start..end]
        } else {
            ""
        };
        vals.push(fortstr_to_f64(field, opts)?);
    }
    Ok(vals)
}

/// Write float values as concatenated fixed-width fields.
pub fn write_fort_floats(vals: &[f64], opts: &WriteOpts) -> String {
    let mut line = String::with_capacity(vals.len() * opts.width);
    for v in vals {
        line.push_str(&f64_to_fortstr(*v, opts));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{ReadOpts, WriteOpts};

    // ── Parsing tests ──────────────────────────────────────────────

    #[test]
    fn test_fortstr_to_f64_implicit_e_plus() {
        let opts = ReadOpts::default();
        let val = fortstr_to_f64("1.23456+7", &opts).unwrap();
        assert!((val - 1.23456e7).abs() < 1.0, "got {}", val);
    }

    #[test]
    fn test_fortstr_to_f64_implicit_e_minus() {
        let opts = ReadOpts::default();
        let val = fortstr_to_f64("1.23456-7", &opts).unwrap();
        assert!((val - 1.23456e-7).abs() / 1.23456e-7 < 1e-10, "got {}", val);
    }

    #[test]
    fn test_fortstr_to_f64_blank() {
        let opts = ReadOpts::default();
        let val = fortstr_to_f64("", &opts).unwrap();
        assert_eq!(val, 0.0);
    }

    #[test]
    fn test_fortstr_to_f64_blank_spaces() {
        let opts = ReadOpts::default();
        let val = fortstr_to_f64("           ", &opts).unwrap();
        assert_eq!(val, 0.0);
    }

    #[test]
    fn test_fortstr_to_f64_simple() {
        let opts = ReadOpts::default();
        let val = fortstr_to_f64(" 1.5 ", &opts).unwrap();
        assert_eq!(val, 1.5);
    }

    #[test]
    fn test_fortstr_to_f64_negative() {
        let opts = ReadOpts::default();
        let val = fortstr_to_f64("-3.14", &opts).unwrap();
        assert!((val - (-3.14)).abs() < 1e-15);
    }

    #[test]
    fn test_fortstr_to_f64_with_spaces() {
        let opts = ReadOpts::default();
        // Internal spaces removed when accept_spaces = true
        let val = fortstr_to_f64("1 .5", &opts).unwrap();
        assert_eq!(val, 1.5);
    }

    #[test]
    fn test_fortstr_to_f64_no_accept_spaces() {
        let opts = ReadOpts {
            accept_spaces: false,
            ..ReadOpts::default()
        };
        let result = fortstr_to_f64("1 .5", &opts);
        assert!(result.is_err());
    }

    #[test]
    fn test_fortstr_to_f64_explicit_e() {
        let opts = ReadOpts::default();
        let val = fortstr_to_f64("1.5E+02", &opts).unwrap();
        assert_eq!(val, 150.0);
    }

    #[test]
    fn test_fortstr_to_f64_negative_exp() {
        let opts = ReadOpts::default();
        let val = fortstr_to_f64("-1.5-3", &opts).unwrap();
        assert!((val - (-1.5e-3)).abs() < 1e-15);
    }

    // ── Integer parsing tests ──────────────────────────────────────

    #[test]
    fn test_read_fort_int_normal() {
        assert_eq!(read_fort_int("  5").unwrap(), 5);
    }

    #[test]
    fn test_read_fort_int_blank() {
        assert_eq!(read_fort_int("").unwrap(), 0);
    }

    #[test]
    fn test_read_fort_int_blank_spaces() {
        assert_eq!(read_fort_int("     ").unwrap(), 0);
    }

    #[test]
    fn test_read_fort_int_negative() {
        assert_eq!(read_fort_int(" -3").unwrap(), -3);
    }

    #[test]
    fn test_read_fort_int_invalid() {
        assert!(read_fort_int("abc").is_err());
    }

    // ── Python-parity tests for WriteOpts::default() ──────────────

    /// Asserts that `f64_to_fortstr` with the default `WriteOpts` produces
    /// byte-identical output to the Python endf-parserpy reference with its
    /// own defaults. Ground-truth strings were captured from
    /// `endf_parserpy.interpreter.fortran_utils.write_fort_floats` on the
    /// venv installation; any future drift on either side will trip this
    /// test. See chunk P0a in the option-alignment work.
    #[test]
    fn test_f64_to_fortstr_matches_python_defaults() {
        let opts = WriteOpts::default();
        let cases: &[(f64, &str)] = &[
            // Straddles the prefer_noexp / scientific decision:
            (0.12345678_f64, " 1.234568-1"),
            (1.234567e-1_f64, " 1.234567-1"),
            // Negative, scientific form — unaffected by abuse_signpos:
            (-1.23456e7_f64, "-1.234560+7"),
            // Large positive — would use the sign slot if abuse_signpos=true:
            (9.87654321e10_f64, " 9.87654+10"),
            // Trivial integer-valued float:
            (1.0_f64, " 1.000000+0"),
        ];
        for (v, expected) in cases {
            let got = f64_to_fortstr(*v, &opts);
            assert_eq!(
                got, *expected,
                "f64_to_fortstr({}) with defaults: got {:?}, expected {:?} (Python)",
                v, got, expected
            );
            assert_eq!(got.len(), 11, "width must be 11 for {:?}", got);
        }
    }

    // ── Exponential formatting tests ───────────────────────────────

    #[test]
    fn test_expform_basic() {
        let opts = WriteOpts::default(); // width=11, no abuse, no keep_e
        let s = float2expformstr(1.23456e7, &opts);
        assert_eq!(s.len(), 11);
        // Parse it back to verify correctness
        let read_opts = ReadOpts::default();
        let back = fortstr_to_f64(s.trim(), &read_opts).unwrap();
        assert!((back - 1.23456e7).abs() / 1.23456e7 < 1e-5);
    }

    #[test]
    fn test_expform_keep_e() {
        let opts = WriteOpts {
            keep_e: true,
            ..WriteOpts::default()
        };
        let s = float2expformstr(1.5e2, &opts);
        assert!(s.contains('E'), "expected 'E' in: '{}'", s);
        assert_eq!(s.len(), 11);
    }

    #[test]
    fn test_expform_abuse_signpos() {
        let opts_abuse = WriteOpts {
            abuse_signpos: true,
            ..WriteOpts::default()
        };
        let opts_normal = WriteOpts::default();
        let s_abuse = float2expformstr(1.5e2, &opts_abuse);
        let s_normal = float2expformstr(1.5e2, &opts_normal);
        // abuse_signpos for positive values should give more precision
        let trimmed_abuse = s_abuse.trim();
        let trimmed_normal = s_normal.trim();
        assert!(
            trimmed_abuse.len() >= trimmed_normal.len(),
            "abuse: '{}', normal: '{}'",
            trimmed_abuse,
            trimmed_normal
        );
    }

    #[test]
    fn test_expform_negative() {
        let opts = WriteOpts::default();
        let s = float2expformstr(-3.14, &opts);
        assert_eq!(s.len(), 11);
        assert!(s.contains('-'));
    }

    #[test]
    fn test_expform_zero() {
        let opts = WriteOpts::default();
        let s = float2expformstr(0.0, &opts);
        assert_eq!(s.len(), 11);
        let read_opts = ReadOpts::default();
        let back = fortstr_to_f64(s.trim(), &read_opts).unwrap();
        assert_eq!(back, 0.0);
    }

    #[test]
    fn test_expform_large_exponent() {
        // With abuse_signpos=true (default), positive numbers use the sign
        // position for an extra digit, so the result may exceed `width`.
        let opts = WriteOpts::default();
        let s = float2expformstr(1.0e100, &opts);
        assert!(s.len() <= opts.width + 1);
        let read_opts = ReadOpts::default();
        let back = fortstr_to_f64(s.trim(), &read_opts).unwrap();
        assert!((back - 1.0e100).abs() / 1.0e100 < 1e-4);

        // Without abuse_signpos, result must fit in exactly `width`
        let opts_no_abuse = WriteOpts { abuse_signpos: false, ..WriteOpts::default() };
        let s2 = float2expformstr(1.0e100, &opts_no_abuse);
        assert_eq!(s2.len(), opts_no_abuse.width);
    }

    // ── Basic formatting tests ─────────────────────────────────────

    #[test]
    fn test_basicnum_integer() {
        let opts = WriteOpts::default();
        let s = float2basicnumstr(42.0, &opts);
        assert_eq!(s.len(), 11);
        assert_eq!(s.trim(), "42");
    }

    #[test]
    fn test_basicnum_zero() {
        let opts = WriteOpts::default();
        let s = float2basicnumstr(0.0, &opts);
        assert_eq!(s.len(), 11);
        assert_eq!(s.trim(), "0");
    }

    #[test]
    fn test_basicnum_negative() {
        let opts = WriteOpts::default();
        let s = float2basicnumstr(-3.14, &opts);
        assert_eq!(s.len(), 11);
        let read_opts = ReadOpts::default();
        let back = fortstr_to_f64(s.trim(), &read_opts).unwrap();
        assert!((back - (-3.14)).abs() < 1e-10);
    }

    #[test]
    fn test_basicnum_skip_intzero() {
        let opts = WriteOpts {
            skip_intzero: true,
            ..WriteOpts::default()
        };
        let s = float2basicnumstr(0.123, &opts);
        assert_eq!(s.len(), 11);
        // Should not contain "0." but should contain "."
        let trimmed = s.trim();
        assert!(
            trimmed.starts_with('.') || trimmed.starts_with(" ."),
            "expected leading zero omitted, got: '{}'",
            trimmed
        );
    }

    #[test]
    fn test_basicnum_abuse_signpos() {
        let opts = WriteOpts {
            abuse_signpos: true,
            ..WriteOpts::default()
        };
        let s = float2basicnumstr(3.14, &opts);
        assert_eq!(s.len(), 11);
        // Should not have a leading space before the number
        let trimmed = s.trim();
        assert!(
            !trimmed.starts_with(' '),
            "unexpected leading space: '{}'",
            s
        );
    }

    // ── Master formatting tests ────────────────────────────────────

    #[test]
    fn test_fortstr_default() {
        let opts = WriteOpts::default();
        let s = f64_to_fortstr(1.23456e7, &opts);
        assert_eq!(s.len(), 11);
    }

    #[test]
    fn test_fortstr_prefer_noexp_integer() {
        let opts = WriteOpts {
            prefer_noexp: true,
            ..WriteOpts::default()
        };
        let s = f64_to_fortstr(42.0, &opts);
        assert_eq!(s.len(), 11);
        assert_eq!(s.trim(), "42");
    }

    #[test]
    fn test_fortstr_prefer_noexp_small() {
        let opts = WriteOpts {
            prefer_noexp: true,
            ..WriteOpts::default()
        };
        let s = f64_to_fortstr(0.001, &opts);
        assert_eq!(s.len(), 11);
    }

    // ── read_fort_floats / write_fort_floats ───────────────────────

    #[test]
    fn test_read_fort_floats_six() {
        // Generate a proper line using write_fort_floats, then read it back
        let write_opts = WriteOpts::default();
        let original = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let line = write_fort_floats(&original, &write_opts);
        let opts = ReadOpts::default();
        let vals = read_fort_floats(&line, 6, &opts).unwrap();
        assert_eq!(vals.len(), 6);
        for (i, v) in vals.iter().enumerate() {
            let expected = (i + 1) as f64;
            assert!(
                (*v - expected).abs() < 1e-5,
                "field {}: got {}, expected {}",
                i,
                v,
                expected
            );
        }
    }

    #[test]
    fn test_read_fort_floats_blank_fields() {
        let line = "           ";
        let opts = ReadOpts::default();
        let vals = read_fort_floats(line, 1, &opts).unwrap();
        assert_eq!(vals[0], 0.0);
    }

    #[test]
    fn test_read_fort_floats_short_line() {
        // Line shorter than expected: missing fields should be 0.0
        let line = " 1.00000+0";
        let opts = ReadOpts::default();
        let vals = read_fort_floats(line, 3, &opts).unwrap();
        assert!((vals[0] - 1.0).abs() < 1e-5);
        assert_eq!(vals[1], 0.0);
        assert_eq!(vals[2], 0.0);
    }

    #[test]
    fn test_write_fort_floats() {
        let opts = WriteOpts::default();
        let vals = vec![1.0, 2.0, 3.0];
        let line = write_fort_floats(&vals, &opts);
        assert_eq!(line.len(), 33); // 3 * 11
    }

    #[test]
    fn test_roundtrip() {
        let write_opts = WriteOpts::default();
        let read_opts = ReadOpts::default();
        let original = vec![1.23456e7, -3.14159, 0.0, 1.0e-30, 9.99999e99];
        let line = write_fort_floats(&original, &write_opts);
        let parsed = read_fort_floats(&line, original.len(), &read_opts).unwrap();
        for (i, (orig, got)) in original.iter().zip(parsed.iter()).enumerate() {
            if *orig == 0.0 {
                assert_eq!(*got, 0.0, "field {}", i);
            } else {
                let rel_err = (got - orig).abs() / orig.abs();
                assert!(
                    rel_err < 1e-4,
                    "field {}: orig={}, got={}, rel_err={}",
                    i,
                    orig,
                    got,
                    rel_err
                );
            }
        }
    }

    // ── fortranify_expformstr internal tests ────────────────────────

    #[test]
    fn test_fortranify_strip_zeros() {
        assert_eq!(fortranify_expformstr("1.5e+02", false), "1.5+2");
        assert_eq!(fortranify_expformstr("1.5e-09", false), "1.5-9");
        assert_eq!(fortranify_expformstr("1.5e+00", false), "1.5+0");
    }

    #[test]
    fn test_fortranify_keep_e() {
        assert_eq!(fortranify_expformstr("1.5e+02", true), "1.5E+2");
        assert_eq!(fortranify_expformstr("1.5e-09", true), "1.5E-9");
    }

    #[test]
    fn test_fortranify_large_exp() {
        assert_eq!(fortranify_expformstr("1.0e+100", false), "1.0+100");
    }

    // ── EndfFloat roundtrip tests ─────────────────────────────────

    #[test]
    fn test_fortstr_to_endf_float_preserves_string() {
        let opts = ReadOpts::default();
        let ef = fortstr_to_endf_float(" 1.23456+7 ", &opts).unwrap();
        assert!((ef.value() - 1.23456e7).abs() < 1.0);
        assert_eq!(ef.original_string(), Some("1.23456+7"));
    }

    #[test]
    fn test_fortstr_to_endf_float_blank() {
        let opts = ReadOpts::default();
        let ef = fortstr_to_endf_float("           ", &opts).unwrap();
        assert_eq!(ef.value(), 0.0);
        assert_eq!(ef.original_string(), Some(""));
    }

    #[test]
    fn test_endf_float_to_fortstr_returns_original() {
        use crate::endf_float::EndfFloat;
        let opts = WriteOpts::default(); // width=11
        let ef = EndfFloat::new(1.23456e7, Some("1.23456+7".to_string()));
        let s = endf_float_to_fortstr(&ef, &opts);
        assert_eq!(s.len(), 11);
        // Original string is 9 chars, right-justified in width 11
        assert_eq!(s, "  1.23456+7");
    }

    #[test]
    fn test_endf_float_to_fortstr_exact_width() {
        use crate::endf_float::EndfFloat;
        let opts = WriteOpts::default(); // width=11
        // original string is exactly 11 chars, returned verbatim
        let ef = EndfFloat::new(1.23456e7, Some(" 1.23456+7 ".to_string()));
        let s = endf_float_to_fortstr(&ef, &opts);
        assert_eq!(s.len(), 11);
        assert_eq!(s, " 1.23456+7 ");
    }

    #[test]
    fn test_endf_float_to_fortstr_no_original() {
        use crate::endf_float::EndfFloat;
        let opts = WriteOpts::default();
        let ef = EndfFloat::from_value(1.23456e7);
        let s = endf_float_to_fortstr(&ef, &opts);
        assert_eq!(s.len(), 11);
        // Should fall back to f64_to_fortstr
        let expected = f64_to_fortstr(1.23456e7, &opts);
        assert_eq!(s, expected);
    }

    #[test]
    fn test_endf_float_roundtrip_via_field() {
        let read_opts = ReadOpts::default();
        let write_opts = WriteOpts::default();
        let field = " 1.23456+7";
        let ef = fortstr_to_endf_float(field, &read_opts).unwrap();
        let output = endf_float_to_fortstr(&ef, &write_opts);
        assert_eq!(output.len(), 11);
        // The trimmed original "1.23456+7" (9 chars) right-justified in 11 chars
        assert_eq!(output, "  1.23456+7");
        // Parse back and verify value is identical
        let back = fortstr_to_f64(&output, &read_opts).unwrap();
        assert_eq!(back, ef.value());
    }
}
