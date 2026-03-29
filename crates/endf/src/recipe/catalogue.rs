use std::collections::HashMap;

use crate::error::{EndfError, EndfResult};
use super::ast::Recipe;
use super::parse_recipe;

/// A collection of parsed ENDF recipes indexed by (MF, MT).
///
/// Recipes with MT = -1 act as wildcards: they match any MT value
/// within that MF when no exact (mf, mt) entry exists.
pub struct RecipeCatalogue {
    recipes: HashMap<(i32, i32), Recipe>,
}

/// Helper macro to include a recipe file at compile time from the endf6 directory,
/// parse it, and insert it into the recipes map under the given (mf, mt) key.
macro_rules! load_recipe {
    ($recipes:expr, $mf:expr, $mt:expr, $file:expr) => {{
        let text = include_str!(concat!("../../recipes/endf6/", $file));
        let parsed = parse_recipe(text).map_err(|e| EndfError::RecipeParse {
            message: format!("failed to parse {}: {}", $file, e),
        })?;
        $recipes.insert(($mf, $mt), parsed);
    }};
}

/// Helper macro to insert the same parsed recipe under multiple (mf, mt) keys.
/// The recipe text is parsed once and cloned for each additional key.
macro_rules! load_recipe_multi {
    ($recipes:expr, [$(($mf:expr, $mt:expr)),+ $(,)?], $file:expr) => {{
        let text = include_str!(concat!("../../recipes/endf6/", $file));
        let parsed = parse_recipe(text).map_err(|e| EndfError::RecipeParse {
            message: format!("failed to parse {}: {}", $file, e),
        })?;
        let keys: &[(i32, i32)] = &[$(($mf, $mt)),+];
        for (i, &key) in keys.iter().enumerate() {
            if i == keys.len() - 1 {
                // Move on last insertion to avoid an extra clone
                $recipes.insert(key, parsed);
                break;
            } else {
                $recipes.insert(key, parsed.clone());
            }
        }
    }};
}

/// Helper macro to load a recipe from a flavor-specific directory and override
/// an entry in an existing recipes map.
macro_rules! load_recipe_from {
    ($recipes:expr, $mf:expr, $mt:expr, $dir:expr, $file:expr) => {{
        let text = include_str!(concat!("../../recipes/", $dir, "/", $file));
        let parsed = parse_recipe(text).map_err(|e| EndfError::RecipeParse {
            message: format!("failed to parse {}/{}: {}", $dir, $file, e),
        })?;
        $recipes.insert(($mf, $mt), parsed);
    }};
}

