use std::path::Path;

use crate::error::{EndfError, EndfResult};
use crate::interpreter::engine::Engine;
use crate::options::{ArrayType, ParseOpts, ReadOpts, WriteOpts};
use crate::recipe::catalogue::RecipeCatalogue;
use crate::records;
use crate::sections;
use crate::value::{EndfKey, EndfValue};

/// Main ENDF parser. Reads and writes ENDF-6 files using recipe-driven
/// interpretation.
pub struct EndfParser {
    engine: Engine,
}

impl EndfParser {
    /// Create a parser with default settings for ENDF-6.
    pub fn new() -> EndfResult<Self> {
        Self::builder().build()
    }

    /// Create a builder for custom configuration.
    pub fn builder() -> EndfParserBuilder {
        EndfParserBuilder::default()
    }

    /// Parse ENDF text into structured data.
    ///
    /// Returns a nested dictionary: `MF -> MT -> section data`.
    pub fn parse(&self, input: &str) -> EndfResult<EndfValue> {
        // Normalize CRLF line endings to LF
        let normalized;
        let input = if input.contains('\r') {
            normalized = input.replace('\r', "");
            normalized.as_str()
        } else {
            input
        };

        let lines: Vec<&str> = input.lines().collect();
        let section_map = sections::split_sections(&lines, &self.engine.read_opts)?;
        let nofail = self.engine.read_opts.nofail;

        let mut result = EndfValue::new_dict();
        for (mf, mt_map) in &section_map {
            let mut mf_dict = EndfValue::new_dict();
            for (mt, section_lines) in mt_map {
                if self.engine.catalogue.get(*mf, *mt).is_some() {
                    match self.engine.parse_section(*mf, *mt, section_lines.clone()) {
                        Ok(data) => {
                            mf_dict.insert(EndfKey::Int(*mt as i64), data);
                        }
                        Err(e) => {
                            if nofail {
                                // Store unparsed on failure
                                let raw = EndfValue::Str(section_lines.join("\n"));
                                mf_dict.insert(EndfKey::Int(*mt as i64), raw);
                            } else {
                                return Err(e);
                            }
                        }
                    }
                } else {
                    // No recipe: store as raw lines
                    let raw = EndfValue::Str(section_lines.join("\n"));
                    mf_dict.insert(EndfKey::Int(*mt as i64), raw);
                }
            }
            result.insert(EndfKey::Int(*mf as i64), mf_dict);
        }
        Ok(result)
    }

    /// Parse an ENDF file from disk.
    pub fn parse_file(&self, path: &Path) -> EndfResult<EndfValue> {
        let content = std::fs::read_to_string(path)?;
        self.parse(&content)
    }

    /// Parse ENDF text with parallel section parsing.
    ///
    /// Splits the file into MF/MT sections, then parses each section
    /// in parallel using rayon's global thread pool. Configure the pool
    /// before calling:
    ///
    /// ```rust,ignore
    /// rayon::ThreadPoolBuilder::new().num_threads(4).build_global().unwrap();
    /// ```
    ///
    /// If the global pool is not configured, rayon defaults to using all
    /// available CPUs.
    #[cfg(feature = "parallel")]
    pub fn parse_parallel(&self, input: &str) -> EndfResult<EndfValue> {
        use rayon::prelude::*;

        let input = if input.contains('\r') {
            std::borrow::Cow::Owned(input.replace('\r', ""))
        } else {
            std::borrow::Cow::Borrowed(input)
        };

        let lines: Vec<&str> = input.lines().collect();
        let section_map = sections::split_sections(&lines, &self.engine.read_opts)?;

        // Flatten into (mf, mt, section_lines, has_recipe).
        let tasks: Vec<(i32, i32, Vec<String>, bool)> = section_map.iter()
            .flat_map(|(mf, mt_map)| {
                mt_map.iter().map(move |(mt, sl)| {
                    let has_recipe = self.engine.catalogue.get(*mf, *mt).is_some();
                    (*mf, *mt, sl.clone(), has_recipe)
                })
            })
            .collect();

        // Parse sections in parallel using the global rayon pool.
        let parsed: Vec<(i32, i32, EndfValue)> = tasks.par_iter()
            .map(|(mf, mt, sl, has_recipe)| {
                if *has_recipe {
                    match self.engine.parse_section(*mf, *mt, sl.clone()) {
                        Ok(data) => (*mf, *mt, data),
                        Err(_) => (*mf, *mt, EndfValue::Str(sl.join("\n"))),
                    }
                } else {
                    (*mf, *mt, EndfValue::Str(sl.join("\n")))
                }
            })
            .collect();

        // Assemble into nested dict.
        let mut result = EndfValue::new_dict();
        for (mf, mt, data) in parsed {
            if !result.contains_key(EndfKey::Int(mf as i64)) {
                result.insert(EndfKey::Int(mf as i64), EndfValue::new_dict());
            }
            let mf_dict = result.get_mut(EndfKey::Int(mf as i64)).unwrap();
            mf_dict.insert(EndfKey::Int(mt as i64), data);
        }
        Ok(result)
    }

