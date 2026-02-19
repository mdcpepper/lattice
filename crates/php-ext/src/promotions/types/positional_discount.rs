//! Positional Discount Promotions

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    flags::DataType,
    prelude::*,
    types::Zval,
};

use lattice::{
    prelude::{PromotionKey, StringTagCollection},
    promotions::types::PositionalDiscountPromotion as CorePositionalDiscountPromotion,
};

use crate::{
    discounts::SimpleDiscountRef,
    promotions::{budgets::BudgetRef, interface::PhpInterfacePromotion},
    qualification::QualificationRef,
    reference_value::ReferenceValue,
};

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Promotion\\Positional")]
#[php(implements(PhpInterfacePromotion))]
pub struct PositionalDiscountPromotion {
    #[php(prop)]
    reference: ReferenceValue,

    #[php(prop)]
    size: u16,

    #[php(prop)]
    positions: Vec<u16>,

    #[php(prop)]
    qualification: QualificationRef,

    #[php(prop)]
    discount: SimpleDiscountRef,

    #[php(prop)]
    budget: BudgetRef,
}

#[php_impl]
impl PositionalDiscountPromotion {
    pub fn __construct(
        reference: ReferenceValue,
        qualification: QualificationRef,
        size: u16,
        positions: Vec<u16>,
        discount: SimpleDiscountRef,
        budget: BudgetRef,
    ) -> Self {
        Self {
            reference,
            size,
            positions,
            qualification,
            discount,
            budget,
        }
    }
}

impl PositionalDiscountPromotion {
    pub(crate) fn try_to_core_with_key(
        &self,
        key: PromotionKey,
    ) -> Result<CorePositionalDiscountPromotion<'static, StringTagCollection>, PhpException> {
        Ok(CorePositionalDiscountPromotion::new(
            key,
            (&self.qualification).try_into()?,
            self.size,
            self.positions.clone().into(),
            (&self.discount).try_into()?,
            (&self.budget).try_into()?,
        ))
    }
}

#[derive(Debug)]
pub struct PositionalDiscountPromotionRef(Zval);

impl PositionalDiscountPromotionRef {
    pub fn from_promotion(promotion: PositionalDiscountPromotion) -> Self {
        let mut zv = Zval::new();

        promotion
            .set_zval(&mut zv, false)
            .expect("positional discount promotion should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for PositionalDiscountPromotionRef {
    const TYPE: DataType = DataType::Object(Some(
        <PositionalDiscountPromotion as RegisteredClass>::CLASS_NAME,
    ));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<PositionalDiscountPromotion>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for PositionalDiscountPromotionRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for PositionalDiscountPromotionRef {
    const NULLABLE: bool = false;
    const TYPE: DataType = DataType::Object(Some(
        <PositionalDiscountPromotion as RegisteredClass>::CLASS_NAME,
    ));

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&PositionalDiscountPromotionRef> for PositionalDiscountPromotion {
    type Error = PhpException;

    fn try_from(value: &PositionalDiscountPromotionRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "positional discount promotion object is invalid".to_string(),
            ));
        };

        let reference = obj
            .get_property::<ReferenceValue>("reference")
            .map_err(|_| {
                PhpException::default(
                    "positional discount reference property is invalid".to_string(),
                )
            })?;

        let size = obj.get_property::<u16>("size").map_err(|_| {
            PhpException::default("positional discount size property is invalid".to_string())
        })?;

        let positions = obj.get_property::<Vec<u16>>("positions").map_err(|_| {
            PhpException::default("positional discount positions property is invalid".to_string())
        })?;

        let qualification = obj
            .get_property::<QualificationRef>("qualification")
            .map_err(|_| {
                PhpException::default(
                    "positional discount qualification property is invalid.".to_string(),
                )
            })?;

        let discount = obj
            .get_property::<SimpleDiscountRef>("discount")
            .map_err(|_| {
                PhpException::default(
                    "positional discount discount property is invalid.".to_string(),
                )
            })?;

        let budget = obj.get_property::<BudgetRef>("budget").map_err(|_| {
            PhpException::default("positional discount budget property is invalid.".to_string())
        })?;

        Ok(PositionalDiscountPromotion {
            reference,
            size,
            positions,
            qualification,
            discount,
            budget,
        })
    }
}

impl TryFrom<PositionalDiscountPromotionRef> for PositionalDiscountPromotion {
    type Error = PhpException;

    fn try_from(value: PositionalDiscountPromotionRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}
