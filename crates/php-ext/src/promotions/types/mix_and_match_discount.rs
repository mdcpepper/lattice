//! Mix and Match Discounts

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    flags::DataType,
    prelude::*,
    types::Zval,
};
use slotmap::SlotMap;

use lattice::{
    prelude::{PromotionKey, PromotionSlotKey},
    promotions::types::{
        MixAndMatchDiscount as CoreMixAndMatchDiscount,
        MixAndMatchPromotion as CoreMixAndMatchPromotion, MixAndMatchSlot as CoreMixAndMatchSlot,
    },
};

use crate::{
    discounts::{InvalidDiscountException, percentages::PercentageRef, require_money},
    money::MoneyRef,
    promotions::{budgets::BudgetRef, interface::PhpInterfacePromotion},
    qualification::QualificationRef,
    reference_value::ReferenceValue,
};

#[derive(Debug, Clone, Copy)]
#[php_enum]
#[php(name = "Lattice\\Promotion\\MixAndMatch\\DiscountKind")]
pub enum DiscountKind {
    #[php(value = "percentage_off_all_items")]
    PercentageOffAllItems,

    #[php(value = "amount_off_each_item")]
    AmountOffEachItem,

    #[php(value = "override_each_item")]
    OverrideEachItem,

    #[php(value = "amount_off_total")]
    AmountOffTotal,

    #[php(value = "override_total")]
    OverrideTotal,

    #[php(value = "percentage_off_cheapest")]
    PercentageOffCheapest,

    #[php(value = "override_cheapest")]
    OverrideCheapest,
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Promotion\\MixAndMatch\\Discount")]
pub struct MixAndMatchDiscount {
    #[php(prop)]
    kind: DiscountKind,

    #[php(prop)]
    percentage: Option<PercentageRef>,

    #[php(prop)]
    amount: Option<MoneyRef>,
}

#[php_impl]
impl MixAndMatchDiscount {
    /// Create a percentage-based discount off all items (eg. "25% off")
    pub fn percentage_off_all_items(percentage: PercentageRef) -> Self {
        Self {
            kind: DiscountKind::PercentageOffAllItems,
            percentage: Some(percentage),
            amount: None,
        }
    }

    /// Create an amount override discount off each item (eg. "£5")
    pub fn amount_off_each_item(amount: MoneyRef) -> Self {
        Self {
            kind: DiscountKind::AmountOffEachItem,
            percentage: None,
            amount: Some(amount),
        }
    }

    /// Create an amount override discount for each item (eg. "£5 each")
    pub fn override_each_item(amount: MoneyRef) -> Self {
        Self {
            kind: DiscountKind::OverrideEachItem,
            percentage: None,
            amount: Some(amount),
        }
    }

    /// Create an amount off discount for all items (eg. "£2 off")
    pub fn amount_off_total(amount: MoneyRef) -> Self {
        Self {
            kind: DiscountKind::AmountOffTotal,
            percentage: None,
            amount: Some(amount),
        }
    }

    pub fn override_total(amount: MoneyRef) -> Self {
        Self {
            kind: DiscountKind::OverrideTotal,
            percentage: None,
            amount: Some(amount),
        }
    }

    pub fn percentage_off_cheapest(percentage: PercentageRef) -> Self {
        Self {
            kind: DiscountKind::PercentageOffCheapest,
            percentage: Some(percentage),
            amount: None,
        }
    }

