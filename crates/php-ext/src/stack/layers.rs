//! Promotion Stack Layers
#![allow(non_snake_case)]

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    exception::PhpException,
    flags::{DataType, PropertyFlags},
    prelude::*,
    types::Zval,
};

use lattice::graph::OutputMode as CoreOutputMode;

use crate::{
    promotions::interface::PromotionRef, reference_value::ReferenceValue,
    stack::InvalidStackException,
};

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Stack\\LayerOutput")]
pub struct LayerOutput {
    #[php(prop, flags = PropertyFlags::Private)]
    participating: Option<LayerRef>,

    #[php(prop, flags = PropertyFlags::Private)]
    non_participating: Option<LayerRef>,
}

#[allow(non_snake_case)]
#[php_impl]
impl LayerOutput {
    #[php(name = "passThrough")]
    pub fn pass_through() -> Self {
        Self {
            participating: None,
            non_participating: None,
        }
    }

    pub fn split(participating: LayerRef, nonParticipating: LayerRef) -> Self {
        Self {
            participating: Some(participating),
            non_participating: Some(nonParticipating),
        }
    }
}

impl LayerOutput {
    pub(crate) fn to_core_output_mode(&self) -> Result<CoreOutputMode, PhpException> {
        match (&self.participating, &self.non_participating) {
            (None, None) => Ok(CoreOutputMode::PassThrough),
            (Some(_), Some(_)) => Ok(CoreOutputMode::Split),
            _ => Err(PhpException::from_class::<InvalidStackException>(
                "Split layer output must include both participating and non-participating targets."
                    .to_string(),
            )),
        }
    }

    pub(crate) fn is_split(&self) -> bool {
        matches!(
            (&self.participating, &self.non_participating),
            (Some(_), Some(_))
        )
    }

    pub(crate) fn split_targets(&self) -> Result<Option<(&LayerRef, &LayerRef)>, PhpException> {
        match (&self.participating, &self.non_participating) {
            (None, None) => Ok(None),
            (Some(participating), Some(non_participating)) => {
                Ok(Some((participating, non_participating)))
            }
            (None, Some(_)) => Err(PhpException::from_class::<InvalidStackException>(
                "Split layer output is missing the participating target.".to_string(),
            )),
            (Some(_), None) => Err(PhpException::from_class::<InvalidStackException>(
                "Split layer output is missing the non-participating target.".to_string(),
            )),
        }
    }
}

#[derive(Debug)]
pub struct LayerOutputRef(Zval);

impl LayerOutputRef {
    pub fn from_layer_output(output: LayerOutput) -> Self {
        let mut zv = Zval::new();

        output
            .set_zval(&mut zv, false)
            .expect("layer output should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for LayerOutputRef {
    const TYPE: DataType = DataType::Object(Some(<LayerOutput as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<LayerOutput>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for LayerOutputRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for LayerOutputRef {
    const TYPE: DataType = DataType::Object(Some(<LayerOutput as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&LayerOutputRef> for LayerOutput {
    type Error = PhpException;

    fn try_from(value: &LayerOutputRef) -> Result<Self, Self::Error> {
        let Some(output) = <&LayerOutput>::from_zval(&value.0) else {
            return Err(PhpException::from_class::<InvalidStackException>(
                "Layer output object is invalid.".to_string(),
            ));
        };

        Ok(output.clone())
    }
}

impl TryFrom<LayerOutputRef> for LayerOutput {
    type Error = PhpException;

    fn try_from(value: LayerOutputRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Stack\\Layer")]
pub struct Layer {
    #[php(prop)]
    reference: ReferenceValue,

    #[php(prop)]
    output: LayerOutputRef,

    #[php(prop)]
    promotions: Vec<PromotionRef>,
}

#[php_impl]
impl Layer {
    pub fn __construct(
        reference: ReferenceValue,
        output: LayerOutputRef,
        promotions: Option<Vec<PromotionRef>>,
    ) -> Self {
        Self {
            reference,
            output,
            promotions: promotions.unwrap_or_default(),
        }
    }
}

impl Layer {
    pub(crate) fn output(&self) -> &LayerOutputRef {
        &self.output
    }

    pub(crate) fn promotions(&self) -> &[PromotionRef] {
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

        let output = obj.get_property::<LayerOutputRef>("output").map_err(|_| {
            PhpException::from_class::<InvalidStackException>(
                "Layer output property is invalid.".to_string(),
            )
        })?;

        let promotions = obj
            .get_property::<Vec<PromotionRef>>("promotions")
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
