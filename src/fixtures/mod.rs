//! Fixtures

use std::{fs, path::PathBuf};

use rustc_hash::FxHashMap;
use slotmap::SlotMap;
use thiserror::Error;

use crate::{
    basket::Basket,
    fixtures::{items::ItemsFixture, promotions::PromotionsFixture},
    items::{Item, groups::ItemGroup},
    products::{Product, ProductKey},
    promotions::{Promotion, PromotionKey, PromotionMeta},
};

pub mod items;
pub mod products;
pub mod promotions;

/// Fixture Parsing Errors
#[derive(Debug, Error)]
pub enum FixtureError {
    /// IO error reading fixture files
    #[error("Failed to read fixture file: {0}")]
    Io(#[from] std::io::Error),

    /// YAML parsing error
    #[error("Failed to parse YAML: {0}")]
    Yaml(#[from] serde_norway::Error),

    /// Invalid price format
    #[error("Invalid price format: {0}")]
    InvalidPrice(String),

    /// Invalid percentage format
    #[error("Invalid percentage format: {0}")]
    InvalidPercentage(String),

    /// Unknown currency code
    #[error("Unknown currency code: {0}")]
    UnknownCurrency(String),

    /// Product not found
    #[error("Product not found: {0}")]
    ProductNotFound(String),

    /// Item not found
    #[error("Item not found: {0}")]
    ItemNotFound(String),

    /// Promotion not found
    #[error("Promotion not found: {0}")]
    PromotionNotFound(String),

    /// Unsupported promotion type
    #[error("Unsupported promotion type: {0}")]
    UnsupportedPromotionType(String),

    /// Invalid promotion data
    #[error("Invalid promotion data: {0}")]
    InvalidPromotionData(String),

    /// Currency mismatch between products
    #[error("Currency mismatch: expected {0}, found {1}")]
    CurrencyMismatch(String, String),

    /// No products loaded yet
    #[error("No products loaded yet; currency unknown")]
    NoCurrency,

    /// No items loaded
    #[error("No items loaded; cannot create basket or item group")]
    NoItems,

    /// Not enough items in fixture
    #[error("Not enough items in fixture, available: {available}, requested: {requested}")]
    NotEnoughItems {
        /// Number of items defined in the fixture
        available: usize,
        /// Number of items requested
        requested: usize,
    },

    /// Basket creation error
    #[error("Failed to create basket: {0}")]
    Basket(#[from] crate::basket::BasketError),
}

/// Fixture
#[derive(Debug)]
pub struct Fixture<'a> {
    /// Base path for fixture files
    base_path: PathBuf,

    /// `SlotMaps` to store the actual types with generated keys
    product_meta: SlotMap<ProductKey, Product<'a>>,
    promotion_meta: SlotMap<PromotionKey, PromotionMeta>,

    /// String key -> `SlotMap` key mappings for lookups
    product_keys: FxHashMap<String, ProductKey>,
    promotion_keys: FxHashMap<String, PromotionKey>,

    /// Pre-built items (reference products by `ProductKey`)
    items: Vec<Item<'a>>,

    /// Pre-built promotions
    promotions: Vec<Promotion<'a>>,

    /// Currency for the fixture set
    currency: Option<&'static rusty_money::iso::Currency>,
}

impl<'a> Fixture<'a> {
    /// Create a new empty fixture with default base path
    pub fn new() -> Self {
        Self::with_base_path("./fixtures")
    }

    /// Create a new empty fixture with custom base path
    pub fn with_base_path(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
            product_meta: SlotMap::with_key(),
            promotion_meta: SlotMap::with_key(),
            product_keys: FxHashMap::default(),
            promotion_keys: FxHashMap::default(),
            items: Vec::new(),
            promotions: Vec::new(),
            currency: None,
        }
    }

    /// Load products from a YAML fixture file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, parsed, or if there are currency mismatches.
    pub fn load_products(&mut self, name: &str) -> Result<&mut Self, FixtureError> {
        let file_path = self.base_path.join("products").join(format!("{name}.yml"));
        let contents = fs::read_to_string(&file_path)?;
        let fixture: products::ProductsFixture = serde_norway::from_str(&contents)?;

        for (key, product_fixture) in fixture.products {
            // Parse to get currency first (before creating Product)
            let (_minor_units, currency) = products::parse_price(&product_fixture.price)?;

            // Validate currency consistency
            if let Some(existing_currency) = self.currency {
                if existing_currency != currency {
                    return Err(FixtureError::CurrencyMismatch(
                        existing_currency.iso_alpha_code.to_string(),
                        currency.iso_alpha_code.to_string(),
                    ));
                }
            } else {
                self.currency = Some(currency);
            }

            // Now create the product
            let product: Product<'a> = product_fixture.try_into()?;
            let product_key = self.product_meta.insert(product);

            self.product_keys.insert(key, product_key);
        }

        Ok(self)
    }

    /// Load items from a YAML fixture file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, parsed, or if referenced products don't exist.
    pub fn load_items(&mut self, name: &str) -> Result<&mut Self, FixtureError> {
        let file_path = self.base_path.join("items").join(format!("{name}.yml"));
        let contents = fs::read_to_string(&file_path)?;
        let fixture: ItemsFixture = serde_norway::from_str(&contents)?;

        for product_key_str in fixture.items {
            let product_key = self
                .product_keys
                .get(&product_key_str)
                .ok_or_else(|| FixtureError::ProductNotFound(product_key_str.clone()))?;

            let product = self
                .product_meta
                .get(*product_key)
                .ok_or_else(|| FixtureError::ProductNotFound(product_key_str.clone()))?;

            let item = Item::with_tags(*product_key, product.price, product.tags.clone());

            self.items.push(item);
        }

        Ok(self)
    }

    /// Load promotions from a YAML fixture file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, parsed, or if the promotion type is unsupported.
    pub fn load_promotions(&mut self, name: &str) -> Result<&mut Self, FixtureError> {
        let file_path = self
            .base_path
            .join("promotions")
            .join(format!("{name}.yml"));

        let contents = fs::read_to_string(&file_path)?;
        let fixture: PromotionsFixture = serde_norway::from_str(&contents)?;

        for (key, promotion_fixture) in fixture.promotions {
            let promotion_key = self.promotion_meta.insert(PromotionMeta {
                name: String::new(),
            });

            let (meta, promotion) = promotion_fixture.try_into_promotion(promotion_key)?;

            if let Some(meta_slot) = self.promotion_meta.get_mut(promotion_key) {
                *meta_slot = meta;
            }

            self.promotions.push(promotion);
            self.promotion_keys.insert(key, promotion_key);
        }

        Ok(self)
    }

    /// Load a complete fixture set (products, items, and promotions with the same name)
    ///
    /// # Errors
    ///
    /// Returns an error if any of the fixture files cannot be loaded.
    pub fn from_set(name: &str) -> Result<Self, FixtureError> {
        let mut fixture = Self::new();

        fixture
            .load_products(name)?
            .load_items(name)?
            .load_promotions(name)?;

        Ok(fixture)
    }

    /// Get a product by its string key
    ///
    /// # Errors
    ///
    /// Returns an error if the product is not found.
    pub fn product(&self, key: &str) -> Result<&Product<'a>, FixtureError> {
        let product_key = self
            .product_keys
            .get(key)
            .ok_or_else(|| FixtureError::ProductNotFound(key.to_string()))?;

        self.product_meta
            .get(*product_key)
            .ok_or_else(|| FixtureError::ProductNotFound(key.to_string()))
    }

