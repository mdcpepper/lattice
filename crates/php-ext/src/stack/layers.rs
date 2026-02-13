//! Promotion Stack Layers

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    exception::PhpException,
    flags::DataType,
    prelude::*,
    types::Zval,
};

use lattice::graph::OutputMode as CoreOutputMode;

use crate::{
    promotions::direct_discount::DirectDiscountPromotionRef, reference_value::ReferenceValue,
    stack::InvalidStackException,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[php_enum]
#[php(name = "Lattice\\LayerOutput")]
pub enum LayerOutput {
    #[php(value = "pass_through")]
    PassThrough,

    #[php(value = "split")]
    Split,
}

#[php_impl]
impl LayerOutput {
    pub fn pass_through() -> Self {
        Self::PassThrough
    }

    pub fn split() -> Self {
        Self::Split
    }
}

impl From<LayerOutput> for CoreOutputMode {
    fn from(value: LayerOutput) -> Self {
        match value {
            LayerOutput::PassThrough => Self::PassThrough,
            LayerOutput::Split => Self::Split,
        }
    }
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Layer")]
pub struct Layer {
    #[php(prop)]
    reference: ReferenceValue,

    #[php(prop)]
    output: LayerOutput,

    #[php(prop)]
    promotions: Vec<DirectDiscountPromotionRef>,
}

#[php_impl]
impl Layer {
    pub fn __construct(
        reference: ReferenceValue,
        output: LayerOutput,
        promotions: Vec<DirectDiscountPromotionRef>,
    ) -> Self {
        Self {
            reference,
            output,
            promotions,
        }
    }
}

impl Layer {
    pub(crate) fn output(&self) -> LayerOutput {
        self.output
    }

    pub(crate) fn promotions(&self) -> &[DirectDiscountPromotionRef] {
        &self.promotions
    }
}

#[derive(Debug)]
pub struct LayerRef(Zval);

impl LayerRef {
    pub fn from_layer(layer: Layer) -> Self {
        let mut zv = Zval::new();

        layer
            .set_zval(&mut zv, false)
            .expect("layer should always convert to object zval");

        Self(zv)
    }

    pub(crate) fn is_identical(&self, other: &Self) -> bool {
        self.0.is_identical(&other.0)
    }
}

impl<'a> FromZval<'a> for LayerRef {
    const TYPE: DataType = DataType::Object(Some(<Layer as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<Layer>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for LayerRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for LayerRef {
    const TYPE: DataType = DataType::Object(Some(<Layer as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&LayerRef> for Layer {
    type Error = PhpException;

    fn try_from(value: &LayerRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::from_class::<InvalidStackException>(
                "Layer object is invalid.".to_string(),
            ));
        };

        let reference = obj
            .get_property::<ReferenceValue>("reference")
            .map_err(|_| {
                PhpException::from_class::<InvalidStackException>(
                    "Layer reference property is invalid.".to_string(),
                )
            })?;

        let output = obj.get_property::<LayerOutput>("output").map_err(|_| {
            PhpException::from_class::<InvalidStackException>(
                "Layer output property is invalid.".to_string(),
            )
        })?;

        let promotions = obj
            .get_property::<Vec<DirectDiscountPromotionRef>>("promotions")
            .map_err(|_| {
                PhpException::from_class::<InvalidStackException>(
                    "Layer promotions property is invalid.".to_string(),
                )
            })?;

        Ok(Self {
            reference,
            output,
            promotions,
        })
    }
}

impl TryFrom<LayerRef> for Layer {
    type Error = PhpException;

    fn try_from(value: LayerRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}
