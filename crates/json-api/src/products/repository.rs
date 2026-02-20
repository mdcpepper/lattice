//! Products Repository

use std::num::TryFromIntError;

use async_trait::async_trait;
use jiff_sqlx::Timestamp as SqlxTimestamp;
use mockall::automock;
use sqlx::{FromRow, PgPool, error::ErrorKind, query_as};
use thiserror::Error;
use uuid::Uuid;

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
    Sql(#[source] sqlx::Error),

    #[error("invalid price value")]
    InvalidPrice(#[from] TryFromIntError),
}

impl From<sqlx::Error> for ProductsRepositoryError {
    fn from(error: sqlx::Error) -> Self {
        match error
            .as_database_error()
            .map(|database_error| database_error.kind())
        {
            Some(ErrorKind::UniqueViolation) => Self::AlreadyExists,
            Some(ErrorKind::ForeignKeyViolation) => Self::InvalidReference,
            Some(ErrorKind::NotNullViolation) => Self::MissingRequiredData,
            Some(ErrorKind::CheckViolation) => Self::InvalidData,
            Some(ErrorKind::Other) | Some(_) | None => Self::Sql(error),
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

#[derive(Debug, FromRow)]
struct ProductRow {
    uuid: Uuid,
    price: i64,
    created_at: SqlxTimestamp,
    updated_at: SqlxTimestamp,
    deleted_at: Option<SqlxTimestamp>,
}

impl TryFrom<ProductRow> for Product {
    type Error = ProductsRepositoryError;

    fn try_from(row: ProductRow) -> Result<Self, Self::Error> {
        Ok(Self {
            uuid: row.uuid,
            price: u64::try_from(row.price)?,
            created_at: row.created_at.to_jiff(),
            updated_at: row.updated_at.to_jiff(),
            deleted_at: row.deleted_at.map(SqlxTimestamp::to_jiff),
        })
    }
}

#[async_trait]
impl ProductsRepository for PgProductsRepository {
    async fn create_product(
        &self,
        product: NewProduct,
    ) -> Result<Product, ProductsRepositoryError> {
        query_as::<_, ProductRow>(CREATE_PRODUCT_SQL)
            .bind(product.uuid)
            .bind(i64::try_from(product.price)?)
            .fetch_one(&self.pool)
            .await?
            .try_into()
    }

    async fn get_products(&self) -> Result<Vec<Product>, ProductsRepositoryError> {
        query_as::<_, ProductRow>(GET_PRODUCTS_SQL)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(TryInto::try_into)
            .collect()
    }
}

#[automock]
#[async_trait]
pub(crate) trait ProductsRepository: Send + Sync {
    async fn create_product(&self, product: NewProduct)
    -> Result<Product, ProductsRepositoryError>;
    async fn get_products(&self) -> Result<Vec<Product>, ProductsRepositoryError>;
}
