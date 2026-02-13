//! Discounts

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    exception::PhpException,
    flags::DataType,
    prelude::*,
    types::Zval,
    zend::ce,
};
use lattice::discounts::SimpleDiscount as CoreSimpleDiscount;
use rusty_money::{Money as RustyMoney, iso::Currency};

use crate::{discounts::percentages::PercentageRef, money::MoneyRef};

pub mod percentages;

/// Exception thrown when a discount configuration is invalid
#[derive(Default)]
#[php_class]
#[php(
    name = "Lattice\\Discount\\InvalidDiscountException",
    extends(ce = ce::exception, stub = "\\Exception")
)]
pub struct InvalidDiscountException;

#[php_impl]
impl InvalidDiscountException {}

#[derive(Debug, Clone, Copy)]
#[php_enum]
#[php(name = "Lattice\\Discount\\DiscountKind")]
pub enum DiscountKind {
    #[php(value = "percentage_off")]
    PercentageOff,

    #[php(value = "amount_override")]
    AmountOverride,

    #[php(value = "amount_off")]
    AmountOff,
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Discount\\SimpleDiscount")]
pub struct SimpleDiscount {
    #[php(prop)]
    kind: DiscountKind,

    #[php(prop)]
    percentage: Option<PercentageRef>,

    #[php(prop)]
    amount: Option<MoneyRef>,
}

#[php_impl]
impl SimpleDiscount {
    /// Create a percentage-based discount (eg. "25% off")
    pub fn percentage_off(percentage: PercentageRef) -> Self {
        Self {
            kind: DiscountKind::PercentageOff,
            percentage: Some(percentage),
            amount: None,
        }
    }

    /// Create an amount override discount (eg. "£5 each")
    pub fn amount_override(amount: MoneyRef) -> Self {
        Self {
            kind: DiscountKind::AmountOverride,
            percentage: None,
            amount: Some(amount),
        }
    }

    /// Create an amount off discount (eg. "£2 off")
    pub fn amount_off(amount: MoneyRef) -> Self {
        Self {
            kind: DiscountKind::AmountOff,
            percentage: None,
            amount: Some(amount),
        }
    }
}

#[derive(Debug)]
pub struct SimpleDiscountRef(Zval);

impl SimpleDiscountRef {
    pub fn from_discount(discount: SimpleDiscount) -> Self {
        let mut zv = Zval::new();

        discount
            .set_zval(&mut zv, false)
            .expect("discount should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for SimpleDiscountRef {
    const TYPE: DataType = DataType::Object(Some(<SimpleDiscount as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<SimpleDiscount>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for SimpleDiscountRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for SimpleDiscountRef {
    const TYPE: DataType = DataType::Object(Some(<SimpleDiscount as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&SimpleDiscountRef> for SimpleDiscount {
    type Error = PhpException;

    fn try_from(value: &SimpleDiscountRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::from_class::<InvalidDiscountException>(
                "SimpleDiscount object is invalid".to_string(),
            ));
        };

        let kind = obj.get_property::<DiscountKind>("kind").map_err(|_| {
            PhpException::from_class::<InvalidDiscountException>(
                "SimpleDiscount kind is invalid".to_string(),
            )
        })?;

        let percentage = obj
            .get_property::<Option<PercentageRef>>("percentage")
            .map_err(|_| {
                PhpException::from_class::<InvalidDiscountException>(
                    "SimpleDiscount percentage is invalid".to_string(),
                )
            })?;

        let amount = obj
            .get_property::<Option<MoneyRef>>("amount")
            .map_err(|_| {
                PhpException::from_class::<InvalidDiscountException>(
                    "SimpleDiscount amount is invalid".to_string(),
                )
            })?;

        Ok(SimpleDiscount {
            kind,
            percentage,
            amount,
        })
    }
}

impl TryFrom<SimpleDiscountRef> for SimpleDiscount {
    type Error = PhpException;

    fn try_from(value: SimpleDiscountRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&SimpleDiscountRef> for CoreSimpleDiscount<'static> {
    type Error = PhpException;

    fn try_from(value: &SimpleDiscountRef) -> Result<Self, Self::Error> {
        let discount: SimpleDiscount = value.try_into()?;

        discount.try_into()
    }
}

impl TryFrom<SimpleDiscountRef> for CoreSimpleDiscount<'static> {
    type Error = PhpException;

    fn try_from(value: SimpleDiscountRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<SimpleDiscount> for CoreSimpleDiscount<'static> {
    type Error = PhpException;

    fn try_from(discount: SimpleDiscount) -> Result<Self, Self::Error> {
        match discount.kind {
            DiscountKind::PercentageOff => {
                let Some(percentage) = discount.percentage else {
                    return Err(PhpException::from_class::<InvalidDiscountException>(
                        "PercentageOff discount requires a percentage value".to_string(),
                    ));
                };

                Ok(CoreSimpleDiscount::PercentageOff(percentage.try_into()?))
            }
            DiscountKind::AmountOverride => Ok(CoreSimpleDiscount::AmountOverride(require_money(
                discount.amount,
                "AmountOverride",
            )?)),
            DiscountKind::AmountOff => Ok(CoreSimpleDiscount::AmountOff(require_money(
                discount.amount,
                "AmountOff",
            )?)),
        }
    }
}

fn require_money(
    amount: Option<MoneyRef>,
    kind: &str,
) -> Result<RustyMoney<'static, Currency>, PhpException> {
    let Some(amount) = amount else {
        return Err(PhpException::from_class::<InvalidDiscountException>(
            format!("{kind} discount requires a money amount"),
        ));
    };

    amount.try_into().map_err(|e| {
        PhpException::from_class::<InvalidDiscountException>(format!("Invalid money amount: {}", e))
    })
}
