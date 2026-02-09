//! Real-world conformance tests

use lattice::{fixtures::Fixture, receipt::Receipt};
use rusty_money::{Money, iso::GBP};
use testresult::TestResult;

#[test]
fn meal_deal_conformance() -> TestResult {
    let fixture = Fixture::from_set("conformance/meal-deals")?;
    let basket = fixture.basket(None)?;
    let item_group = fixture.item_group()?;
    let result = fixture.graph()?.evaluate(&item_group)?;
    let receipt = Receipt::from_layered_result(&basket, result)?;

    assert_eq!(receipt.subtotal(), Money::from_minor(11_99, GBP));
    assert_eq!(receipt.total(), Money::from_minor(7_70, GBP));

    Ok(())
}
