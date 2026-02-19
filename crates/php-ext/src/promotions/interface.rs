//! Promotion marker interface

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    flags::DataType,
    prelude::*,
    types::Zval,
};

/// Marker interface for all PHP promotion configuration objects.
#[php_interface]
#[php(name = "Lattice\\Promotion\\PromotionInterface")]
pub trait Promotion {}

#[derive(Debug)]
pub struct PromotionRef(Zval);

impl PromotionRef {
    pub(crate) fn as_zval(&self) -> &Zval {
        &self.0
    }
}

impl<'a> FromZval<'a> for PromotionRef {
    const TYPE: DataType =
        DataType::Object(Some(<PhpInterfacePromotion as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;
        let metadata = <PhpInterfacePromotion as RegisteredClass>::get_metadata();

        if !metadata.has_ce() || !obj.instance_of(metadata.ce()) {
            return None;
        }

        Some(Self(zval.shallow_clone()))
    }
}

impl Clone for PromotionRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for PromotionRef {
    const TYPE: DataType =
        DataType::Object(Some(<PhpInterfacePromotion as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}
