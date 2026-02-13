//! Direct Discount Promotions

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    exception::PhpException,
    flags::DataType,
    prelude::*,
    types::Zval,
};

use lattice::{
    promotions::{PromotionKey, types::DirectDiscountPromotion as CoreDirectDiscountPromotion},
    tags::string::StringTagCollection,
};

use crate::{
    discounts::SimpleDiscountRef,
    promotions::{budgets::BudgetRef, interface::PhpInterfacePromotion},
    qualification::QualificationRef,
    reference_value::ReferenceValue,
};

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Promotions\\DirectDiscount")]
#[php(implements(PhpInterfacePromotion))]
pub struct DirectDiscountPromotion {
    #[php(prop)]
    reference: ReferenceValue,

    #[php(prop)]
    qualification: QualificationRef,

    #[php(prop)]
    discount: SimpleDiscountRef,

    #[php(prop)]
    budget: BudgetRef,
}

#[php_impl]
impl DirectDiscountPromotion {
    pub fn __construct(
        reference: ReferenceValue,
        qualification: QualificationRef,
        discount: SimpleDiscountRef,
        budget: BudgetRef,
    ) -> Self {
        Self {
            reference,
            qualification,
            discount,
            budget,
        }
    }
}

#[derive(Debug)]
pub struct DirectDiscountPromotionRef(Zval);

impl DirectDiscountPromotionRef {
    pub fn from_promotion(promotion: DirectDiscountPromotion) -> Self {
        let mut zv = Zval::new();

        promotion
            .set_zval(&mut zv, false)
            .expect("direct discount promotion should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for DirectDiscountPromotionRef {
    const TYPE: DataType = DataType::Object(Some(
        <DirectDiscountPromotion as RegisteredClass>::CLASS_NAME,
    ));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<DirectDiscountPromotion>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for DirectDiscountPromotionRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for DirectDiscountPromotionRef {
    const NULLABLE: bool = false;
    const TYPE: DataType = DataType::Object(Some(
        <DirectDiscountPromotion as RegisteredClass>::CLASS_NAME,
    ));

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&DirectDiscountPromotionRef> for DirectDiscountPromotion {
    type Error = PhpException;

    fn try_from(value: &DirectDiscountPromotionRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "DirectDiscount promotion object is invalid.".to_string(),
            ));
        };

        let reference = obj
            .get_property::<ReferenceValue>("reference")
            .map_err(|_| {
                PhpException::default("DirectDiscount reference property is invalid.".to_string())
            })?;

        let qualification = obj
            .get_property::<QualificationRef>("qualification")
            .map_err(|_| {
                PhpException::default(
                    "DirectDiscount qualification property is invalid.".to_string(),
                )
            })?;

        let discount = obj
            .get_property::<SimpleDiscountRef>("discount")
            .map_err(|_| {
                PhpException::default("DirectDiscount discount property is invalid.".to_string())
            })?;

        let budget = obj.get_property::<BudgetRef>("budget").map_err(|_| {
            PhpException::default("DirectDiscount budget property is invalid.".to_string())
        })?;

        Ok(DirectDiscountPromotion {
            reference,
            qualification,
            discount,
            budget,
        })
    }
}

impl TryFrom<DirectDiscountPromotionRef> for DirectDiscountPromotion {
    type Error = PhpException;

    fn try_from(value: DirectDiscountPromotionRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl DirectDiscountPromotion {
    pub(crate) fn try_to_core_with_reference(
        &self,
        reference: PromotionKey,
    ) -> Result<CoreDirectDiscountPromotion<'static, StringTagCollection>, PhpException> {
        Ok(CoreDirectDiscountPromotion::new(
            reference,
            (&self.qualification).try_into()?,
            (&self.discount).try_into()?,
            (&self.budget).try_into()?,
        ))
    }
}
