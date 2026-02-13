//! Promotion Stack Layers

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    exception::PhpException,
    flags::DataType,
    prelude::*,
    types::Zval,
};

use lattice::{
    graph::OutputMode as CoreOutputMode,
    promotions::{Promotion, PromotionKey},
};

use crate::{
    promotions::direct_discount::{DirectDiscountPromotion, DirectDiscountPromotionRef},
    reference_value::ReferenceValue,
};

#[derive(Debug, Clone, Copy)]
#[php_enum]
#[php(name = "FeedCode\\Lattice\\LayerOutput")]
pub enum LayerOutput {
    #[php(value = "pass_through")]
    PassThrough,

    #[php(value = "split")]
    Split,
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
#[php(name = "FeedCode\\Lattice\\Layer")]
pub struct Layer {
    #[php(prop)]
    key: ReferenceValue,

    #[php(prop)]
    output: LayerOutput,

    #[php(prop)]
    promotions: Vec<LayerPromotionRef>,
}

#[php_impl]
impl Layer {
    pub fn __construct(
        key: ReferenceValue,
        output: LayerOutput,
        promotions: Option<Vec<LayerPromotionRef>>,
    ) -> Self {
        Self {
            key,
            output,
            promotions: promotions.unwrap_or_default(),
        }
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
            return Err(PhpException::default(
                "Layer object is invalid.".to_string(),
            ));
        };

        let key = obj
            .get_property::<ReferenceValue>("key")
            .map_err(|_| PhpException::default("Layer key property is invalid.".to_string()))?;

        let output = obj
            .get_property::<LayerOutput>("output")
            .map_err(|_| PhpException::default("Layer output property is invalid.".to_string()))?;

        let promotions = obj
            .get_property::<Vec<LayerPromotionRef>>("promotions")
            .map_err(|_| {
                PhpException::default("Layer promotions property is invalid.".to_string())
            })?;

        Ok(Self {
            key,
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

#[derive(Debug)]
pub struct LayerPromotionRef(Zval);

impl LayerPromotionRef {
    #[allow(dead_code)]
    pub(crate) fn try_to_core_with_key(
        &self,
        key: PromotionKey,
    ) -> Result<Promotion<'static>, PhpException> {
        let Some(obj) = self.0.object() else {
            return Err(PhpException::default(
                "Layer promotion object is invalid.".to_string(),
            ));
        };

        if obj.is_instance::<DirectDiscountPromotion>() {
            let direct_ref = <DirectDiscountPromotionRef as FromZval>::from_zval(&self.0)
                .ok_or_else(|| {
                    PhpException::default(
                        "DirectDiscount promotion could not be decoded from layer.".to_string(),
                    )
                })?;

            let direct: DirectDiscountPromotion = (&direct_ref).try_into()?;

            return Ok(lattice::promotions::promotion(
                direct.try_to_core_with_key(key)?,
            ));
        }

        Err(PhpException::default(
            "Unsupported promotion type in layer promotions.".to_string(),
        ))
    }
}

impl<'a> FromZval<'a> for LayerPromotionRef {
    const TYPE: DataType = DataType::Object(None);

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<DirectDiscountPromotion>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for LayerPromotionRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for LayerPromotionRef {
    const TYPE: DataType = DataType::Object(None);
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}
