//! Items

use std::collections::HashSet;

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    exception::PhpException,
    flags::DataType,
    prelude::*,
    types::Zval,
};

use crate::{money::MoneyRef, products::ProductRef, reference_value::ReferenceValue};

#[derive(Debug)]
#[php_class]
#[php(name = "Lattice\\Item")]
pub struct Item {
    #[php(prop)]
    reference: ReferenceValue,

    #[php(prop)]
    name: String,

    #[php(prop)]
    price: MoneyRef,

    #[php(prop)]
    product: ProductRef,

    #[php(prop)]
    tags: HashSet<String>,
}

#[php_impl]
impl Item {
    pub fn __construct(
        reference: ReferenceValue,
        name: String,
        price: MoneyRef,
        product: ProductRef,
        tags: Option<HashSet<String>>,
    ) -> Self {
        Self {
            reference,
            name,
            price,
            product,
            tags: tags.unwrap_or_default(),
        }
    }

    pub fn from_product(reference: ReferenceValue, product: ProductRef) -> Self {
        Self {
            reference,
            name: product.name(),
            price: product.price(),
            tags: product.tags(),
            product,
        }
    }
}

impl Item {
    pub(crate) fn price(&self) -> MoneyRef {
        self.price.clone()
    }

    pub(crate) fn tags(&self) -> &HashSet<String> {
        &self.tags
    }
}

#[derive(Debug)]
pub struct ItemRef(Zval);

impl ItemRef {
    pub fn from_item(item: Item) -> Self {
        let mut zv = Zval::new();

        item.set_zval(&mut zv, false)
            .expect("item should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for ItemRef {
    const TYPE: DataType = DataType::Object(Some(<Item as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<Item>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for ItemRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for ItemRef {
    const TYPE: DataType = DataType::Object(Some(<Item as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&ItemRef> for Item {
    type Error = PhpException;

    fn try_from(value: &ItemRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default("Item object is invalid.".to_string()));
        };

        let reference = obj
            .get_property::<ReferenceValue>("reference")
            .map_err(|_| PhpException::default("Item reference is invalid.".to_string()))?;

        let name = obj
            .get_property::<String>("name")
            .map_err(|_| PhpException::default("Item name is invalid.".to_string()))?;

        let price = obj
            .get_property::<MoneyRef>("price")
            .map_err(|_| PhpException::default("Item price is invalid.".to_string()))?;

        let product = obj
            .get_property::<ProductRef>("product")
            .map_err(|_| PhpException::default("Item product is invalid.".to_string()))?;

        let tags = obj
            .get_property::<HashSet<String>>("tags")
            .map_err(|_| PhpException::default("Item tags are invalid.".to_string()))?;

        Ok(Self {
            reference,
            name,
            price,
            product,
            tags,
        })
    }
}

impl TryFrom<ItemRef> for Item {
    type Error = PhpException;

    fn try_from(value: ItemRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}