    /// Parse an ENDF file from disk with parallel section parsing.
    ///
    /// Uses rayon's global thread pool. See [`parse_parallel`](Self::parse_parallel).
    #[cfg(feature = "parallel")]
    pub fn parse_file_parallel(&self, path: &Path) -> EndfResult<EndfValue> {
        let content = std::fs::read_to_string(path)?;
        self.parse_parallel(&content)
    }

    /// Write structured data back to ENDF format.
    pub fn write(&self, data: &EndfValue) -> EndfResult<String> {
        let mut all_lines: Vec<String> = Vec::new();
        let mf_dict = data.as_dict().ok_or_else(|| EndfError::RecipeParse {
            message: "top-level value must be a Dict".to_string(),
        })?;

        let mut mat = 0i32;

        // First pass: determine MAT from any parsed section.
        for (_mf_key, mt_dict_val) in mf_dict {
            if let Some(mt_d) = mt_dict_val.as_dict() {
                for (_mt_key, section_data) in mt_d {
                    if let Some(mat_val) = section_data.get("MAT") {
                        if let Some(m) = mat_val.as_int() {
                            if m > 0 { mat = m as i32; break; }
                        }
                    }
                }
            }
            if mat > 0 { break; }
        }

        for (mf_key, mt_dict_val) in mf_dict {
            let mf = match mf_key {
                EndfKey::Int(n) => *n as i32,
                _ => continue,
            };

            // MF0 is the TPID (tape identification) — a single text line
            // with no SEND/FEND records. Handle it specially.
            if mf == 0 {
                if let Some(mt_d) = mt_dict_val.as_dict() {
                    for (_mt_key, section_data) in mt_d {
                        if let Some(EndfValue::Str(ref text)) = section_data.get("TPID") {
                            let ctrl = records::CtrlRecord { mat: 0, mf: 0, mt: 0 };
                            let rec = records::TextRecord { text: text.clone() };
                            let tpid_line = records::write_text(
                                &rec, &ctrl, &self.engine.write_opts,
                            );
                            all_lines.push(tpid_line);
                        } else {
                            // Try writing the section via engine (tapehead recipe)
                            let section_lines =
                                self.engine.write_section(0, 0, section_data.clone())?;
                            // TPID has no line numbers
                            all_lines.extend(section_lines);
                        }
                    }
                }
                continue;
            }

            let mt_dict = mt_dict_val.as_dict().ok_or_else(|| EndfError::RecipeParse {
                message: format!("MF {} value must be a Dict", mf),
            })?;

            for (mt_key, section_data) in mt_dict {
                let mt = match mt_key {
                    EndfKey::Int(n) => *n as i32,
                    _ => continue,
                };

                if let EndfValue::Str(raw) = section_data {
                    // Unparsed section: output raw
                    all_lines.extend(raw.lines().map(String::from));
                } else {
                    // Parsed section: use engine
                    let mut section_lines =
                        self.engine
                            .write_section(mf, mt, section_data.clone())?;
                    sections::add_linenumbers(&mut section_lines, mf, &self.engine.write_opts);
                    all_lines.extend(section_lines);
                }
                // Add SEND
                all_lines.push(records::write_send(mat, mf, &self.engine.write_opts));
            }
            // Add FEND
            all_lines.push(records::write_fend(mat, &self.engine.write_opts));
        }
        // Add MEND and TEND
        all_lines.push(records::write_mend(&self.engine.write_opts));
        all_lines.push(records::write_tend(&self.engine.write_opts));

        Ok(all_lines.join("\n"))
    }

