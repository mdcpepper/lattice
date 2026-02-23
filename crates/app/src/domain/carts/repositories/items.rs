//! Cart Items Repository

use jiff::Timestamp;
use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{FromRow, Postgres, Row, Transaction, postgres::PgRow, query, query_as};

use crate::domain::{
    carts::models::{CartItem, CartItemUuid, CartUuid, NewCartItem},
    products::models::ProductUuid,
};

use super::carts::try_get_amount;

const GET_CART_ITEMS_SQL: &str = include_str!("../sql/get_cart_items.sql");
const CREATE_CART_ITEM_SQL: &str = include_str!("../sql/create_cart_item.sql");
const DELETE_CART_ITEM_SQL: &str = include_str!("../sql/delete_cart_item.sql");

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
        cart: CartUuid,
        point_in_time: Timestamp,
    ) -> Result<Vec<CartItem>, sqlx::Error> {
        query_as::<Postgres, CartItem>(GET_CART_ITEMS_SQL)
            .bind(cart.into_uuid())
            .bind(SqlxTimestamp::from(point_in_time))
            .fetch_all(&mut **tx)
            .await
    }

    pub(crate) async fn create_cart_item(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cart: CartUuid,
        item: NewCartItem,
    ) -> Result<CartItem, sqlx::Error> {
        query_as::<Postgres, CartItem>(CREATE_CART_ITEM_SQL)
            .bind(item.uuid.into_uuid())
            .bind(cart.into_uuid())
            .bind(item.product_uuid.into_uuid())
            .fetch_one(&mut **tx)
            .await
    }

    pub(crate) async fn delete_cart_item(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cart: CartUuid,
        item: CartItemUuid,
    ) -> Result<u64, sqlx::Error> {
        let rows_affected = query(DELETE_CART_ITEM_SQL)
            .bind(item.into_uuid())
            .bind(cart.into_uuid())
            .execute(&mut **tx)
            .await?
            .rows_affected();

        Ok(rows_affected)
    }
}

impl<'r> FromRow<'r, PgRow> for CartItem {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        let base_price = try_get_amount(row, "base_price")?;

        Ok(Self {
            uuid: CartItemUuid::from_uuid(row.try_get("uuid")?),
            base_price,
            product_uuid: ProductUuid::from_uuid(row.try_get("product_uuid")?),
            created_at: row.try_get::<SqlxTimestamp, _>("created_at")?.to_jiff(),
            updated_at: row.try_get::<SqlxTimestamp, _>("updated_at")?.to_jiff(),
            deleted_at: row
                .try_get::<Option<SqlxTimestamp>, _>("deleted_at")?
                .map(SqlxTimestamp::to_jiff),
        })
    }
}