    /// Get a product key by its string key
    ///
    /// # Errors
    ///
    /// Returns an error if the product is not found.
    pub fn product_key(&self, key: &str) -> Result<ProductKey, FixtureError> {
        self.product_keys
            .get(key)
            .copied()
            .ok_or_else(|| FixtureError::ProductNotFound(key.to_string()))
    }

    /// Get a promotion by its string key
    ///
    /// # Errors
    ///
    /// Returns an error if the promotion is not found.
    pub fn promotion(&self, key: &str) -> Result<&Promotion<'a>, FixtureError> {
        let promotion_key = self
            .promotion_keys
            .get(key)
            .ok_or_else(|| FixtureError::PromotionNotFound(key.to_string()))?;

        self.promotions
            .iter()
            .find(|p| p.key() == *promotion_key)
            .ok_or_else(|| FixtureError::PromotionNotFound(key.to_string()))
    }

    /// Get promotion metadata by its string key
    ///
    /// # Errors
    ///
    /// Returns an error if the promotion is not found.
    pub fn promotion_meta(&self, key: &str) -> Result<&PromotionMeta, FixtureError> {
        let promotion_key = self
            .promotion_keys
            .get(key)
            .ok_or_else(|| FixtureError::PromotionNotFound(key.to_string()))?;

        self.promotion_meta
            .get(*promotion_key)
            .ok_or_else(|| FixtureError::PromotionNotFound(key.to_string()))
    }

    /// Get all items
    pub fn items(&self) -> &[Item<'a>] {
        &self.items
    }

    /// Get all promotions
    pub fn promotions(&self) -> &[Promotion<'a>] {
        &self.promotions
    }

    /// Create a basket from the loaded items
    ///
    /// # Errors
    ///
    /// Returns an error if no items are loaded or if basket creation fails.
    pub fn basket(&self, n: Option<usize>) -> Result<Basket<'a>, FixtureError> {
        let currency = self.currency.ok_or(FixtureError::NoCurrency)?;

        if self.items.is_empty() {
            return Err(FixtureError::NoItems);
        }

        if let Some(n) = n
            && n > self.items.len()
        {
            return Err(FixtureError::NotEnoughItems {
                requested: n,
                available: self.items.len(),
            });
        }

        let items: Vec<Item<'_>> = self
            .items
            .iter()
            .take(n.unwrap_or(self.items.len()))
            .cloned()
            .collect();

        Ok(Basket::with_items(items, currency)?)
    }

    /// Create an item group from the loaded items
    ///
    /// # Errors
    ///
    /// Returns an error if no items are loaded.
    pub fn item_group(&self) -> Result<ItemGroup<'a>, FixtureError> {
        let currency = self.currency.ok_or(FixtureError::NoCurrency)?;

        if self.items.is_empty() {
            return Err(FixtureError::NoItems);
        }

        let items = self.items.iter().cloned().collect();

        Ok(ItemGroup::new(items, currency))
    }

    /// Get the currency
    ///
    /// # Errors
    ///
    /// Returns an error if no products have been loaded yet.
    pub fn currency(&self) -> Result<&'static rusty_money::iso::Currency, FixtureError> {
        self.currency.ok_or(FixtureError::NoCurrency)
    }

    /// Get the product metadata `SlotMap`
    pub fn product_meta_map(&self) -> &SlotMap<ProductKey, Product<'a>> {
        &self.product_meta
    }

    /// Get the promotion metadata `SlotMap`
    pub fn promotion_meta_map(&self) -> &SlotMap<PromotionKey, PromotionMeta> {
        &self.promotion_meta
    }
}

