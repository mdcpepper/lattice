//! Integration test for comprehensive fixture set covering all promotion types.
//!
//! This test validates that the ILP solver correctly handles a realistic basket
//! with all four promotion types: `DirectDiscount`, `PositionalDiscount`,
//! `MixAndMatch`, and `TieredThreshold`.

use smallvec::SmallVec;
use testresult::TestResult;

use lattice::{
    fixtures::Fixture,
    items::groups::ItemGroup,
    solvers::{Solver, ilp::ILPSolver},
};

#[test]
fn test_comprehensive_fixture_solving() -> TestResult {
    // Load comprehensive fixture set
    let fixture = Fixture::from_set("comprehensive")?;

    // Create basket with all items
    let basket = fixture.basket(None)?;
    let item_group = ItemGroup::from(&basket);

    // Load all promotions
    let promotions = fixture.promotions();

    // Solve with ILP solver
    let result = ILPSolver::solve(promotions, &item_group)?;

    // Expected total from the current comprehensive fixture optimization.
    let expected_total_minor = 2122i64;
    let actual_total_minor = result.total.to_minor_units();

    assert_eq!(
        actual_total_minor,
        expected_total_minor,
        "Expected {} pence (£{}.{}), got {} pence (£{}.{})",
        expected_total_minor,
        expected_total_minor / 100,
        expected_total_minor % 100,
        actual_total_minor,
        actual_total_minor / 100,
        actual_total_minor % 100
    );

    // Verify we have the correct number of promotion applications
    assert_eq!(
        result.promotion_applications.len(),
        13,
        "Expected exactly 13 promotion applications"
    );

    let mut affected_items = result.affected_items;
    affected_items.sort_unstable();

    // Verify we have affected items (items with promotions applied)
    assert_eq!(
        affected_items,
        SmallVec::from_buf([
            0,  // Granny Smith Apple
            1,  // Organic Banana
            2,  // Navel Orange
            3,  // Iceberg Lettuce
            5,  // Cheddar Chese 200g
            7,  // Salted Butter
            8,  // Sourdough Loaf
            9,  // Butter Criossant
            10, // Blueberry Muffin
            12, // Orange Juice 1L
            14, // Sea Salt Crisps
            15, // Chocolate Chip Cookies
            16, // Dark Chocolate Bar
        ])
    );

    let mut unaffected_items = result.unaffected_items;
    unaffected_items.sort_unstable();

    // Verify we have unaffected items (items without promotions)
    assert_eq!(
        unaffected_items,
        SmallVec::from_buf([
            4,  // Whole Milk 1L
            6,  // Greek Yoghurt
            11, // Mineral Water 1L
            13, // Cola 2L
        ])
    );

    // Total items should equal basket size
    let total_items = affected_items.len() + unaffected_items.len();
    assert_eq!(total_items, 17, "All items should be accounted for");

    Ok(())
}
