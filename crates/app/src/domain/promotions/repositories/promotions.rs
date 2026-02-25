//! Promotions Repository

use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{Postgres, Transaction, query, query_as};
use uuid::Uuid;

use crate::domain::promotions::{
    data::{Promotion, budgets::Budgets, discounts::SimpleDiscount},
    records::{PromotionRecord, PromotionUuid},
};

const COLUMN_DISCOUNT_AMOUNT: &str = "discount_amount";
const COLUMN_REDEMPTION_BUDGET: &str = "redemption_budget";
const COLUMN_MONETARY_BUDGET: &str = "monetary_budget";

const CREATE_PROMOTION_SQL: &str = include_str!("../sql/create_promotion.sql");
const CREATE_DIRECT_DISCOUNT_PROMOTION_SQL: &str =
    include_str!("../sql/direct/create_direct_discount_promotion.sql");
const CREATE_DIRECT_DISCOUNT_PROMOTION_DETAIL_SQL: &str =
    include_str!("../sql/direct/create_direct_discount_promotion_detail.sql");

#[derive(Debug, Clone, Default)]
pub(crate) struct PgPromotionsRepository;

impl PgPromotionsRepository {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn create_promotion(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        promotion: Promotion,
    ) -> Result<PromotionRecord, sqlx::Error> {
        match &promotion {
            Promotion::DirectDiscount {
                uuid,
                budgets,
                discount,
                ..
            } => {
                insert_direct_discount_promotion(tx, uuid, budgets, discount).await?;
                insert_promotion_record(tx, uuid, &promotion).await
            }
        }
    }
}

async fn insert_direct_discount_promotion(
    tx: &mut Transaction<'_, Postgres>,
    uuid: &PromotionUuid,
    budgets: &Budgets,
    discount: &SimpleDiscount,
) -> Result<(), sqlx::Error> {
    let redemption_budget = budgets
        .redemptions
        .map(|v| try_i64_from_u64(v, COLUMN_REDEMPTION_BUDGET))
        .transpose()?;

    let monetary_budget = budgets
        .monetary
        .map(|v| try_i64_from_u64(v, COLUMN_MONETARY_BUDGET))
        .transpose()?;

    let (discount_percentage, discount_amount) = discount_numeric_sql_values(discount)?;

    let db_uuid = uuid.into_uuid();

    query(CREATE_DIRECT_DISCOUNT_PROMOTION_SQL)
        .bind(db_uuid)
        .execute(&mut **tx)
        .await?;

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

async fn insert_promotion_record(
    tx: &mut Transaction<'_, Postgres>,
    uuid: &PromotionUuid,
    promotion: &Promotion,
) -> Result<PromotionRecord, sqlx::Error> {
    let (db_uuid, created_at, updated_at, deleted_at): (
        Uuid,
        SqlxTimestamp,
        SqlxTimestamp,
        Option<SqlxTimestamp>,
    ) = query_as(CREATE_PROMOTION_SQL)
        .bind(uuid.into_uuid())
        .bind(promotion.type_as_str())
        .fetch_one(&mut **tx)
        .await?;

    Ok(PromotionRecord {
        uuid: PromotionUuid::from_uuid(db_uuid),
        created_at: created_at.to_jiff(),
        updated_at: updated_at.to_jiff(),
        deleted_at: deleted_at.map(SqlxTimestamp::to_jiff),
    })
}

fn discount_numeric_sql_values(
    discount: &SimpleDiscount,
) -> Result<(Option<i64>, Option<i64>), sqlx::Error> {
    match discount {
        SimpleDiscount::PercentageOff { percentage } => Ok((Some(i64::from(*percentage)), None)),
        SimpleDiscount::FixedAmountOff { amount } => Ok((
            None,
            Some(try_i64_from_u64(*amount, COLUMN_DISCOUNT_AMOUNT)?),
        )),
    }
}

fn try_i64_from_u64(value: u64, column: &'static str) -> Result<i64, sqlx::Error> {
    i64::try_from(value).map_err(|e| sqlx::Error::ColumnDecode {
        index: column.to_string(),
        source: Box::new(e),
    })
}
