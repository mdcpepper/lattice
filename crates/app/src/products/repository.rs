//! Products Repository

use jiff::Timestamp;
use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{FromRow, Postgres, Row, Transaction, postgres::PgRow, query, query_as};
use uuid::Uuid;

use crate::products::models::Product;

const LIST_PRODUCTS_SQL: &str = include_str!("sql/list_products.sql");
const GET_PRODUCT_SQL: &str = include_str!("sql/get_product.sql");
const CREATE_PRODUCT_INSERT_SQL: &str = include_str!("sql/create_product_insert.sql");
const CREATE_PRODUCT_DETAIL_INSERT_SQL: &str = include_str!("sql/create_product_detail_insert.sql");
const UPDATE_PRODUCT_SQL: &str = include_str!("sql/update_product.sql");
const DELETE_PRODUCT_SQL: &str = include_str!("sql/delete_product.sql");

#[derive(Debug, Clone, Default)]
pub(crate) struct PgProductsRepository;

impl PgProductsRepository {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn list_products(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        point_in_time: Timestamp,
    ) -> Result<Vec<Product>, sqlx::Error> {
        query_as::<Postgres, Product>(LIST_PRODUCTS_SQL)
            .bind(SqlxTimestamp::from(point_in_time))
            .fetch_all(&mut **tx)
            .await
    }

    pub(crate) async fn get_product(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        uuid: Uuid,
        point_in_time: Timestamp,
    ) -> Result<Product, sqlx::Error> {
        query_as::<Postgres, Product>(GET_PRODUCT_SQL)
            .bind(uuid)
            .bind(SqlxTimestamp::from(point_in_time))
            .fetch_one(&mut **tx)
            .await
    }

    pub(crate) async fn create_product(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        uuid: Uuid,
        price: i64,
    ) -> Result<Product, sqlx::Error> {
        let (created_uuid, created_at, updated_at, deleted_at): (
            Uuid,
            SqlxTimestamp,
            SqlxTimestamp,
            Option<SqlxTimestamp>,
        ) = query_as(CREATE_PRODUCT_INSERT_SQL)
            .bind(uuid)
            .fetch_one(&mut **tx)
            .await?;

        query(CREATE_PRODUCT_DETAIL_INSERT_SQL)
            .bind(created_uuid)
            .bind(price)
            .bind(created_at)
            .execute(&mut **tx)
            .await?;

        let price = u64::try_from(price).map_err(|e| sqlx::Error::ColumnDecode {
            index: "price".to_string(),
            source: Box::new(e),
        })?;

        Ok(Product {
            uuid: created_uuid,
            price,
            created_at: created_at.to_jiff(),
            updated_at: updated_at.to_jiff(),
            deleted_at: deleted_at.map(SqlxTimestamp::to_jiff),
        })
    }

    pub(crate) async fn update_product(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        uuid: Uuid,
        product_detail_uuid: Option<Uuid>,
        price: i64,
    ) -> Result<Product, sqlx::Error> {
        let product_detail_uuid = product_detail_uuid.unwrap_or_else(Uuid::now_v7);

        query_as::<Postgres, Product>(UPDATE_PRODUCT_SQL)
            .bind(uuid)
            .bind(product_detail_uuid)
            .bind(price)
            .fetch_one(&mut **tx)
            .await
    }

    pub(crate) async fn delete_product(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        uuid: Uuid,
    ) -> Result<u64, sqlx::Error> {
        let rows_affected = query(DELETE_PRODUCT_SQL)
            .bind(uuid)
            .execute(&mut **tx)
            .await?
            .rows_affected();

        Ok(rows_affected)
    }
}

impl<'r> FromRow<'r, PgRow> for Product {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        let price_i64: i64 = row.try_get("price")?;

        let price = u64::try_from(price_i64).map_err(|e| sqlx::Error::ColumnDecode {
            index: "price".to_string(),
            source: Box::new(e),
        })?;

        Ok(Self {
            uuid: row.try_get("uuid")?,
            price,
            created_at: row.try_get::<SqlxTimestamp, _>("created_at")?.to_jiff(),
            updated_at: row.try_get::<SqlxTimestamp, _>("updated_at")?.to_jiff(),
            deleted_at: row
                .try_get::<Option<SqlxTimestamp>, _>("deleted_at")?
                .map(SqlxTimestamp::to_jiff),
        })
    }
}