impl Default for Fixture<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::Path};

    use rusty_money::iso::GBP;
    use testresult::TestResult;

    use super::*;

    fn write_fixture(base: &Path, category: &str, name: &str, contents: &str) -> TestResult {
        let dir = base.join(category);

        fs::create_dir_all(&dir)?;
        fs::write(dir.join(format!("{name}.yml")), contents)?;

        Ok(())
    }

    #[test]
    fn fixture_loads_products_items_and_promotions() -> TestResult {
        let mut fixture = Fixture::new();

        fixture
            .load_products("direct")?
            .load_items("direct")?
            .load_promotions("direct")?;

        // Check products were loaded
        assert_eq!(fixture.product_keys.len(), 3);

        let sandwich = fixture.product("sandwich")?;

        assert_eq!(sandwich.name, "Sandwich");
        assert_eq!(sandwich.price.to_minor_units(), 299);

        // Check items were loaded
        assert_eq!(fixture.items.len(), 3);

        // Check promotions were loaded
        assert_eq!(fixture.promotions.len(), 2);

        // Check currency was set
        assert_eq!(fixture.currency()?, GBP);

        Ok(())
    }

    #[test]
    fn fixture_from_set_loads_all_fixtures() -> TestResult {
        let fixture = Fixture::from_set("direct")?;

        assert_eq!(fixture.product_keys.len(), 3);
        assert_eq!(fixture.items.len(), 3);
        assert_eq!(fixture.promotions.len(), 2);

        Ok(())
    }

    #[test]
    fn fixture_basket_creates_basket_from_all_items() -> TestResult {
        let fixture = Fixture::from_set("direct")?;
        let basket = fixture.basket(None)?;

        assert_eq!(basket.len(), 3);
        assert_eq!(basket.currency(), GBP);

        Ok(())
    }

    #[test]
    fn fixture_basket_creates_basket_from_first_n_items() -> TestResult {
        let fixture = Fixture::from_set("direct")?;
        let basket = fixture.basket(Some(2))?;

        assert_eq!(basket.len(), 2);

        Ok(())
    }

    #[test]
    fn fixture_basket_rejects_request_for_too_many_items() -> TestResult {
        let fixture = Fixture::from_set("direct")?;
        let result = fixture.basket(Some(10));

        assert!(matches!(
            result,
            Err(FixtureError::NotEnoughItems {
                requested: 10,
                available: 3
            })
        ));

        Ok(())
    }

    #[test]
    fn fixture_item_group_creates_item_group_from_items() -> TestResult {
        let fixture = Fixture::from_set("direct")?;
        let item_group = fixture.item_group()?;

        assert_eq!(item_group.len(), 3);
        assert_eq!(item_group.currency(), GBP);

        Ok(())
    }

    #[test]
    fn fixture_product_not_found_returns_error() {
        let fixture = Fixture::new();
        let result = fixture.product("nonexistent");

        assert!(matches!(result, Err(FixtureError::ProductNotFound(_))));
    }

    #[test]
    fn fixture_no_items_returns_error() -> TestResult {
        let mut fixture = Fixture::new();

        fixture.load_products("direct")?;

        let result = fixture.basket(None);

        assert!(matches!(result, Err(FixtureError::NoItems)));

        Ok(())
    }

    #[test]
    fn fixture_no_currency_returns_error() {
        let fixture = Fixture::new();
        let result = fixture.currency();

        assert!(matches!(result, Err(FixtureError::NoCurrency)));
    }

    #[test]
    fn fixture_load_products_rejects_currency_mismatch() -> TestResult {
        let unique = format!(
            "dante-fixtures-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );

        let base_path = env::temp_dir().join(unique);

        write_fixture(
            &base_path,
            "products",
            "usd_set",
            "products:\n  apple:\n    name: Apple\n    tags: []\n    price: 1.00 USD\n",
        )?;

        write_fixture(
            &base_path,
            "products",
            "gbp_set",
            "products:\n  banana:\n    name: Banana\n    tags: []\n    price: 1.00 GBP\n",
        )?;

        let mut fixture = Fixture::with_base_path(&base_path);

        fixture.load_products("usd_set")?;

        let result = fixture.load_products("gbp_set");

        assert!(matches!(result, Err(FixtureError::CurrencyMismatch(_, _))));

        Ok(())
    }

    #[test]
    fn fixture_product_key_not_found_returns_error() {
        let fixture = Fixture::new();
        let result = fixture.product_key("nonexistent");

        assert!(matches!(result, Err(FixtureError::ProductNotFound(_))));
    }

    #[test]
    fn fixture_promotion_not_found_returns_error() {
        let fixture = Fixture::new();
        let result = fixture.promotion("missing");

        assert!(matches!(result, Err(FixtureError::PromotionNotFound(_))));
    }

    #[test]
    fn fixture_promotion_missing_from_list_returns_error() {
        let mut fixture = Fixture::new();

        fixture
            .promotion_keys
            .insert("missing-key".to_string(), PromotionKey::default());

        let result = fixture.promotion("missing-key");

        assert!(matches!(result, Err(FixtureError::PromotionNotFound(_))));
    }

    #[test]
    fn fixture_promotion_meta_not_found_returns_error() {
        let fixture = Fixture::new();
        let result = fixture.promotion_meta("missing");

        assert!(matches!(result, Err(FixtureError::PromotionNotFound(_))));
    }

    #[test]
    fn fixture_promotion_meta_missing_slot_returns_error() {
        let mut fixture = Fixture::new();

        fixture
            .promotion_keys
            .insert("missing-key".to_string(), PromotionKey::default());

        let result = fixture.promotion_meta("missing-key");

        assert!(matches!(result, Err(FixtureError::PromotionNotFound(_))));
    }

    #[test]
    fn fixture_items_and_promotions_accessors_return_loaded_data() -> TestResult {
        let fixture = Fixture::from_set("direct")?;

        assert_eq!(fixture.items().len(), 3);
        assert_eq!(fixture.promotions().len(), 2);

        Ok(())
    }

    #[test]
    fn fixture_item_group_no_items_returns_error() -> TestResult {
        let mut fixture = Fixture::new();

        fixture.load_products("direct")?;

        let result = fixture.item_group();

        assert!(matches!(result, Err(FixtureError::NoItems)));

        Ok(())
    }

    #[test]
    fn fixture_meta_maps_are_exposed() -> TestResult {
        let fixture = Fixture::from_set("direct")?;

        assert_eq!(fixture.product_meta_map().len(), 3);
        assert_eq!(fixture.promotion_meta_map().len(), 2);

        Ok(())
    }

    #[test]
    fn fixture_default_matches_new() {
        let fixture = Fixture::default();

        assert_eq!(fixture.base_path, PathBuf::from("./fixtures"));
        assert!(fixture.items.is_empty());
        assert!(fixture.promotions.is_empty());
    }
}
