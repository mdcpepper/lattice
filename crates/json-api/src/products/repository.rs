//! Products Repository

use std::num::TryFromIntError;

use async_trait::async_trait;
use jiff_sqlx::Timestamp as SqlxTimestamp;
use mockall::automock;
use sqlx::{
    Error, FromRow, PgPool, Postgres, Row,
    error::{DatabaseError, ErrorKind},
    postgres::PgRow,
    query_as,
};
use thiserror::Error;

use crate::products::models::{NewProduct, Product};

const CREATE_PRODUCT_SQL: &str = include_str!("sql/create_product.sql");
const GET_PRODUCTS_SQL: &str = include_str!("sql/get_products.sql");

#[derive(Debug, Error)]
pub enum ProductsRepositoryError {
    #[error("product already exists")]
    AlreadyExists,

    #[error("related resource not found")]
    InvalidReference,

    #[error("missing required data")]
    MissingRequiredData,

    #[error("invalid data")]
    InvalidData,

    #[error("storage error")]
    Sql(#[source] Error),

    #[error("invalid price value")]
    InvalidPrice(#[from] TryFromIntError),
}

impl From<Error> for ProductsRepositoryError {
    fn from(error: Error) -> Self {
        match error.as_database_error().map(DatabaseError::kind) {
            Some(ErrorKind::UniqueViolation) => Self::AlreadyExists,
            Some(ErrorKind::ForeignKeyViolation) => Self::InvalidReference,
            Some(ErrorKind::NotNullViolation) => Self::MissingRequiredData,
            Some(ErrorKind::CheckViolation) => Self::InvalidData,
            Some(ErrorKind::Other | _) | None => Self::Sql(error),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PgProductsRepository {
    pool: PgPool,
}

impl PgProductsRepository {
    #[must_use]
    pub(crate) fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl<'r> FromRow<'r, PgRow> for Product {
    fn from_row(row: &'r PgRow) -> sqlx::Result<Self> {
        let price_i64: i64 = row.try_get("price")?;

        let price = u64::try_from(price_i64).map_err(|e| Error::ColumnDecode {
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

#[async_trait]
impl ProductsRepository for PgProductsRepository {
    async fn create_product(
        &self,
        product: NewProduct,
    ) -> Result<Product, ProductsRepositoryError> {
        query_as::<Postgres, Product>(CREATE_PRODUCT_SQL)
            .bind(product.uuid)
            .bind(i64::try_from(product.price)?)
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    async fn get_products(&self) -> Result<Vec<Product>, ProductsRepositoryError> {
        query_as::<Postgres, Product>(GET_PRODUCTS_SQL)
            .fetch_all(&self.pool)
            .await
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_price_rejects_negative() {
        let result = u64::try_from(-1_i64);

        assert!(result.is_err());
    }
}

#[automock]
#[async_trait]
pub(crate) trait ProductsRepository: Send + Sync {
    async fn create_product(&self, product: NewProduct)
    -> Result<Product, ProductsRepositoryError>;
    async fn get_products(&self) -> Result<Vec<Product>, ProductsRepositoryError>;
}
