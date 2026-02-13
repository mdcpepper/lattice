//! Qualification DTOs

use std::collections::HashSet;

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    flags::DataType,
    prelude::*,
    types::Zval,
};
use smallvec::SmallVec;

use lattice::{
    promotions::qualification::{
        BoolOp as CoreBoolOp, Qualification as CoreQualification,
        QualificationRule as CoreQualificationRule,
    },
    tags::string::StringTagCollection,
};

#[derive(Debug, Clone, Copy)]
#[php_enum]
#[php(name = "Lattice\\Qualification\\BoolOp")]
pub enum BoolOp {
    #[php(value = "and")]
    AndOp,

    #[php(value = "or")]
    OrOp,
}

impl From<BoolOp> for CoreBoolOp {
    fn from(value: BoolOp) -> Self {
        match value {
            BoolOp::AndOp => Self::And,
            BoolOp::OrOp => Self::Or,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[php_enum]
#[php(name = "Lattice\\Qualification\\RuleKind")]
pub enum RuleKind {
    #[php(value = "has_all")]
    HasAll,

    #[php(value = "has_any")]
    HasAny,

    #[php(value = "has_none")]
    HasNone,

    #[php(value = "group")]
    Group,
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Qualification\\Rule")]
pub struct Rule {
    #[php(prop)]
    kind: RuleKind,

    #[php(prop)]
    tags: HashSet<String>,

    #[php(prop)]
    group: Option<QualificationRef>,
}

#[php_impl]
impl Rule {
    pub fn has_all(tags: Option<HashSet<String>>) -> Self {
        Self {
            kind: RuleKind::HasAll,
            tags: tags.unwrap_or_default(),
            group: None,
        }
    }

    pub fn has_any(tags: Option<HashSet<String>>) -> Self {
        Self {
            kind: RuleKind::HasAny,
            tags: tags.unwrap_or_default(),
            group: None,
        }
    }

    pub fn has_none(tags: Option<HashSet<String>>) -> Self {
        Self {
            kind: RuleKind::HasNone,
            tags: tags.unwrap_or_default(),
            group: None,
        }
    }

    pub fn group(qualification: QualificationRef) -> Self {
        Self {
            kind: RuleKind::Group,
            tags: HashSet::default(),
            group: Some(qualification),
        }
    }

    pub fn matches(&self, item_tags: Option<HashSet<String>>) -> PhpResult<bool> {
        let core_rule: CoreQualificationRule<StringTagCollection> = self.clone().try_into()?;

        let item_tags = tags_to_collection(item_tags.unwrap_or_default());

        let qualification =
            CoreQualification::new(CoreBoolOp::And, SmallVec::from_vec(vec![core_rule]));

        Ok(qualification.matches(&item_tags))
    }
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Qualification")]
pub struct Qualification {
    #[php(prop)]
    op: BoolOp,

    #[php(prop)]
    rules: Vec<QualificationRuleRef>,
}

#[php_impl]
impl Qualification {
    pub fn __construct(op: BoolOp, rules: Option<Vec<QualificationRuleRef>>) -> Self {
        Self {
            op,
            rules: rules.unwrap_or_default(),
        }
    }

    pub fn match_all() -> Self {
        Self {
            op: BoolOp::AndOp,
            rules: Vec::default(),
        }
    }

    pub fn match_any(tags: Option<HashSet<String>>) -> Self {
        let tags = tags.unwrap_or_default();

        if tags.is_empty() {
            return Self::match_all();
        }

        Self {
            op: BoolOp::AndOp,
            rules: vec![QualificationRuleRef::from_rule(Rule::has_any(Some(tags)))],
        }
    }

    pub fn matches(&self, item_tags: Option<HashSet<String>>) -> PhpResult<bool> {
        let qualification: CoreQualification<StringTagCollection> = self.clone().try_into()?;

        let item_tags = tags_to_collection(item_tags.unwrap_or_default());

        Ok(qualification.matches(&item_tags))
    }
}

#[derive(Debug)]
pub struct QualificationRef(Zval);

impl QualificationRef {
    pub fn from_qualification(qualification: Qualification) -> Self {
        let mut zv = Zval::new();

        qualification
            .set_zval(&mut zv, false)
            .expect("qualification should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for QualificationRef {
    const TYPE: DataType = DataType::Object(Some(<Qualification as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<Qualification>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for QualificationRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for QualificationRef {
    const TYPE: DataType = DataType::Object(Some(<Qualification as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

#[derive(Debug)]
pub struct QualificationRuleRef(Zval);

impl QualificationRuleRef {
    pub fn from_rule(rule: Rule) -> Self {
        let mut zv = Zval::new();

        rule.set_zval(&mut zv, false)
            .expect("rule should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for QualificationRuleRef {
    const TYPE: DataType = DataType::Object(Some(<Rule as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<Rule>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for QualificationRuleRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for QualificationRuleRef {
    const NULLABLE: bool = false;
    const TYPE: DataType = DataType::Object(Some(<Rule as RegisteredClass>::CLASS_NAME));

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&QualificationRef> for Qualification {
    type Error = PhpException;

    fn try_from(value: &QualificationRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default(
                "Qualification object is invalid.".to_string(),
            ));
        };

        let op = obj
            .get_property::<BoolOp>("op")
            .map_err(|_| PhpException::default("Qualification op is invalid.".to_string()))?;

        let rules = obj
            .get_property::<Vec<QualificationRuleRef>>("rules")
            .map_err(|_| PhpException::default("Qualification rules are invalid.".to_string()))?;

        Ok(Qualification { op, rules })
    }
}

impl TryFrom<QualificationRef> for Qualification {
    type Error = PhpException;

    fn try_from(value: QualificationRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&QualificationRef> for CoreQualification<StringTagCollection> {
    type Error = PhpException;

    fn try_from(value: &QualificationRef) -> Result<Self, Self::Error> {
        let qualification: Qualification = value.try_into()?;

        qualification.try_into()
    }
}

impl TryFrom<QualificationRef> for CoreQualification<StringTagCollection> {
    type Error = PhpException;

    fn try_from(value: QualificationRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&QualificationRuleRef> for Rule {
    type Error = PhpException;

    fn try_from(value: &QualificationRuleRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::default("Rule object is invalid.".to_string()));
        };

        let kind = obj
            .get_property::<RuleKind>("kind")
            .map_err(|_| PhpException::default("Rule kind is invalid.".to_string()))?;

        let tags = obj
            .get_property::<HashSet<String>>("tags")
            .map_err(|_| PhpException::default("Rule tags are invalid.".to_string()))?;

        let group = obj
            .get_property::<Option<QualificationRef>>("group")
            .map_err(|_| PhpException::default("Rule group is invalid.".to_string()))?;

        Ok(Rule { kind, tags, group })
    }
}

impl TryFrom<QualificationRuleRef> for Rule {
    type Error = PhpException;

    fn try_from(value: QualificationRuleRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<&QualificationRuleRef> for CoreQualificationRule<StringTagCollection> {
    type Error = PhpException;

    fn try_from(value: &QualificationRuleRef) -> Result<Self, Self::Error> {
        let rule: Rule = value.try_into()?;

        rule.try_into()
    }
}

impl TryFrom<QualificationRuleRef> for CoreQualificationRule<StringTagCollection> {
    type Error = PhpException;

    fn try_from(value: QualificationRuleRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

impl TryFrom<Qualification> for CoreQualification<StringTagCollection> {
    type Error = PhpException;

    fn try_from(qualification: Qualification) -> Result<Self, Self::Error> {
        let op: CoreBoolOp = qualification.op.into();

        let mut rules = SmallVec::<[CoreQualificationRule<StringTagCollection>; 2]>::new();

        for rule in qualification.rules {
            rules.push(rule.try_into()?);
        }

        Ok(CoreQualification::new(op, rules))
    }
}

impl TryFrom<Rule> for CoreQualificationRule<StringTagCollection> {
    type Error = PhpException;

    fn try_from(rule: Rule) -> Result<Self, Self::Error> {
        match rule.kind {
            RuleKind::HasAll => Ok(CoreQualificationRule::HasAll {
                tags: tags_to_collection(rule.tags),
            }),
            RuleKind::HasAny => Ok(CoreQualificationRule::HasAny {
                tags: tags_to_collection(rule.tags),
            }),
            RuleKind::HasNone => Ok(CoreQualificationRule::HasNone {
                tags: tags_to_collection(rule.tags),
            }),
            RuleKind::Group => {
                let Some(group) = rule.group else {
                    return Err(PhpException::default(
                        "Group rule requires a nested qualification.".to_string(),
                    ));
                };

                Ok(CoreQualificationRule::Group(Box::new(group.try_into()?)))
            }
        }
    }
}

fn tags_to_collection(tags: HashSet<String>) -> StringTagCollection {
    StringTagCollection::new(tags.into_iter().collect())
}
