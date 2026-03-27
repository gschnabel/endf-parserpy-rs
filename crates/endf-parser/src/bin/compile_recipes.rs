//! Binary that compiles ENDF recipe ASTs into Rust source code.
//!
//! Usage:
//!   cargo run --bin compile_recipes > src/generated_parsers.rs
//!
//! The output is a self-contained Rust source file that can be compiled
//! as a separate crate or included as a module.

use endf_parser::recipe::catalogue::RecipeCatalogue;
use endf_parser::recipe::compiler::compile_catalogue;

fn main() {
    let catalogue = RecipeCatalogue::endf6().expect("failed to load recipe catalogue");

    // Collect all available recipes
    let sections = catalogue.available_sections();
    let mut recipe_refs: Vec<(i32, i32, &[endf_parser::recipe::ast::RecipeNode])> = Vec::new();

    for &(mf, mt) in &sections {
        if let Some(recipe) = catalogue.get(mf, mt) {
            recipe_refs.push((mf, mt, recipe));
        }
    }

    let code = compile_catalogue(&recipe_refs);
    print!("{}", code);
}
