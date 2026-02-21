//! Products Repository

use jiff_sqlx::Timestamp as SqlxTimestamp;
use sqlx::{FromRow, Postgres, Row, Transaction, postgres::PgRow, query, query_as};
use uuid::Uuid;

use crate::products::{
    errors::ProductsServiceError,
    models::{NewProduct, Product, ProductUpdate},
};

const GET_PRODUCTS_SQL: &str = include_str!("sql/get_products.sql");
const CREATE_PRODUCT_SQL: &str = include_str!("sql/create_product.sql");
const UPDATE_PRODUCT_SQL: &str = include_str!("sql/update_product.sql");
const DELETE_PRODUCT_SQL: &str = include_str!("sql/delete_product.sql");

#[derive(Debug, Clone, Default)]
pub(crate) struct PgProductsRepository;

impl PgProductsRepository {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) async fn get_products(
        &self,
        tx: &mut Transaction<'_, Postgres>,
    ) -> Result<Vec<Product>, ProductsServiceError> {
        query_as::<Postgres, Product>(GET_PRODUCTS_SQL)
            .fetch_all(&mut **tx)
            .await
            .map_err(ProductsServiceError::from)
    }

    pub(crate) async fn create_product(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        product: NewProduct,
    ) -> Result<Product, ProductsServiceError> {
        query_as::<Postgres, Product>(CREATE_PRODUCT_SQL)
            .bind(product.uuid)
            .bind(i64::try_from(product.price)?)
            .fetch_one(&mut **tx)
            .await
            .map_err(ProductsServiceError::from)
    }

    pub(crate) async fn update_product(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        uuid: Uuid,
        update: ProductUpdate,
    ) -> Result<Product, ProductsServiceError> {
        query_as::<Postgres, Product>(UPDATE_PRODUCT_SQL)
            .bind(uuid)
            .bind(i64::try_from(update.price)?)
            .fetch_one(&mut **tx)
            .await
            .map_err(ProductsServiceError::from)
    }

    pub(crate) async fn delete_product(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        uuid: Uuid,
    ) -> Result<(), ProductsServiceError> {
        let result = query(DELETE_PRODUCT_SQL)
            .bind(uuid)
            .execute(&mut **tx)
            .await
            .map_err(ProductsServiceError::from)?;

        if result.rows_affected() == 0 {
            return Err(ProductsServiceError::NotFound);
        }

        Ok(())
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
