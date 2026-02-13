//! Budgets

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    exception::PhpException,
    flags::DataType,
    prelude::*,
    types::Zval,
};

use lattice::promotions::budget::PromotionBudget;

use crate::money::{Money, MoneyRef};

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Promotions\\Budget")]
pub struct Budget {
    #[php(prop)]
    pub application_limit: Option<i64>,

    #[php(prop)]
    pub monetary_limit: Option<MoneyRef>,
}

#[php_impl]
impl Budget {
    pub fn unlimited() -> Self {
        Self {
            application_limit: None,
            monetary_limit: None,
        }
    }

    pub fn with_application_limit(limit: i64) -> Self {
        Self {
            application_limit: Some(limit),
            monetary_limit: None,
        }
    }

    pub fn with_monetary_limit(limit: MoneyRef) -> Self {
        Self {
            application_limit: None,
            monetary_limit: Some(limit),
        }
    }

    pub fn with_both_limits(application: i64, monetary: MoneyRef) -> Self {
        Self {
            application_limit: Some(application),
            monetary_limit: Some(monetary),
        }
    }
}

#[derive(Debug)]
pub struct BudgetRef(Zval);

impl BudgetRef {
    pub fn from_budget(budget: Budget) -> Self {
        let mut zv = Zval::new();

        budget
            .set_zval(&mut zv, false)
            .expect("budget should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for BudgetRef {
    const TYPE: DataType = DataType::Object(Some(<Budget as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<Budget>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for BudgetRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for BudgetRef {
    const TYPE: DataType = DataType::Object(Some(<Budget as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&BudgetRef> for Budget {
    type Error = PhpException;

    fn try_from(value: &BudgetRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "Budget object is invalid.".to_string(),
            ));
        };

        let application_limit = obj
            .get_property::<Option<i64>>("applicationLimit")
            .map_err(|_| {
                PhpException::default("Budget application_limit is invalid.".to_string())
            })?;

        let monetary_limit = obj
            .get_property::<Option<MoneyRef>>("monetaryLimit")
            .map_err(|_| PhpException::default("Budget monetary_limit is invalid.".to_string()))?;

        Ok(Budget {
            application_limit,
            monetary_limit,
        })
    }
}

impl TryFrom<BudgetRef> for Budget {
    type Error = PhpException;

    fn try_from(value: BudgetRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&BudgetRef> for PromotionBudget<'static> {
    type Error = PhpException;

    fn try_from(value: &BudgetRef) -> Result<Self, Self::Error> {
        let budget: Budget = value.try_into()?;

        budget.try_into()
    }
}

impl TryFrom<BudgetRef> for PromotionBudget<'static> {
    type Error = PhpException;

    fn try_from(value: BudgetRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<Budget> for PromotionBudget<'static> {
    type Error = PhpException;

    fn try_from(budget: Budget) -> Result<Self, Self::Error> {
        let application_limit = budget
            .application_limit
            .map(|limit| {
                u32::try_from(limit).map_err(|_| {
                    PhpException::default(
                        "Budget application_limit must be a non-negative 32-bit integer."
                            .to_string(),
                    )
                })
            })
            .transpose()?;

        let monetary_limit = budget
            .monetary_limit
            .map(|limit| {
                limit.try_into().map_err(|e| {
                    PhpException::default(format!("Invalid budget monetary_limit: {}", e))
                })
            })
            .transpose()?;

        Ok(PromotionBudget {
            application_limit,
            monetary_limit,
        })
    }
}

impl From<PromotionBudget<'static>> for Budget {
    fn from(budget: PromotionBudget<'static>) -> Self {
        let monetary_limit = budget.monetary_limit.map(|limit| {
            let money = Money::__construct(
                limit.to_minor_units(),
                limit.currency().iso_alpha_code.to_string(),
            )
            .expect("core budget should always contain a valid ISO currency");

            MoneyRef::from_money(money)
        });

        Self {
            application_limit: budget.application_limit.map(i64::from),
            monetary_limit,
        }
    }
}
