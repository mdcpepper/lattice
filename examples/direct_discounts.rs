//! Direct Discounts Example
//!
//! This example demonstrates simple percentage discounts applied directly to
//! individual items. Two promotions are configured, one that applies a 20%
//! discount to items tagged "20-off", and one that applies a 40% discount to
//! items tagged "40-off".
//!
//! Run with: `cargo run --example direct_discounts`

use std::{io, time::Instant};

use anyhow::Result;

use dante::{
    fixtures::Fixture,
    items::groups::ItemGroup,
    receipt::Receipt,
    solvers::{Solver, ilp::ILPSolver},
};

/// Direct Discounts Example
#[expect(clippy::print_stdout, reason = "Example code")]
pub fn main() -> Result<()> {
    // Load fixture set
    let fixture = Fixture::from_set("example_direct_discounts")?;

    let basket = fixture.basket()?;
    let item_group: ItemGroup<'_> = ItemGroup::from(&basket);
    let promotions = fixture.promotions();

    let start = Instant::now();

    let result = ILPSolver::solve(promotions, &item_group)?;

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
