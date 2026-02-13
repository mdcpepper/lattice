//! Discount Percentages

use decimal_percentage::Percentage as CorePercentage;
use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    exception::PhpException,
    flags::DataType,
    prelude::*,
    types::Zval,
    zend::ce,
};

/// Exception thrown when a percentage value is invalid
#[derive(Default)]
#[php_class]
#[php(
    name = "Lattice\\Discount\\InvalidPercentageException",
    extends(ce = ce::exception, stub = "\\Exception")
)]
pub struct InvalidPercentageException;

#[php_impl]
impl InvalidPercentageException {}

/// Exception thrown when a percentage value is out of the valid range (0-100%)
#[derive(Default)]
#[php_class]
#[php(
    name = "Lattice\\Discount\\PercentageOutOfRangeException",
    extends(ce = ce::exception, stub = "\\Exception")
)]
pub struct PercentageOutOfRangeException;

#[php_impl]
impl PercentageOutOfRangeException {}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Discount\\Percentage")]
#[cfg_attr(php82, php(readonly))]
pub struct Percentage {
    #[php(prop)]
    value: f64,
}

#[php_impl]
impl Percentage {
    /// Construct a Percentage from a string representation (eg. "0.25", "25%")
    /// Normalizes to a decimal value between 0 and 1
    pub fn __construct(value: String) -> PhpResult<Self> {
        let trimmed = value.trim();

        let (parse_value, has_percent_suffix) = if let Some(stripped) = trimmed.strip_suffix('%') {
            (stripped.trim(), true)
        } else {
            (trimmed, false)
        };

        let mut normalized = parse_value.parse::<f64>().map_err(|_| {
            PhpException::from_class::<InvalidPercentageException>(format!(
                "Invalid percentage value: '{value}'"
            ))
        })?;

        if has_percent_suffix {
            normalized /= 100.0;
        }

        Self::from_normalized(normalized)
    }

    /// Create a Percentage from a decimal value between 0 and 1 (eg. 0.25 for 25%)
    pub fn from_decimal(value: f64) -> PhpResult<Self> {
        Self::from_normalized(value)
    }

    fn from_normalized(value: f64) -> PhpResult<Self> {
        Self::validate_normalized(value)?;

        Ok(Self { value })
    }

    fn validate_normalized(value: f64) -> PhpResult<()> {
        if !value.is_finite() {
            return Err(PhpException::from_class::<InvalidPercentageException>(
                "Percentage value must be finite".to_string(),
            ));
        }

        if value < 0.0 {
            return Err(PhpException::from_class::<PercentageOutOfRangeException>(
                "Discount percentage cannot be negative".to_string(),
            ));
        }

        if value > 1.0 {
            return Err(PhpException::from_class::<PercentageOutOfRangeException>(
                "Discount percentage cannot exceed 100%".to_string(),
            ));
        }

        Ok(())
    }

    /// Get the decimal value of the percentage (between 0 and 1)
    pub fn value(&self) -> f64 {
        self.value
    }
}

impl TryFrom<Percentage> for CorePercentage {
    type Error = PhpException;

    fn try_from(percentage: Percentage) -> Result<Self, Self::Error> {
        Percentage::validate_normalized(percentage.value)?;

        Ok(CorePercentage::from(percentage.value))
    }
}

#[derive(Debug)]
pub struct PercentageRef(Zval);

impl PercentageRef {
    pub fn from_percentage(percentage: Percentage) -> Self {
        let mut zv = Zval::new();

        percentage
            .set_zval(&mut zv, false)
            .expect("percentage should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for PercentageRef {
    const TYPE: DataType = DataType::Object(Some(<Percentage as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<Percentage>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for PercentageRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for PercentageRef {
    const TYPE: DataType = DataType::Object(Some(<Percentage as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&PercentageRef> for Percentage {
    type Error = PhpException;

    fn try_from(value: &PercentageRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::from_class::<InvalidPercentageException>(
                "Percentage object is invalid".to_string(),
            ));
        };

        let amount = obj.get_property::<f64>("value").map_err(|_| {
            PhpException::from_class::<InvalidPercentageException>(
                "Percentage value is invalid".to_string(),
            )
        })?;

        Percentage::from_decimal(amount)
    }
}

impl TryFrom<PercentageRef> for Percentage {
    type Error = PhpException;

    fn try_from(value: PercentageRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&PercentageRef> for CorePercentage {
    type Error = PhpException;

    fn try_from(value: &PercentageRef) -> Result<Self, Self::Error> {
        let percentage: Percentage = value.try_into()?;

        percentage.try_into()
    }
}

impl TryFrom<PercentageRef> for CorePercentage {
    type Error = PhpException;

    fn try_from(value: PercentageRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}
