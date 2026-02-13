//! Promotion Applications

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    flags::DataType,
    prelude::*,
    types::Zval,
};

use crate::{
    items::ItemRef, money::MoneyRef, promotions::direct_discount::DirectDiscountPromotionRef,
};

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\PromotionApplication")]
pub struct PromotionApplication {
    #[php(prop)]
    promotion: DirectDiscountPromotionRef,

    #[php(prop)]
    item: ItemRef,

    #[php(prop)]
    bundle_id: usize,

    #[php(prop)]
    original_price: MoneyRef,

    #[php(prop)]
    final_price: MoneyRef,
}

#[php_impl]
impl PromotionApplication {
    pub fn __construct(
        promotion: DirectDiscountPromotionRef,
        item: ItemRef,
        bundle_id: usize,
        original_price: MoneyRef,
        final_price: MoneyRef,
    ) -> Self {
        Self {
            promotion,
            item,
            bundle_id,
            original_price,
            final_price,
        }
    }
}

#[derive(Debug)]
pub struct PromotionApplicationRef(Zval);

impl PromotionApplicationRef {
    pub fn from_application(application: PromotionApplication) -> Self {
        let mut zv = Zval::new();

        application
            .set_zval(&mut zv, false)
            .expect("promotion application should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for PromotionApplicationRef {
    const TYPE: DataType =
        DataType::Object(Some(<PromotionApplication as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<PromotionApplication>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for PromotionApplicationRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for PromotionApplicationRef {
    const TYPE: DataType =
        DataType::Object(Some(<PromotionApplication as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&PromotionApplicationRef> for PromotionApplication {
    type Error = PhpException;

    fn try_from(value: &PromotionApplicationRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "PromotionApplication object is invalid.".to_string(),
            ));
        };

        let item = obj.get_property::<ItemRef>("item").map_err(|_| {
            PhpException::default("PromotionApplication item is invalid.".to_string())
        })?;

        let promotion = obj
            .get_property::<DirectDiscountPromotionRef>("promotion")
            .map_err(|_| {
                PhpException::default("PromotionApplication promotion is invalid.".to_string())
            })?;

        let bundle_id = obj.get_property::<usize>("bundle_id").map_err(|_| {
            PhpException::default("PromotionApplication bundle_id is invalid.".to_string())
        })?;

        let original_price = obj
            .get_property::<MoneyRef>("original_price")
            .map_err(|_| {
                PhpException::default("PromotionApplication original_price is invalid.".to_string())
            })?;

        let final_price = obj.get_property::<MoneyRef>("final_price").map_err(|_| {
            PhpException::default("PromotionApplication final_price is invalid.".to_string())
        })?;

        Ok(Self {
            promotion,
            item,
            bundle_id,
            original_price,
            final_price,
        })
    }
}

impl TryFrom<PromotionApplicationRef> for PromotionApplication {
    type Error = PhpException;

    fn try_from(value: PromotionApplicationRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}
