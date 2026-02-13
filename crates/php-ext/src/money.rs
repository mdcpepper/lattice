//! Money

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    flags::DataType,
    prelude::*,
    types::Zval,
};
use rusty_money::{Findable, Money as RustyMoney, MoneyError, iso::Currency};

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Money")]
pub struct Money {
    #[php(prop)]
    amount: i64,

    #[php(prop)]
    currency: String,
}

#[php_impl]
impl Money {
    pub fn __construct(amount: i64, currency: String) -> PhpResult<Self> {
        let Some(_) = Currency::find(&currency) else {
            return Err(PhpException::default("Invalid currency.".to_string()));
        };

        Ok(Self { amount, currency })
    }

    pub fn currency(&self) -> &str {
        &self.currency
    }
}

impl TryFrom<Money> for RustyMoney<'static, Currency> {
    type Error = MoneyError;

    fn try_from(money: Money) -> Result<Self, Self::Error> {
        let Some(currency) = Currency::find(money.currency()) else {
            return Err(MoneyError::InvalidCurrency);
        };

        Ok(Self::from_minor(money.amount, currency))
    }
}

#[derive(Debug)]
pub struct MoneyRef(Zval);

impl MoneyRef {
    pub fn from_money(money: Money) -> Self {
        let mut zv = Zval::new();

        money
            .set_zval(&mut zv, false)
            .expect("money should always convert to object zval");

        Self(zv)
    }

    pub fn amount(&self) -> i64 {
        self.0
            .object()
            .and_then(|obj| obj.get_property::<i64>("amount").ok())
            .unwrap_or_default()
    }

    pub fn currency(&self) -> String {
        self.0
            .object()
            .and_then(|obj| obj.get_property::<String>("currency").ok())
            .unwrap_or_default()
    }
}

impl<'a> FromZval<'a> for MoneyRef {
    const TYPE: DataType = DataType::Object(Some(<Money as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<Money>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for MoneyRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for MoneyRef {
    const TYPE: DataType = DataType::Object(Some(<Money as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&MoneyRef> for Money {
    type Error = PhpException;

    fn try_from(value: &MoneyRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "Money object is invalid.".to_string(),
            ));
        };

        let amount = obj
            .get_property::<i64>("amount")
            .map_err(|_| PhpException::default("Money amount is invalid.".to_string()))?;

        let currency = obj
            .get_property::<String>("currency")
            .map_err(|_| PhpException::default("Money currency is invalid.".to_string()))?;

        Money::__construct(amount, currency)
    }
}

impl TryFrom<MoneyRef> for Money {
    type Error = PhpException;

    fn try_from(value: MoneyRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<MoneyRef> for RustyMoney<'static, Currency> {
    type Error = MoneyError;

    fn try_from(money: MoneyRef) -> Result<Self, Self::Error> {
        let Some(currency) = Currency::find(&money.currency()) else {
            return Err(MoneyError::InvalidCurrency);
        };

        Ok(Self::from_minor(money.amount(), currency))
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use super::*;

    #[test]
    fn money_from_money_success() -> TestResult {
        let input = Money::__construct(123, "GBP".to_string())
            .expect("valid currency should construct money");

        let money: RustyMoney<'static, Currency> = input.clone().try_into()?;

        assert_eq!(input.amount, money.to_minor_units());
        assert_eq!(input.currency(), money.currency().iso_alpha_code);

        Ok(())
    }

    #[test]
    fn money_from_money_failure() -> TestResult {
        let input = Money::__construct(123, "XXX".to_string());

        assert!(input.is_err());

        Ok(())
    }
}
