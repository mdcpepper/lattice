//! Cart Items Repository

use jiff::Timestamp;
use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{FromRow, Postgres, Row, Transaction, postgres::PgRow, query_as};
use uuid::Uuid;

use crate::domain::carts::models::{CartItem, NewCartItem};

use super::carts::try_get_amount;

const GET_CART_ITEMS_SQL: &str = include_str!("../sql/get_cart_items.sql");
const CREATE_CART_ITEM_SQL: &str = include_str!("../sql/create_cart_item.sql");

#[derive(Debug, Clone, Default)]
pub(crate) struct PgCartItemsRepository;

impl PgCartItemsRepository {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn get_cart_items(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cart_uuid: Uuid,
        point_in_time: Timestamp,
    ) -> Result<Vec<CartItem>, sqlx::Error> {
        query_as::<Postgres, CartItem>(GET_CART_ITEMS_SQL)
            .bind(cart_uuid)
            .bind(SqlxTimestamp::from(point_in_time))
            .fetch_all(&mut **tx)
            .await
    }

    pub(crate) async fn create_cart_item(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cart_uuid: Uuid,
        item: NewCartItem,
    ) -> Result<CartItem, sqlx::Error> {
        query_as::<Postgres, CartItem>(CREATE_CART_ITEM_SQL)
            .bind(item.uuid)
            .bind(cart_uuid)
            .bind(item.product_uuid)
            .fetch_one(&mut **tx)
            .await
    }
}

impl<'r> FromRow<'r, PgRow> for CartItem {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        let base_price = try_get_amount(row, "base_price")?;

        Ok(Self {
            uuid: row.try_get("uuid")?,
            base_price,
            product_uuid: row.try_get("product_uuid")?,
            created_at: row.try_get::<SqlxTimestamp, _>("created_at")?.to_jiff(),
            updated_at: row.try_get::<SqlxTimestamp, _>("updated_at")?.to_jiff(),
            deleted_at: row
                .try_get::<Option<SqlxTimestamp>, _>("deleted_at")?
                .map(SqlxTimestamp::to_jiff),
        })
    }
}
