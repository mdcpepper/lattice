//! Cart Items Repository

use jiff::Timestamp;
use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{FromRow, Postgres, Row, Transaction, postgres::PgRow, query, query_as};
use tracing::debug;

use crate::domain::{
    carts::{
        data::NewCartItem,
        records::{CartItemRecord, CartItemUuid, CartUuid},
    },
    products::records::ProductUuid,
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

    #[tracing::instrument(
        name = "carts.items_repository.get_cart_items",
        skip(self, tx),
        fields(cart_uuid = %cart, point_in_time = %point_in_time),
        err
    )]
    pub(crate) async fn get_cart_items(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cart: CartUuid,
        point_in_time: Timestamp,
    ) -> Result<Vec<CartItemRecord>, sqlx::Error> {
        let items = query_as::<Postgres, CartItemRecord>(GET_CART_ITEMS_SQL)
            .bind(cart.into_uuid())
            .bind(SqlxTimestamp::from(point_in_time))
            .fetch_all(&mut **tx)
            .await?;
        let item_count = items.len();

        debug!(cart_uuid = %cart, item_count, "queried cart items");

        Ok(items)
    }

    #[tracing::instrument(
        name = "carts.items_repository.create_cart_item",
        skip(self, tx),
        fields(
            cart_uuid = %cart,
            item_uuid = %item.uuid,
            product_uuid = %item.product_uuid
        ),
        err
    )]
    pub(crate) async fn create_cart_item(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cart: CartUuid,
        item: NewCartItem,
    ) -> Result<CartItemRecord, sqlx::Error> {
        let created = query_as::<Postgres, CartItemRecord>(CREATE_CART_ITEM_SQL)
            .bind(item.uuid.into_uuid())
            .bind(cart.into_uuid())
            .bind(item.product_uuid.into_uuid())
            .fetch_one(&mut **tx)
            .await?;

        debug!(
            cart_uuid = %cart,
            item_uuid = %created.uuid,
            product_uuid = %created.product_uuid,
            "created cart item"
        );

        Ok(created)
    }

    #[tracing::instrument(
        name = "carts.items_repository.delete_cart_item",
        skip(self, tx),
        fields(cart_uuid = %cart, item_uuid = %item),
        err
    )]
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

        debug!(cart_uuid = %cart, item_uuid = %item, rows_affected, "deleted cart item rows");

        Ok(rows_affected)
    }
}

impl<'r> FromRow<'r, PgRow> for CartItemRecord {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        let price = try_get_amount(row, "price")?;

        Ok(Self {
            uuid: CartItemUuid::from_uuid(row.try_get("uuid")?),
            price,
            product_uuid: ProductUuid::from_uuid(row.try_get("product_uuid")?),
            created_at: row.try_get::<SqlxTimestamp, _>("created_at")?.to_jiff(),
            updated_at: row.try_get::<SqlxTimestamp, _>("updated_at")?.to_jiff(),
            deleted_at: row
                .try_get::<Option<SqlxTimestamp>, _>("deleted_at")?
                .map(SqlxTimestamp::to_jiff),
        })
    }
}
