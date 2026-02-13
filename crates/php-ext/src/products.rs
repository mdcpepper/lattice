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
#[php(name = "Lattice\\Product")]
pub struct Product {
    #[php(prop)]
    reference: ReferenceValue,

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
        reference: ReferenceValue,
        name: String,
        price: MoneyRef,
        tags: Option<HashSet<String>>,
    ) -> Self {
        Self {
            reference,
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
    const NULLABLE: bool = false;
    const TYPE: DataType = DataType::Object(Some(<Product as RegisteredClass>::CLASS_NAME));

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&ProductRef> for Product {
    type Error = PhpException;

    fn try_from(value: &ProductRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "Product object is invalid.".to_string(),
            ));
        };

        let reference = obj
            .get_property::<ReferenceValue>("reference")
            .map_err(|_| PhpException::default("Product reference is invalid.".to_string()))?;

        let name = obj
            .get_property::<String>("name")
            .map_err(|_| PhpException::default("Product name is invalid.".to_string()))?;

        let price = obj
            .get_property::<MoneyRef>("price")
            .map_err(|_| PhpException::default("Product price is invalid.".to_string()))?;

        let tags = obj
            .get_property::<HashSet<String>>("tags")
            .map_err(|_| PhpException::default("Product tags are invalid.".to_string()))?;

        Ok(Product {
            reference,
            name,
            price,
            tags,
        })
    }
}

impl TryFrom<ProductRef> for Product {
    type Error = PhpException;

    fn try_from(value: ProductRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}
