//! Direct Discount Promotions

use sqlx::{Postgres, Transaction, query, query_scalar};
use uuid::Uuid;

use crate::domain::promotions::{
    data::{budgets::Budgets, discounts::SimpleDiscount},
    records::{DirectDiscountDetailUuid, PromotionUuid},
    repositories::promotions::{budget_numeric_sql_values, discount_numeric_sql_values},
};

const CREATE_DIRECT_DISCOUNT_PROMOTION_DETAIL_SQL: &str =
    include_str!("../../sql/direct/create_direct_discount_promotion_detail.sql");

const UPDATE_DIRECT_DISCOUNT_PROMOTION_DETAIL_SQL: &str =
    include_str!("../../sql/direct/update_direct_discount_promotion_detail.sql");

pub(crate) async fn insert_direct_discount_promotion(
    tx: &mut Transaction<'_, Postgres>,
    uuid: PromotionUuid,
    budgets: &Budgets,
    discount: &SimpleDiscount,
) -> Result<(), sqlx::Error> {
    let db_uuid = uuid.into_uuid();

    let (redemption_budget, monetary_budget) = budget_numeric_sql_values(budgets)?;
    let (discount_percentage, discount_amount) = discount_numeric_sql_values(discount)?;

    query(CREATE_DIRECT_DISCOUNT_PROMOTION_DETAIL_SQL)
        .bind(db_uuid)
        .bind(redemption_budget)
        .bind(monetary_budget)
        .bind(discount.to_str())
        .bind(discount_percentage)
        .bind(discount_amount)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

pub(crate) async fn update_direct_discount_promotion(
    tx: &mut Transaction<'_, Postgres>,
    uuid: PromotionUuid,
    budgets: &Budgets,
    discount: &SimpleDiscount,
) -> Result<DirectDiscountDetailUuid, sqlx::Error> {
    let new_detail_uuid = DirectDiscountDetailUuid::new();

    let (redemption_budget, monetary_budget) = budget_numeric_sql_values(budgets)?;
    let (discount_percentage, discount_amount) = discount_numeric_sql_values(discount)?;

    let returned_uuid: Uuid = query_scalar(UPDATE_DIRECT_DISCOUNT_PROMOTION_DETAIL_SQL)
        .bind(uuid.into_uuid())
        .bind(new_detail_uuid.into_uuid())
        .bind(redemption_budget)
        .bind(monetary_budget)
        .bind(discount.to_str())
        .bind(discount_percentage)
        .bind(discount_amount)
        .fetch_one(&mut **tx)
        .await?;

    Ok(DirectDiscountDetailUuid::from_uuid(returned_uuid))
}