impl RecipeCatalogue {
    /// Load the built-in ENDF-6 recipes.
    ///
    /// All recipe source files are embedded at compile time via `include_str!`.
    /// Each file is parsed on the first call. This constructor is not lazy;
    /// callers who want deferred initialization should wrap it in
    /// `std::sync::OnceLock` or similar.
    pub fn endf6() -> EndfResult<Self> {
        let mut recipes = HashMap::new();

        // ---- MF 0 (tape head) ----
        load_recipe!(recipes, 0, 0, "endf_recipe_mf0_mt0_tapehead.recipe");

        // ---- MF 1 ----
        load_recipe!(recipes, 1, 451, "endf_recipe_mf1_mt451.recipe");
        load_recipe!(recipes, 1, 452, "endf_recipe_mf1_mt452.recipe");
        load_recipe!(recipes, 1, 455, "endf_recipe_mf1_mt455.recipe");
        load_recipe!(recipes, 1, 456, "endf_recipe_mf1_mt456.recipe");
        load_recipe!(recipes, 1, 458, "endf_recipe_mf1_mt458.recipe");
        load_recipe!(recipes, 1, 460, "endf_recipe_mf1_mt460.recipe");

        // ---- MF 2 ----
        load_recipe!(recipes, 2, 151, "endf_recipe_mf2_mt151.recipe");

        // ---- MF 3-6 (wildcard) ----
        load_recipe!(recipes, 3, -1, "endf_recipe_mf3.recipe");
        load_recipe!(recipes, 4, -1, "endf_recipe_mf4.recipe");
        load_recipe!(recipes, 5, -1, "endf_recipe_mf5.recipe");
        load_recipe!(recipes, 6, -1, "endf_recipe_mf6.recipe");

        // ---- MF 7 ----
        load_recipe!(recipes, 7, 2, "endf_recipe_mf7_mt2.recipe");
        load_recipe!(recipes, 7, 4, "endf_recipe_mf7_mt4.recipe");
        load_recipe!(recipes, 7, 451, "endf_recipe_mf7_mt451.recipe");

        // ---- MF 8 ----
        load_recipe!(recipes, 8, -1, "endf_recipe_mf8.recipe");
        load_recipe!(recipes, 8, 454, "endf_recipe_mf8_mt454.recipe");
        load_recipe!(recipes, 8, 457, "endf_recipe_mf8_mt457.recipe");
        load_recipe!(recipes, 8, 459, "endf_recipe_mf8_mt459.recipe");

        // ---- MF 9-10 (wildcard) ----
        load_recipe!(recipes, 9, -1, "endf_recipe_mf9.recipe");
        load_recipe!(recipes, 10, -1, "endf_recipe_mf10.recipe");

        // ---- MF 12-15 (wildcard) ----
        load_recipe!(recipes, 12, -1, "endf_recipe_mf12.recipe");
        load_recipe!(recipes, 13, -1, "endf_recipe_mf13.recipe");
        load_recipe!(recipes, 14, -1, "endf_recipe_mf14.recipe");
        load_recipe!(recipes, 15, -1, "endf_recipe_mf15.recipe");

        // ---- MF 23, 26-28 (wildcard) ----
        load_recipe!(recipes, 23, -1, "endf_recipe_mf23.recipe");
        load_recipe!(recipes, 26, -1, "endf_recipe_mf26.recipe");
        load_recipe!(recipes, 27, -1, "endf_recipe_mf27.recipe");
        load_recipe!(recipes, 28, -1, "endf_recipe_mf28.recipe");

        // ---- MF 31 ----
        // MT 452, 455, 456 share the same recipe file
        load_recipe_multi!(
            recipes,
            [(31, 452), (31, 455), (31, 456)],
            "endf_recipe_mf31_mt452_455_456.recipe"
        );
        load_recipe!(recipes, 31, -1, "endf_recipe_mf31.recipe");

        // ---- MF 32-35 (wildcard) ----
        load_recipe!(recipes, 32, -1, "endf_recipe_mf32.recipe");
        load_recipe!(recipes, 33, -1, "endf_recipe_mf33.recipe");
        load_recipe!(recipes, 34, -1, "endf_recipe_mf34.recipe");
        load_recipe!(recipes, 35, -1, "endf_recipe_mf35.recipe");

        // ---- MF 40 (wildcard) ----
        load_recipe!(recipes, 40, -1, "endf_recipe_mf40.recipe");

        Ok(Self { recipes })
    }

    /// Load the ENDF-6 extended recipes.
    ///
    /// Starts from the standard ENDF-6 catalogue, then overrides:
    /// - MF8/MT457 with the JENDL-specific version (tolerates NT=4, NT=10)
    /// - MF4 (wildcard) with an extended version supporting the obsolete
    ///   energy transformation matrix (LVT > 0)
    pub fn endf6_ext() -> EndfResult<Self> {
        let mut cat = Self::endf6()?;
        load_recipe_from!(cat.recipes, 8, 457, "jendl", "endf_recipe_mf8_mt457.recipe");
        load_recipe_from!(cat.recipes, 4, -1, "endf6-ext", "endf_recipe_mf4.recipe");
        Ok(cat)
    }