    pub fn override_cheapest(amount: MoneyRef) -> Self {
        Self {
            kind: DiscountKind::OverrideCheapest,
            percentage: None,
            amount: Some(amount),
        }
    }
}

#[derive(Debug)]
pub struct MixAndMatchDiscountRef(Zval);

impl MixAndMatchDiscountRef {
    pub fn from_discount(discount: MixAndMatchDiscount) -> Self {
        let mut zv = Zval::new();

        discount
            .set_zval(&mut zv, false)
            .expect("discount should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for MixAndMatchDiscountRef {
    const TYPE: DataType =
        DataType::Object(Some(<MixAndMatchDiscount as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<MixAndMatchDiscount>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for MixAndMatchDiscountRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for MixAndMatchDiscountRef {
    const TYPE: DataType =
        DataType::Object(Some(<MixAndMatchDiscount as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&MixAndMatchDiscountRef> for MixAndMatchDiscount {
    type Error = PhpException;

    fn try_from(value: &MixAndMatchDiscountRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::from_class::<InvalidDiscountException>(
                "MixAndMatchDiscount object is invalid".to_string(),
            ));
        };

        let kind = obj.get_property::<DiscountKind>("kind").map_err(|_| {
            PhpException::from_class::<InvalidDiscountException>(
                "MixAndMatchDiscount kind is invalid".to_string(),
            )
        })?;

        let percentage = obj
            .get_property::<Option<PercentageRef>>("percentage")
            .map_err(|_| {
                PhpException::from_class::<InvalidDiscountException>(
                    "MixAndMatchDiscount percentage is invalid".to_string(),
                )
            })?;

        let amount = obj
            .get_property::<Option<MoneyRef>>("amount")
            .map_err(|_| {
                PhpException::from_class::<InvalidDiscountException>(
                    "MixAndMatchDiscount amount is invalid".to_string(),
                )
            })?;

        Ok(MixAndMatchDiscount {
            kind,
            percentage,
            amount,
        })
    }
}

impl TryFrom<MixAndMatchDiscountRef> for MixAndMatchDiscount {
    type Error = PhpException;

    fn try_from(value: MixAndMatchDiscountRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&MixAndMatchDiscountRef> for CoreMixAndMatchDiscount<'static> {
    type Error = PhpException;

    fn try_from(value: &MixAndMatchDiscountRef) -> Result<Self, Self::Error> {
        let discount: MixAndMatchDiscount = value.try_into()?;

        discount.try_into()
    }
}

impl TryFrom<MixAndMatchDiscountRef> for CoreMixAndMatchDiscount<'static> {
    type Error = PhpException;

    fn try_from(value: MixAndMatchDiscountRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<MixAndMatchDiscount> for CoreMixAndMatchDiscount<'static> {
    type Error = PhpException;

    fn try_from(discount: MixAndMatchDiscount) -> Result<Self, Self::Error> {
        match discount.kind {
            DiscountKind::PercentageOffAllItems => {
                let Some(percentage) = discount.percentage else {
                    return Err(PhpException::from_class::<InvalidDiscountException>(
                        "PercentageOff discount requires a percentage value".to_string(),
                    ));
                };

                Ok(CoreMixAndMatchDiscount::PercentAllItems(
                    percentage.try_into()?,
                ))
            }
            DiscountKind::AmountOffEachItem => Ok(CoreMixAndMatchDiscount::AmountOffEachItem(
                require_money(discount.amount, "AmountOffEachItem")?,
            )),
            DiscountKind::OverrideEachItem => Ok(CoreMixAndMatchDiscount::FixedPriceEachItem(
                require_money(discount.amount, "OverrideEachItem")?,
            )),
            DiscountKind::AmountOffTotal => Ok(CoreMixAndMatchDiscount::AmountOffTotal(
                require_money(discount.amount, "AmountOffTotal")?,
            )),
            DiscountKind::OverrideTotal => Ok(CoreMixAndMatchDiscount::FixedTotal(require_money(
                discount.amount,
                "OverrideTotal",
            )?)),
            DiscountKind::PercentageOffCheapest => {
                let Some(percentage) = discount.percentage else {
                    return Err(PhpException::from_class::<InvalidDiscountException>(
                        "PercentageOff discount requires a percentage value".to_string(),
                    ));
                };

                Ok(CoreMixAndMatchDiscount::PercentCheapest(
                    percentage.try_into()?,
                ))
            }
            DiscountKind::OverrideCheapest => Ok(CoreMixAndMatchDiscount::FixedCheapest(
                require_money(discount.amount, "OverrideCheapest")?,
            )),
        }
    }
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Promotion\\MixAndMatch\\Slot")]
pub struct MixAndMatchSlot {
    #[php(prop)]
    reference: ReferenceValue,

    #[php(prop)]
    qualification: QualificationRef,

    #[php(prop)]
    min: usize,

    #[php(prop)]
    max: Option<usize>,
}

#[php_impl]
impl MixAndMatchSlot {
    pub fn __construct(
        reference: ReferenceValue,
        qualification: QualificationRef,
        min: usize,
        max: Option<usize>,
    ) -> Self {
        Self {
            reference,
            qualification,
            min,
            max,
        }
    }
}

impl MixAndMatchSlot {
    pub(crate) fn try_to_core_with_key(
        &self,
        key: PromotionSlotKey,
    ) -> Result<CoreMixAndMatchSlot, PhpException> {
        Ok(CoreMixAndMatchSlot::new(
            key,
            (&self.qualification).try_into()?,
            self.min,
            self.max,
        ))
    }
}

#[derive(Debug)]
pub struct MixAndMatchSlotRef(Zval);

impl MixAndMatchSlotRef {
    pub fn from_slot(slot: MixAndMatchSlot) -> Self {
        let mut zv = Zval::new();

        slot.set_zval(&mut zv, false)
            .expect("mix and match discount slot should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for MixAndMatchSlotRef {
    const TYPE: DataType = DataType::Object(Some(<MixAndMatchSlot as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<MixAndMatchSlot>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for MixAndMatchSlotRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for MixAndMatchSlotRef {
    const NULLABLE: bool = false;
    const TYPE: DataType = DataType::Object(Some(<MixAndMatchSlot as RegisteredClass>::CLASS_NAME));

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&MixAndMatchSlotRef> for MixAndMatchSlot {
    type Error = PhpException;

    fn try_from(value: &MixAndMatchSlotRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "mix and match promotion slot object is invalid".to_string(),
            ));
        };

        let reference = obj
            .get_property::<ReferenceValue>("reference")
            .map_err(|_| {
                PhpException::default(
                    "mix and match discount reference property is invalid".to_string(),
                )
            })?;

        let min = obj.get_property::<usize>("min").map_err(|_| {
            PhpException::default("mix and match discount min property is invalid".to_string())
        })?;

        let max = obj.get_property::<Option<usize>>("max").map_err(|_| {
            PhpException::default("mix and match discount max property is invalid".to_string())
        })?;

        let qualification = obj
            .get_property::<QualificationRef>("qualification")
            .map_err(|_| {
                PhpException::default(
                    "mix and match discount qualification property is invalid.".to_string(),
                )
            })?;

        Ok(MixAndMatchSlot {
            reference,
            qualification,
            min,
            max,
        })
    }
}

impl TryFrom<MixAndMatchSlotRef> for MixAndMatchSlot {
    type Error = PhpException;

    fn try_from(value: MixAndMatchSlotRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Promotion\\MixAndMatch\\MixAndMatch")]
#[php(implements(PhpInterfacePromotion))]
pub struct MixAndMatchDiscountPromotion {
    #[php(prop)]
    reference: ReferenceValue,

    #[php(prop)]
    slots: Vec<MixAndMatchSlotRef>,

    #[php(prop)]
    discount: MixAndMatchDiscountRef,

    #[php(prop)]
    budget: BudgetRef,
}

#[php_impl]
impl MixAndMatchDiscountPromotion {
    pub fn __construct(
        reference: ReferenceValue,
        slots: Vec<MixAndMatchSlotRef>,
        discount: MixAndMatchDiscountRef,
        budget: BudgetRef,
    ) -> Self {
        Self {
            reference,
            slots,
            discount,
            budget,
        }
    }
}

#[derive(Debug)]
pub struct MixAndMatchDiscountPromotionRef(Zval);

impl MixAndMatchDiscountPromotionRef {
    pub fn from_promotion(promotion: MixAndMatchDiscountPromotion) -> Self {
        let mut zv = Zval::new();

        promotion
            .set_zval(&mut zv, false)
            .expect("mix and match promotion should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for MixAndMatchDiscountPromotionRef {
    const TYPE: DataType = DataType::Object(Some(
        <MixAndMatchDiscountPromotion as RegisteredClass>::CLASS_NAME,
    ));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<MixAndMatchDiscountPromotion>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for MixAndMatchDiscountPromotionRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for MixAndMatchDiscountPromotionRef {
    const NULLABLE: bool = false;
    const TYPE: DataType = DataType::Object(Some(
        <MixAndMatchDiscountPromotion as RegisteredClass>::CLASS_NAME,
    ));

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&MixAndMatchDiscountPromotionRef> for MixAndMatchDiscountPromotion {
    type Error = PhpException;

    fn try_from(value: &MixAndMatchDiscountPromotionRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "mix and match promotion object is invalid".to_string(),
            ));
        };

        let reference = obj
            .get_property::<ReferenceValue>("reference")
            .map_err(|_| {
                PhpException::default(
                    "mix and match promotion reference property is invalid".to_string(),
                )
            })?;

        let slots = obj
            .get_property::<Vec<MixAndMatchSlotRef>>("slots")
            .map_err(|_| {
                PhpException::default(
                    "mix and match promotion slots property is invalid".to_string(),
                )
            })?;

        let discount = obj
            .get_property::<MixAndMatchDiscountRef>("discount")
            .map_err(|_| {
                PhpException::default(
                    "mix and match promotion discount property is invalid".to_string(),
                )
            })?;

        let budget = obj.get_property::<BudgetRef>("budget").map_err(|_| {
            PhpException::default("mix and match promotion budget property is invalid".to_string())
        })?;

        Ok(MixAndMatchDiscountPromotion {
            reference,
            slots,
            discount,
            budget,
        })
    }
}

impl TryFrom<MixAndMatchDiscountPromotionRef> for MixAndMatchDiscountPromotion {
    type Error = PhpException;

    fn try_from(value: MixAndMatchDiscountPromotionRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl MixAndMatchDiscountPromotion {
    pub(crate) fn try_to_core_with_key(
        &self,
        key: PromotionKey,
    ) -> Result<CoreMixAndMatchPromotion<'static>, PhpException> {
        let mut slot_keys = SlotMap::<PromotionSlotKey, ()>::with_key();

        let slots = self
            .slots
            .iter()
            .map(|slot| -> Result<CoreMixAndMatchSlot, PhpException> {
                let slot: MixAndMatchSlot = slot.try_into()?;

                slot.try_to_core_with_key(slot_keys.insert(()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CoreMixAndMatchPromotion::new(
            key,
            slots,
            (&self.discount).try_into()?,
            (&self.budget).try_into()?,
        ))
    }
}