    /// Write structured data to a file.
    pub fn write_file(&self, path: &Path, data: &EndfValue) -> EndfResult<()> {
        let content = self.write(data)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

impl Default for EndfParser {
    fn default() -> Self {
        Self::new().expect("failed to initialize default parser")
    }
}

/// Builder for `EndfParser` with configurable options.
pub struct EndfParserBuilder {
    parse_opts: ParseOpts,
    read_opts: ReadOpts,
    write_opts: WriteOpts,
    recipes_dir: Option<std::path::PathBuf>,
    endf_format: String,
}

impl Default for EndfParserBuilder {
    fn default() -> Self {
        Self {
            parse_opts: ParseOpts::default(),
            read_opts: ReadOpts::default(),
            write_opts: WriteOpts::default(),
            recipes_dir: None,
            endf_format: "endf6".to_string(),
        }
    }
}

impl EndfParserBuilder {
    pub fn ignore_number_mismatch(mut self, v: bool) -> Self {
        self.parse_opts.ignore_number_mismatch = v;
        self
    }
    pub fn ignore_zero_mismatch(mut self, v: bool) -> Self {
        self.parse_opts.ignore_zero_mismatch = v;
        self
    }
    pub fn ignore_varspec_mismatch(mut self, v: bool) -> Self {
        self.parse_opts.ignore_varspec_mismatch = v;
        self
    }
    pub fn fuzzy_matching(mut self, v: bool) -> Self {
        self.parse_opts.fuzzy_matching = v;
        self
    }
    pub fn array_type(mut self, v: ArrayType) -> Self {
        self.parse_opts.array_type = v;
        self
    }
    pub fn accept_spaces(mut self, v: bool) -> Self {
        self.read_opts.accept_spaces = v;
        self
    }
    pub fn preserve_value_strings(mut self, v: bool) -> Self {
        self.read_opts.preserve_value_strings = v;
        self
    }
    pub fn ignore_blank_lines(mut self, v: bool) -> Self {
        self.read_opts.ignore_blank_lines = v;
        self
    }
    pub fn ignore_send_records(mut self, v: bool) -> Self {
        self.read_opts.ignore_send_records = v;
        self
    }
    pub fn ignore_missing_tpid(mut self, v: bool) -> Self {
        self.read_opts.ignore_missing_tpid = v;
        self
    }
    pub fn width(mut self, v: usize) -> Self {
        self.read_opts.width = v;
        self.write_opts.width = v;
        self
    }
    pub fn abuse_signpos(mut self, v: bool) -> Self {
        self.write_opts.abuse_signpos = v;
        self
    }
    pub fn skip_intzero(mut self, v: bool) -> Self {
        self.write_opts.skip_intzero = v;
        self
    }
    pub fn prefer_noexp(mut self, v: bool) -> Self {
        self.write_opts.prefer_noexp = v;
        self
    }
    pub fn keep_e(mut self, v: bool) -> Self {
        self.write_opts.keep_e = v;
        self
    }
    pub fn include_linenum(mut self, v: bool) -> Self {
        self.write_opts.include_linenum = v;
        self
    }
    pub fn strict_datatypes(mut self, v: bool) -> Self {
        self.write_opts.strict_datatypes = v;
        self
    }
    pub fn zero_as_blank(mut self, v: bool) -> Self {
        self.write_opts.zero_as_blank = v;
        self
    }
    pub fn nofail(mut self, v: bool) -> Self {
        self.read_opts.nofail = v;
        self
    }

    /// Load recipes from a directory at runtime instead of using the
    /// compiled-in defaults. This avoids recompilation when recipes change.
    pub fn recipes_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.recipes_dir = Some(dir.into());
        self
    }

    /// Select the ENDF format / recipe flavour.
    ///
    /// Supported values: `"endf6"` (default), `"endf6-ext"`, `"jendl"`,
    /// `"pendf"`, `"errorr"`.
    ///
    /// This setting is ignored when `recipes_dir` is also set (runtime
    /// directory takes precedence).
    pub fn endf_format(mut self, format: &str) -> Self {
        self.endf_format = format.to_string();
        self
    }

    pub fn build(self) -> EndfResult<EndfParser> {
        let catalogue = if let Some(dir) = self.recipes_dir {
            RecipeCatalogue::load_from_dir(&dir)?
        } else {
            RecipeCatalogue::for_format(&self.endf_format)?
        };
        let engine = Engine::new(catalogue, self.parse_opts, self.read_opts, self.write_opts);
        Ok(EndfParser { engine })
    }
}
