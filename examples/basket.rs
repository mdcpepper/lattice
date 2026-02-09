//! Processed Basket Receipt Example
//!
//! This example demonstrates a basket with multiple promotion types.
//!
//! Use `-f` to load a fixture set by name
//! Use `-n` to limit the number of items
//! Use `-o` to specify the filename of a typst formatted output file in `target/ilp-formulations`

use std::{fs::create_dir_all, io, io::Write, path::PathBuf, time::Instant};

use anyhow::Result;
use clap::Parser;
use humanize_duration::{Truncate, prelude::DurationExt};

use lattice::{
    fixtures::Fixture, items::groups::ItemGroup, receipt::Receipt,
    solvers::ilp::renderers::typst::MultiLayerRenderer, utils::ExampleBasketArgs,
};

/// Processed Basket Receipt Example
#[expect(clippy::print_stdout, reason = "Example program output to user")]
pub fn main() -> Result<()> {
    let args = ExampleBasketArgs::parse();

    let fixture = Fixture::from_set(&args.fixture)?;
    let basket = fixture.basket(args.n)?;
    let item_group = ItemGroup::from(&basket);

    let start = Instant::now();

    let result = if let Some(out) = args.out.as_deref() {
        let output_dir = PathBuf::from("target").join("ilp-formulations");

        create_dir_all(&output_dir)?;

        let mut renderer = MultiLayerRenderer::new_with_metadata(
            output_dir.join(out),
            &item_group,
            fixture.product_meta_map(),
            fixture.promotion_meta_map(),
        );

        let result = fixture
            .graph()?
            .evaluate_with_observer(&item_group, Some(&mut renderer))?;

        renderer.write()?;

        println!(
            "\nILP formulation written to: {}",
            renderer.output_path().display()
        );

        result
    } else {
        fixture.graph()?.evaluate(&item_group)?
    };

    let elapsed = start.elapsed();

    let receipt = Receipt::from_layered_result(&basket, result)?;

    let stdout = io::stdout();
    let mut handle = stdout.lock();

    receipt.write_to(
        &mut handle,
        &basket,
        fixture.product_meta_map(),
        fixture.promotion_meta_map(),
    )?;

    writeln!(
        handle,
        " {} ({}s)",
        elapsed.human(Truncate::Nano),
        elapsed.as_secs_f32()
    )?;

    Ok(())
}
