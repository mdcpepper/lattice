//! Reference value

use ext_php_rs::{
    convert::{FromZval, IntoZval},
    flags::DataType,
    types::Zval,
};

#[derive(Debug)]
pub struct ReferenceValue(Zval);

impl Clone for ReferenceValue {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl<'a> FromZval<'a> for ReferenceValue {
    const TYPE: DataType = DataType::Mixed;

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        Some(Self(zval.shallow_clone()))
    }
}

impl IntoZval for ReferenceValue {
    const TYPE: DataType = DataType::Mixed;
    const NULLABLE: bool = true;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}