    /// Load the JENDL-specific recipes.
    ///
    /// Starts from the standard ENDF-6 catalogue, then overrides:
    /// - MF8/MT457 with a version that handles JENDL-specific NT values
    ///   (NT=4, NT=10) and the JENDL stable-nucleus specialization.
    pub fn jendl() -> EndfResult<Self> {
        let mut cat = Self::endf6()?;
        load_recipe_from!(cat.recipes, 8, 457, "jendl", "endf_recipe_mf8_mt457.recipe");
        Ok(cat)
    }

    /// Load the PENDF (processed ENDF) recipes.
    ///
    /// Starts from the standard ENDF-6 catalogue, then overrides
    /// and extends with NJOY PENDF-specific sections:
    /// - MF1/MT451: modified header with TEMP and TOL fields
    /// - MF2/MT152: Bondarenko self-shielded cross sections (new)
    /// - MF2/MT153: probability tables for URR self-shielding (new)
    /// - MF3 (wildcard): linearized cross sections with LMTR flag
    /// - MF6 (wildcard): extended with NJOY/THERMR thermal scattering
    /// - MF23 (wildcard): photoatomic cross sections with LMTR flag
    pub fn pendf() -> EndfResult<Self> {
        let mut cat = Self::endf6()?;
        load_recipe_from!(cat.recipes, 1, 451, "pendf", "endf_recipe_mf1_mt451.recipe");
        load_recipe_from!(cat.recipes, 2, 152, "pendf", "endf_recipe_mf2_mt152.recipe");
        load_recipe_from!(cat.recipes, 2, 153, "pendf", "endf_recipe_mf2_mt153.recipe");
        load_recipe_from!(cat.recipes, 3, -1, "pendf", "endf_recipe_mf3.recipe");
        load_recipe_from!(cat.recipes, 6, -1, "pendf", "endf_recipe_mf6.recipe");
        load_recipe_from!(cat.recipes, 23, -1, "pendf", "endf_recipe_mf23.recipe");
        Ok(cat)
    }

    /// Load the ERRORR covariance format recipes.
    ///
    /// This is a standalone format (not based on ENDF-6) produced by
    /// NJOY's ERRORR module. It contains only:
    /// - MF0/MT0: tape description
    /// - MF1/MT451: multigroup energy boundaries
    /// - MF3 (wildcard): group-averaged cross sections
    /// - MF33 (wildcard): group-averaged covariance matrices
    pub fn errorr() -> EndfResult<Self> {
        let mut recipes = HashMap::new();
        load_recipe_from!(recipes, 0, 0, "errorr", "endf_recipe_mf0_mt0.recipe");
        load_recipe_from!(recipes, 1, 451, "errorr", "endf_recipe_mf1_mt451.recipe");
        load_recipe_from!(recipes, 3, -1, "errorr", "endf_recipe_mf3.recipe");
        load_recipe_from!(recipes, 33, -1, "errorr", "endf_recipe_mf33.recipe");
        Ok(Self { recipes })
    }

    /// Build a catalogue for the given format name.
    ///
    /// Supported formats: `"endf6"`, `"endf6-ext"`, `"jendl"`, `"pendf"`, `"errorr"`.
    pub fn for_format(format: &str) -> EndfResult<Self> {
        match format {
            "endf6" => Self::endf6(),
            "endf6-ext" => Self::endf6_ext(),
            "jendl" => Self::jendl(),
            "pendf" => Self::pendf(),
            "errorr" => Self::errorr(),
            _ => Err(EndfError::RecipeParse {
                message: format!("unknown ENDF format: '{}'. Expected one of: endf6, endf6-ext, jendl, pendf, errorr", format),
            }),
        }
    }

    /// Look up a recipe for a given MF/MT combination.
    ///
    /// Resolution order:
    /// 1. Exact match on `(mf, mt)`
    /// 2. Wildcard match on `(mf, -1)`
    /// 3. `None`
    pub fn get(&self, mf: i32, mt: i32) -> Option<&Recipe> {
        self.recipes
            .get(&(mf, mt))
            .or_else(|| self.recipes.get(&(mf, -1)))
    }

