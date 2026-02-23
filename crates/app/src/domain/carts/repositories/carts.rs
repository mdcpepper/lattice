//! Carts Repository

use jiff::Timestamp;
use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{FromRow, Postgres, Row, Transaction, postgres::PgRow, query, query_as};

use crate::domain::carts::models::{Cart, CartUuid};

const GET_CART_SQL: &str = include_str!("../sql/get_cart.sql");
const CREATE_CART_SQL: &str = include_str!("../sql/create_cart.sql");
const DELETE_CART_SQL: &str = include_str!("../sql/delete_cart.sql");

#[derive(Debug, Clone, Default)]
pub(crate) struct PgCartsRepository;

impl PgCartsRepository {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn get_cart(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cart: CartUuid,
        point_in_time: Timestamp,
    ) -> Result<Cart, sqlx::Error> {
        query_as::<Postgres, Cart>(GET_CART_SQL)
            .bind(cart.into_uuid())
            .bind(SqlxTimestamp::from(point_in_time))
            .fetch_one(&mut **tx)
            .await
    }

    pub(crate) async fn create_cart(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cart: CartUuid,
    ) -> Result<Cart, sqlx::Error> {
        query_as::<Postgres, Cart>(CREATE_CART_SQL)
            .bind(cart.into_uuid())
            .fetch_one(&mut **tx)
            .await
    }

    pub(crate) async fn delete_cart(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cart: CartUuid,
    ) -> Result<u64, sqlx::Error> {
        let rows_affected = query(DELETE_CART_SQL)
            .bind(cart.into_uuid())
            .execute(&mut **tx)
            .await?
            .rows_affected();

        Ok(rows_affected)
    }
}

impl<'r> FromRow<'r, PgRow> for Cart {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        let subtotal = try_get_amount(row, "subtotal")?;
        let total = try_get_amount(row, "total")?;

        let cart_items_count: i64 = row.try_get("cart_items_count")?;

        Ok(Self {
            uuid: CartUuid::from_uuid(row.try_get("uuid")?),
            subtotal,
            total,
            items: Vec::with_capacity(cart_items_count as usize),
            created_at: row.try_get::<SqlxTimestamp, _>("created_at")?.to_jiff(),
            updated_at: row.try_get::<SqlxTimestamp, _>("updated_at")?.to_jiff(),
            deleted_at: row
                .try_get::<Option<SqlxTimestamp>, _>("deleted_at")?
                .map(SqlxTimestamp::to_jiff),
        })
    }
}

pub(super) fn try_get_amount(row: &PgRow, col: &str) -> Result<u64, sqlx::Error> {
    let amount_i64: i64 = row.try_get(col)?;

    u64::try_from(amount_i64).map_err(|e| sqlx::Error::ColumnDecode {
        index: col.to_string(),
        source: Box::new(e),
    })
}
