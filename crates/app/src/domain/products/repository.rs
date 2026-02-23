//! Products Repository

use jiff::Timestamp;
use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{FromRow, Postgres, Row, Transaction, postgres::PgRow, query, query_as};
use uuid::Uuid;

use crate::domain::products::models::{Product, ProductDetailsUuid, ProductUuid};

const LIST_PRODUCTS_SQL: &str = include_str!("sql/list_products.sql");
const GET_PRODUCT_SQL: &str = include_str!("sql/get_product.sql");
const CREATE_PRODUCT_SQL: &str = include_str!("sql/create_product.sql");
const CREATE_PRODUCT_DETAIL_SQL: &str = include_str!("sql/create_product_detail.sql");
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
        product: ProductUuid,
        point_in_time: Timestamp,
    ) -> Result<Product, sqlx::Error> {
        query_as::<Postgres, Product>(GET_PRODUCT_SQL)
            .bind(product.into_uuid())
            .bind(SqlxTimestamp::from(point_in_time))
            .fetch_one(&mut **tx)
            .await
    }

    pub(crate) async fn create_product(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        product: ProductUuid,
        price: u64,
    ) -> Result<Product, sqlx::Error> {
        let price_i64 = i64::try_from(price).map_err(|e| sqlx::Error::ColumnDecode {
            index: "price".to_string(),
            source: Box::new(e),
        })?;

        let (created_uuid, created_at, updated_at, deleted_at): (
            Uuid,
            SqlxTimestamp,
            SqlxTimestamp,
            Option<SqlxTimestamp>,
        ) = query_as(CREATE_PRODUCT_SQL)
            .bind(product.into_uuid())
            .fetch_one(&mut **tx)
            .await?;

        query(CREATE_PRODUCT_DETAIL_SQL)
            .bind(created_uuid)
            .bind(price_i64)
            .bind(created_at)
            .execute(&mut **tx)
            .await?;

        Ok(Product {
            uuid: product,
            price,
            created_at: created_at.to_jiff(),
            updated_at: updated_at.to_jiff(),
            deleted_at: deleted_at.map(SqlxTimestamp::to_jiff),
        })
    }

    pub(crate) async fn update_product(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        product: ProductUuid,
        product_details: Option<ProductDetailsUuid>,
        price: u64,
    ) -> Result<Product, sqlx::Error> {
        let product_details = product_details.unwrap_or_else(ProductDetailsUuid::new);

        let price_i64 = i64::try_from(price).map_err(|e| sqlx::Error::ColumnDecode {
            index: "price".to_string(),
            source: Box::new(e),
        })?;

        query_as::<Postgres, Product>(UPDATE_PRODUCT_SQL)
            .bind(product.into_uuid())
            .bind(product_details.into_uuid())
            .bind(price_i64)
            .fetch_one(&mut **tx)
            .await
    }

    pub(crate) async fn delete_product(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        product: ProductUuid,
    ) -> Result<u64, sqlx::Error> {
        let rows_affected = query(DELETE_PRODUCT_SQL)
            .bind(product.into_uuid())
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
            uuid: ProductUuid::from_uuid(row.try_get("uuid")?),
            price,
            created_at: row.try_get::<SqlxTimestamp, _>("created_at")?.to_jiff(),
            updated_at: row.try_get::<SqlxTimestamp, _>("updated_at")?.to_jiff(),
            deleted_at: row
                .try_get::<Option<SqlxTimestamp>, _>("deleted_at")?
                .map(SqlxTimestamp::to_jiff),
        })
    }
}