    /// Create an empty catalogue for manual population.
    pub fn new() -> Self {
        Self {
            recipes: HashMap::new(),
        }
    }

    /// Load a recipe from a string and register it for the given MF/MT.
    /// Use MT = -1 for a wildcard that matches any MT within the MF.
    pub fn add_recipe_from_str(&mut self, mf: i32, mt: i32, recipe_text: &str) -> EndfResult<()> {
        let parsed = parse_recipe(recipe_text)?;
        self.recipes.insert((mf, mt), parsed);
        Ok(())
    }

    /// Load all `.recipe` files from a directory.
    /// File names must follow the pattern `endf_recipe_mfX.recipe` or
    /// `endf_recipe_mfX_mtY.recipe`. Files without MT are registered
    /// as wildcard (MT = -1).
    pub fn load_from_dir(dir: &std::path::Path) -> EndfResult<Self> {
        use std::fs;
        let mut catalogue = Self::new();
        let entries = fs::read_dir(dir).map_err(|e| EndfError::Io(e))?;
        for entry in entries {
            let entry = entry.map_err(|e| EndfError::Io(e))?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("recipe") {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if let Some((mf, mt)) = parse_recipe_filename(stem) {
                let text = fs::read_to_string(&path).map_err(|e| EndfError::Io(e))?;
                catalogue.add_recipe_from_str(mf, mt, &text)?;
            }
        }
        Ok(catalogue)
    }

    /// List all available (MF, MT) pairs, sorted.
    pub fn available_sections(&self) -> Vec<(i32, i32)> {
        let mut keys: Vec<_> = self.recipes.keys().copied().collect();
        keys.sort();
        keys
    }
}

/// Parse a recipe filename stem like "endf_recipe_mf3" or "endf_recipe_mf1_mt451"
/// into (mf, mt). Returns MT=-1 for wildcard (no MT in name).
fn parse_recipe_filename(stem: &str) -> Option<(i32, i32)> {
    let stem = stem.strip_prefix("endf_recipe_")?;
    if let Some(rest) = stem.strip_prefix("mf") {
        if let Some(idx) = rest.find("_mt") {
            let mf: i32 = rest[..idx].parse().ok()?;
            let mt: i32 = rest[idx + 3..].parse().ok()?;
            Some((mf, mt))
        } else {
            let mf: i32 = rest.parse().ok()?;
            Some((mf, -1))
        }
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_catalogue_loads_all_recipes() {
        let catalogue = RecipeCatalogue::endf6().expect("all built-in recipes should parse");
        let sections = catalogue.available_sections();

        // Verify we have a reasonable number of entries
        // 40 entries based on the catalogue.json mapping
        assert!(
            sections.len() >= 38,
            "expected at least 38 catalogue entries, got {}",
            sections.len()
        );
    }

    #[test]
    fn test_exact_lookup() {
        let catalogue = RecipeCatalogue::endf6().unwrap();

        // Exact matches should work
        assert!(catalogue.get(1, 451).is_some(), "MF1/MT451 should exist");
        assert!(catalogue.get(2, 151).is_some(), "MF2/MT151 should exist");
        assert!(catalogue.get(7, 2).is_some(), "MF7/MT2 should exist");
        assert!(catalogue.get(8, 457).is_some(), "MF8/MT457 should exist");
        assert!(catalogue.get(31, 452).is_some(), "MF31/MT452 should exist");
    }

    #[test]
    fn test_wildcard_fallback() {
        let catalogue = RecipeCatalogue::endf6().unwrap();

        // MF3 only has a wildcard entry (mt=-1), so any MT should resolve
        assert!(
            catalogue.get(3, 1).is_some(),
            "MF3/MT1 should resolve via wildcard"
        );
        assert!(
            catalogue.get(3, 999).is_some(),
            "MF3/MT999 should resolve via wildcard"
        );

        // MF8 has both wildcard and specific entries
        // MT457 should hit the exact match, MT999 should hit the wildcard
        assert!(catalogue.get(8, 457).is_some(), "MF8/MT457 exact match");
        assert!(
            catalogue.get(8, 999).is_some(),
            "MF8/MT999 should resolve via wildcard"
        );
    }

    #[test]
    fn test_missing_mf_returns_none() {
        let catalogue = RecipeCatalogue::endf6().unwrap();

        // MF99 does not exist
        assert!(
            catalogue.get(99, 1).is_none(),
            "MF99 should not resolve to anything"
        );
    }

    #[test]
    fn test_tapehead_entry() {
        let catalogue = RecipeCatalogue::endf6().unwrap();
        assert!(
            catalogue.get(0, 0).is_some(),
            "tape head recipe (MF0/MT0) should exist"
        );
    }

    #[test]
    fn test_endf6_ext_catalogue() {
        let catalogue = RecipeCatalogue::endf6_ext().expect("endf6-ext recipes should parse");
        let sections = catalogue.available_sections();
        // Should have at least as many entries as endf6
        assert!(sections.len() >= 38, "endf6-ext should have at least 38 entries, got {}", sections.len());
        // MF4 wildcard and MF8/MT457 should exist (overridden)
        assert!(catalogue.get(4, 1).is_some(), "MF4 wildcard should resolve");
        assert!(catalogue.get(8, 457).is_some(), "MF8/MT457 should exist");
    }

    #[test]
    fn test_jendl_catalogue() {
        let catalogue = RecipeCatalogue::jendl().expect("jendl recipes should parse");
        let sections = catalogue.available_sections();
        assert!(sections.len() >= 38, "jendl should have at least 38 entries, got {}", sections.len());
        assert!(catalogue.get(8, 457).is_some(), "MF8/MT457 should exist");
    }

    #[test]
    fn test_pendf_catalogue() {
        let catalogue = RecipeCatalogue::pendf().expect("pendf recipes should parse");
        // Should have endf6 entries plus MF2/MT152 and MF2/MT153
        assert!(catalogue.get(1, 451).is_some(), "MF1/MT451 should exist");
        assert!(catalogue.get(2, 152).is_some(), "MF2/MT152 should exist (pendf-specific)");
        assert!(catalogue.get(2, 153).is_some(), "MF2/MT153 should exist (pendf-specific)");
        assert!(catalogue.get(3, 1).is_some(), "MF3 wildcard should resolve");
        assert!(catalogue.get(6, 1).is_some(), "MF6 wildcard should resolve");
        assert!(catalogue.get(23, 1).is_some(), "MF23 wildcard should resolve");
    }

    #[test]
    fn test_errorr_catalogue() {
        let catalogue = RecipeCatalogue::errorr().expect("errorr recipes should parse");
        let sections = catalogue.available_sections();
        assert_eq!(sections.len(), 4, "errorr should have exactly 4 entries, got {}", sections.len());
        assert!(catalogue.get(0, 0).is_some(), "MF0/MT0 should exist");
        assert!(catalogue.get(1, 451).is_some(), "MF1/MT451 should exist");
        assert!(catalogue.get(3, 1).is_some(), "MF3 wildcard should resolve");
        assert!(catalogue.get(33, 1).is_some(), "MF33 wildcard should resolve");
        // Should NOT have endf6-specific entries
        assert!(catalogue.get(2, 151).is_none(), "MF2/MT151 should not exist in errorr");
    }

    #[test]
    fn test_for_format_dispatch() {
        assert!(RecipeCatalogue::for_format("endf6").is_ok());
        assert!(RecipeCatalogue::for_format("endf6-ext").is_ok());
        assert!(RecipeCatalogue::for_format("jendl").is_ok());
        assert!(RecipeCatalogue::for_format("pendf").is_ok());
        assert!(RecipeCatalogue::for_format("errorr").is_ok());
        assert!(RecipeCatalogue::for_format("unknown").is_err());
    }
}
