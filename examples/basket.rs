//! Basket Example
//!
//! This example demonstrates a basket with multiple promotion types.
//!
//! Use `-f` to load a fixture set by name
//! Use `-n` to specify the number of items to add to the basket
//! Use `-o` to specify the filename of a typst formatted output file in `target/ilp-formulations`

use std::{fs::create_dir_all, io, path::PathBuf, time::Instant};

use anyhow::Result;

use clap::Parser;
use dante::{
    fixtures::Fixture,
    items::groups::ItemGroup,
    receipt::Receipt,
    solvers::{
        Solver,
        ilp::{ILPSolver, renderers::typst::TypstRenderer},
    },
    utils::ExampleBasketArgs,
};

/// Basket Example
#[expect(clippy::print_stdout, reason = "Example code")]
pub fn main() -> Result<()> {
    let args = ExampleBasketArgs::parse();

    let fixture = Fixture::from_set(&args.fixture)?;

    let basket = fixture.basket(args.n)?;
    let item_group = ItemGroup::from(&basket);
    let promotions = fixture.promotions();

    let start = Instant::now();

    let result = if let Some(out) = args.out.as_deref() {
        let output_dir = PathBuf::from("target").join("ilp-formulations");
        create_dir_all(&output_dir)?;

        let output_path = output_dir.join(out);

        let mut renderer = TypstRenderer::new_with_metadata(
            output_path,
            &item_group,
            fixture.product_meta_map(),
            fixture.promotion_meta_map(),
        );

        let result = ILPSolver::solve_with_observer(promotions, &item_group, &mut renderer)?;
        renderer.write()?;

        result
    } else {
        ILPSolver::solve(promotions, &item_group)?
    };

    let elapsed = start.elapsed().as_secs_f32();

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    Receipt::from_solver_result(&basket, result)?.write_to(
        &mut handle,
        &basket,
        fixture.product_meta_map(),
        fixture.promotion_meta_map(),
    )?;

    println!("\nSolution: {elapsed}s");

    Ok(())
}
