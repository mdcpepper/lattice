//! Tiered Threshold Promotions

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    flags::DataType,
    prelude::*,
    types::Zval,
};

use lattice::{
    prelude::PromotionKey,
    promotions::types::{
        ThresholdDiscount as CoreThresholdDiscount, ThresholdTier as CoreThresholdTier,
        TierThreshold as CoreTierThreshold,
        TieredThresholdPromotion as CoreTieredThresholdPromotion,
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
#[php(name = "Lattice\\Promotion\\TieredThreshold\\DiscountKind")]
pub enum DiscountKind {
    #[php(value = "percentage_off_each_item")]
    PercentageOffEachItem,

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
#[php(name = "Lattice\\Promotion\\TieredThreshold\\Discount")]
pub struct ThresholdDiscount {
    #[php(prop)]
    kind: DiscountKind,

    #[php(prop)]
    percentage: Option<PercentageRef>,

    #[php(prop)]
    amount: Option<MoneyRef>,
}

#[php_impl]
impl ThresholdDiscount {
    pub fn percentage_off_each_item(percentage: PercentageRef) -> Self {
        Self {
            kind: DiscountKind::PercentageOffEachItem,
            percentage: Some(percentage),
            amount: None,
        }
    }

    pub fn amount_off_each_item(amount: MoneyRef) -> Self {
        Self {
            kind: DiscountKind::AmountOffEachItem,
            percentage: None,
            amount: Some(amount),
        }
    }

    pub fn override_each_item(amount: MoneyRef) -> Self {
        Self {
            kind: DiscountKind::OverrideEachItem,
            percentage: None,
            amount: Some(amount),
        }
    }

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
pub struct ThresholdDiscountRef(Zval);

impl ThresholdDiscountRef {
    pub fn from_discount(discount: ThresholdDiscount) -> Self {
        let mut zv = Zval::new();

        discount
            .set_zval(&mut zv, false)
            .expect("threshold discount should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for ThresholdDiscountRef {
    const TYPE: DataType =
        DataType::Object(Some(<ThresholdDiscount as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<ThresholdDiscount>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for ThresholdDiscountRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for ThresholdDiscountRef {
    const TYPE: DataType =
        DataType::Object(Some(<ThresholdDiscount as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&ThresholdDiscountRef> for ThresholdDiscount {
    type Error = PhpException;

    fn try_from(value: &ThresholdDiscountRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::from_class::<InvalidDiscountException>(
                "ThresholdDiscount object is invalid".to_string(),
            ));
        };

        let kind = obj.get_property::<DiscountKind>("kind").map_err(|_| {
            PhpException::from_class::<InvalidDiscountException>(
                "ThresholdDiscount kind is invalid".to_string(),
            )
        })?;

        let percentage = obj
            .get_property::<Option<PercentageRef>>("percentage")
            .map_err(|_| {
                PhpException::from_class::<InvalidDiscountException>(
                    "ThresholdDiscount percentage is invalid".to_string(),
                )
            })?;

        let amount = obj
            .get_property::<Option<MoneyRef>>("amount")
            .map_err(|_| {
                PhpException::from_class::<InvalidDiscountException>(
                    "ThresholdDiscount amount is invalid".to_string(),
                )
            })?;

        Ok(ThresholdDiscount {
            kind,
            percentage,
            amount,
        })
    }
}

impl TryFrom<ThresholdDiscountRef> for ThresholdDiscount {
    type Error = PhpException;

    fn try_from(value: ThresholdDiscountRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&ThresholdDiscountRef> for CoreThresholdDiscount<'static> {
    type Error = PhpException;

    fn try_from(value: &ThresholdDiscountRef) -> Result<Self, Self::Error> {
        let discount: ThresholdDiscount = value.try_into()?;

        discount.try_into()
    }
}

impl TryFrom<ThresholdDiscountRef> for CoreThresholdDiscount<'static> {
    type Error = PhpException;

    fn try_from(value: ThresholdDiscountRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<ThresholdDiscount> for CoreThresholdDiscount<'static> {
    type Error = PhpException;

    fn try_from(discount: ThresholdDiscount) -> Result<Self, Self::Error> {
        match discount.kind {
            DiscountKind::PercentageOffEachItem => {
                let Some(percentage) = discount.percentage else {
                    return Err(PhpException::from_class::<InvalidDiscountException>(
                        "PercentageOffEachItem discount requires a percentage value".to_string(),
                    ));
                };

                Ok(CoreThresholdDiscount::PercentEachItem(
                    percentage.try_into()?,
                ))
            }
            DiscountKind::AmountOffEachItem => Ok(CoreThresholdDiscount::AmountOffEachItem(
                require_money(discount.amount, "AmountOffEachItem")?,
            )),
            DiscountKind::OverrideEachItem => Ok(CoreThresholdDiscount::FixedPriceEachItem(
                require_money(discount.amount, "OverrideEachItem")?,
            )),
            DiscountKind::AmountOffTotal => Ok(CoreThresholdDiscount::AmountOffTotal(
                require_money(discount.amount, "AmountOffTotal")?,
            )),
            DiscountKind::OverrideTotal => Ok(CoreThresholdDiscount::FixedTotal(require_money(
                discount.amount,
                "OverrideTotal",
            )?)),
            DiscountKind::PercentageOffCheapest => {
                let Some(percentage) = discount.percentage else {
                    return Err(PhpException::from_class::<InvalidDiscountException>(
                        "PercentageOffCheapest discount requires a percentage value".to_string(),
                    ));
                };

                Ok(CoreThresholdDiscount::PercentCheapest(
                    percentage.try_into()?,
                ))
            }
            DiscountKind::OverrideCheapest => Ok(CoreThresholdDiscount::FixedCheapest(
                require_money(discount.amount, "OverrideCheapest")?,
            )),
        }
    }
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Promotion\\TieredThreshold\\Threshold")]
pub struct TierThreshold {
    #[php(prop)]
    monetary_threshold: Option<MoneyRef>,

    #[php(prop)]
    item_count_threshold: Option<u32>,
}

#[php_impl]
impl TierThreshold {
    pub fn __construct(
        monetary_threshold: Option<MoneyRef>,
        item_count_threshold: Option<u32>,
    ) -> Self {
        Self {
            monetary_threshold,
            item_count_threshold,
        }
    }

    pub fn with_monetary_threshold(monetary_threshold: MoneyRef) -> Self {
        Self {
            monetary_threshold: Some(monetary_threshold),
            item_count_threshold: None,
        }
    }

    pub fn with_item_count_threshold(item_count_threshold: u32) -> Self {
        Self {
            monetary_threshold: None,
            item_count_threshold: Some(item_count_threshold),
        }
    }

    pub fn with_both_thresholds(monetary_threshold: MoneyRef, item_count_threshold: u32) -> Self {
        Self {
            monetary_threshold: Some(monetary_threshold),
            item_count_threshold: Some(item_count_threshold),
        }
    }
}

#[derive(Debug)]
pub struct TierThresholdRef(Zval);

impl TierThresholdRef {
    pub fn from_threshold(threshold: TierThreshold) -> Self {
        let mut zv = Zval::new();

        threshold
            .set_zval(&mut zv, false)
            .expect("tier threshold should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for TierThresholdRef {
    const TYPE: DataType = DataType::Object(Some(<TierThreshold as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<TierThreshold>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for TierThresholdRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for TierThresholdRef {
    const NULLABLE: bool = false;
    const TYPE: DataType = DataType::Object(Some(<TierThreshold as RegisteredClass>::CLASS_NAME));

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&TierThresholdRef> for TierThreshold {
    type Error = PhpException;

    fn try_from(value: &TierThresholdRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "tier threshold object is invalid".to_string(),
            ));
        };

        let monetary_threshold = obj
            .get_property::<Option<MoneyRef>>("monetaryThreshold")
            .map_err(|_| {
                PhpException::default(
                    "tier threshold monetary_threshold property is invalid".to_string(),
                )
            })?;

        let item_count_threshold = obj
            .get_property::<Option<u32>>("itemCountThreshold")
            .map_err(|_| {
                PhpException::default(
                    "tier threshold item_count_threshold property is invalid".to_string(),
                )
            })?;

        Ok(TierThreshold {
            monetary_threshold,
            item_count_threshold,
        })
    }
}

impl TryFrom<TierThresholdRef> for TierThreshold {
    type Error = PhpException;

    fn try_from(value: TierThresholdRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&TierThresholdRef> for CoreTierThreshold<'static> {
    type Error = PhpException;

    fn try_from(value: &TierThresholdRef) -> Result<Self, Self::Error> {
        let threshold: TierThreshold = value.try_into()?;

        threshold.try_into()
    }
}

impl TryFrom<TierThresholdRef> for CoreTierThreshold<'static> {
    type Error = PhpException;

    fn try_from(value: TierThresholdRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<TierThreshold> for CoreTierThreshold<'static> {
    type Error = PhpException;

    fn try_from(threshold: TierThreshold) -> Result<Self, Self::Error> {
        let monetary_threshold = threshold
            .monetary_threshold
            .map(|amount| {
                amount.try_into().map_err(|e| {
                    PhpException::default(format!("Invalid tier threshold monetary amount: {}", e))
                })
            })
            .transpose()?;

        Ok(CoreTierThreshold::new(
            monetary_threshold,
            threshold.item_count_threshold,
        ))
    }
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Promotion\\TieredThreshold\\Tier")]
pub struct ThresholdTier {
    #[php(prop)]
    lower_threshold: TierThresholdRef,

    #[php(prop)]
    upper_threshold: Option<TierThresholdRef>,

    #[php(prop)]
    contribution_qualification: QualificationRef,

    #[php(prop)]
    discount_qualification: QualificationRef,

    #[php(prop)]
    discount: ThresholdDiscountRef,
}

#[php_impl]
impl ThresholdTier {
    pub fn __construct(
        lower_threshold: TierThresholdRef,
        upper_threshold: Option<TierThresholdRef>,
        contribution_qualification: QualificationRef,
        discount_qualification: QualificationRef,
        discount: ThresholdDiscountRef,
    ) -> Self {
        Self {
            lower_threshold,
            upper_threshold,
            contribution_qualification,
            discount_qualification,
            discount,
        }
    }
}

impl ThresholdTier {
    pub(crate) fn try_to_core(&self) -> Result<CoreThresholdTier<'static>, PhpException> {
        let lower_threshold: CoreTierThreshold<'static> = (&self.lower_threshold).try_into()?;

        let upper_threshold: Option<CoreTierThreshold<'static>> = self
            .upper_threshold
            .as_ref()
            .map(TryInto::try_into)
            .transpose()?;

        Ok(CoreThresholdTier::new(
            lower_threshold,
            upper_threshold,
            (&self.contribution_qualification).try_into()?,
            (&self.discount_qualification).try_into()?,
            (&self.discount).try_into()?,
        ))
    }
}

#[derive(Debug)]
pub struct ThresholdTierRef(Zval);

impl ThresholdTierRef {
    pub fn from_tier(tier: ThresholdTier) -> Self {
        let mut zv = Zval::new();

        tier.set_zval(&mut zv, false)
            .expect("threshold tier should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for ThresholdTierRef {
    const TYPE: DataType = DataType::Object(Some(<ThresholdTier as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<ThresholdTier>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for ThresholdTierRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for ThresholdTierRef {
    const NULLABLE: bool = false;
    const TYPE: DataType = DataType::Object(Some(<ThresholdTier as RegisteredClass>::CLASS_NAME));

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&ThresholdTierRef> for ThresholdTier {
    type Error = PhpException;

    fn try_from(value: &ThresholdTierRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "threshold tier object is invalid".to_string(),
            ));
        };

        let lower_threshold = obj
            .get_property::<TierThresholdRef>("lowerThreshold")
            .map_err(|_| {
                PhpException::default(
                    "threshold tier lower_threshold property is invalid".to_string(),
                )
            })?;

        let upper_threshold = obj
            .get_property::<Option<TierThresholdRef>>("upperThreshold")
            .map_err(|_| {
                PhpException::default(
                    "threshold tier upper_threshold property is invalid".to_string(),
                )
            })?;

        let contribution_qualification = obj
            .get_property::<QualificationRef>("contributionQualification")
            .map_err(|_| {
                PhpException::default(
                    "threshold tier contribution_qualification property is invalid".to_string(),
                )
            })?;

        let discount_qualification = obj
            .get_property::<QualificationRef>("discountQualification")
            .map_err(|_| {
                PhpException::default(
                    "threshold tier discount_qualification property is invalid".to_string(),
                )
            })?;

        let discount = obj
            .get_property::<ThresholdDiscountRef>("discount")
            .map_err(|_| {
                PhpException::default("threshold tier discount property is invalid".to_string())
            })?;

        Ok(ThresholdTier {
            lower_threshold,
            upper_threshold,
            contribution_qualification,
            discount_qualification,
            discount,
        })
    }
}

impl TryFrom<ThresholdTierRef> for ThresholdTier {
    type Error = PhpException;

    fn try_from(value: ThresholdTierRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Promotion\\TieredThreshold\\TieredThreshold")]
#[php(implements(PhpInterfacePromotion))]
pub struct TieredThresholdPromotion {
    #[php(prop)]
    reference: ReferenceValue,

    #[php(prop)]
    tiers: Vec<ThresholdTierRef>,

    #[php(prop)]
    budget: BudgetRef,
}

#[php_impl]
impl TieredThresholdPromotion {
    pub fn __construct(
        reference: ReferenceValue,
        tiers: Vec<ThresholdTierRef>,
        budget: BudgetRef,
    ) -> Self {
        Self {
            reference,
            tiers,
            budget,
        }
    }
}

#[derive(Debug)]
pub struct TieredThresholdPromotionRef(Zval);

impl TieredThresholdPromotionRef {
    pub fn from_promotion(promotion: TieredThresholdPromotion) -> Self {
        let mut zv = Zval::new();

        promotion
            .set_zval(&mut zv, false)
            .expect("tiered threshold promotion should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for TieredThresholdPromotionRef {
    const TYPE: DataType = DataType::Object(Some(
        <TieredThresholdPromotion as RegisteredClass>::CLASS_NAME,
    ));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<TieredThresholdPromotion>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for TieredThresholdPromotionRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for TieredThresholdPromotionRef {
    const NULLABLE: bool = false;
    const TYPE: DataType = DataType::Object(Some(
        <TieredThresholdPromotion as RegisteredClass>::CLASS_NAME,
    ));

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&TieredThresholdPromotionRef> for TieredThresholdPromotion {
    type Error = PhpException;

    fn try_from(value: &TieredThresholdPromotionRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "tiered threshold promotion object is invalid".to_string(),
            ));
        };

        let reference = obj
            .get_property::<ReferenceValue>("reference")
            .map_err(|_| {
                PhpException::default(
                    "tiered threshold promotion reference property is invalid".to_string(),
                )
            })?;

        let tiers = obj
            .get_property::<Vec<ThresholdTierRef>>("tiers")
            .map_err(|_| {
                PhpException::default(
                    "tiered threshold promotion tiers property is invalid".to_string(),
                )
            })?;

        let budget = obj.get_property::<BudgetRef>("budget").map_err(|_| {
            PhpException::default(
                "tiered threshold promotion budget property is invalid".to_string(),
            )
        })?;

        Ok(TieredThresholdPromotion {
            reference,
            tiers,
            budget,
        })
    }
}

impl TryFrom<TieredThresholdPromotionRef> for TieredThresholdPromotion {
    type Error = PhpException;

    fn try_from(value: TieredThresholdPromotionRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TieredThresholdPromotion {
    pub(crate) fn try_to_core_with_key(
        &self,
        key: PromotionKey,
    ) -> Result<CoreTieredThresholdPromotion<'static>, PhpException> {
        let tiers = self
            .tiers
            .iter()
            .map(|tier| -> Result<CoreThresholdTier<'static>, PhpException> {
                let tier: ThresholdTier = tier.try_into()?;

                tier.try_to_core()
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CoreTieredThresholdPromotion::new(
            key,
            tiers,
            (&self.budget).try_into()?,
        ))
    }
}
