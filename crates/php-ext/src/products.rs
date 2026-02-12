//! Product

use std::collections::HashSet;

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    flags::DataType,
    prelude::*,
    types::Zval,
};

use crate::{money::MoneyRef, reference_value::ReferenceValue};

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "FeedCode\\Lattice\\Product")]
pub struct Product {
    #[php(prop)]
    key: ReferenceValue,

    #[php(prop)]
    name: String,

    #[php(prop)]
    price: MoneyRef,

    #[php(prop)]
    tags: HashSet<String>,
}

#[php_impl]
impl Product {
    pub fn __construct(
        key: ReferenceValue,
        name: String,
        price: MoneyRef,
        tags: Option<HashSet<String>>,
    ) -> Self {
        Self {
            key,
            name,
            price,
            tags: tags.unwrap_or_default(),
        }
    }
}

#[derive(Debug)]
pub struct ProductRef(Zval);

impl ProductRef {
    pub fn from_product(product: Product) -> Self {
        let mut zv = Zval::new();

        product
            .set_zval(&mut zv, false)
            .expect("product should always convert to object zval");

        Self(zv)
    }

    pub fn name(&self) -> String {
        self.0
            .object()
            .and_then(|obj| obj.get_property::<String>("name").ok())
            .unwrap_or_default()
    }

    pub fn price(&self) -> MoneyRef {
        self.0
            .object()
            .and_then(|obj| obj.get_property::<MoneyRef>("price").ok())
            .expect("product object is missing valid Money price property")
    }

    pub fn tags(&self) -> HashSet<String> {
        self.0
            .object()
            .and_then(|obj| obj.get_property::<HashSet<String>>("tags").ok())
            .unwrap_or_default()
    }
}

impl<'a> FromZval<'a> for ProductRef {
    const TYPE: DataType = DataType::Object(Some(<Product as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<Product>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for ProductRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for ProductRef {
    const TYPE: DataType = DataType::Object(Some(<Product as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}
