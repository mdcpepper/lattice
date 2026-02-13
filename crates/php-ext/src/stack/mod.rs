//! Promotion Stack/Graph and Layers/Nodes

use std::collections::HashMap;

use ext_php_rs::{
    class::RegisteredClass,
    convert::{FromZval, IntoZval},
    exception::PhpException,
    flags::DataType,
    prelude::*,
    types::Zval,
    zend::ce,
};
use rusty_money::{Money as RustyMoney, iso::Currency};
use slotmap::SlotMap;
use smallvec::SmallVec;

use lattice::{
    graph::{GraphError, PromotionGraph, PromotionGraphBuilder},
    items::{Item as CoreItem, groups::ItemGroup},
    products::ProductKey,
    promotions::{PromotionKey, promotion},
    tags::string::StringTagCollection,
};

use crate::{
    items::{Item, ItemRef},
    money::{Money, MoneyRef},
    promotions::direct_discount::{DirectDiscountPromotion, DirectDiscountPromotionRef},
    receipt::{
        Receipt,
        applications::{PromotionApplication, PromotionApplicationRef},
    },
    stack::layers::{Layer, LayerOutput, LayerRef},
};

pub mod layers;

/// Exception thrown when a stack or layer configuration is invalid
#[derive(Default)]
#[php_class]
#[php(
    name = "Lattice\\Stack\\InvalidStackException",
    extends(ce = ce::exception, stub = "\\Exception")
)]
pub struct InvalidStackException;

#[php_impl]
impl InvalidStackException {}

#[derive(Debug, Default)]
#[php_class]
#[php(name = "Lattice\\StackBuilder")]
pub struct StackBuilder {
    #[php(prop)]
    layers: Vec<LayerRef>,

    #[php(prop)]
    root_layer: Option<LayerRef>,
}

#[php_impl]
impl StackBuilder {
    pub fn __construct() -> Self {
        Self::default()
    }

    pub fn add_layer(&mut self, layer: LayerRef) -> LayerRef {
        self.layers.push(layer.clone());

        layer
    }

    pub fn set_root(&mut self, layer: LayerRef) -> PhpResult<()> {
        if !self
            .layers
            .iter()
            .any(|candidate| candidate.is_identical(&layer))
        {
            return Err(PhpException::from_class::<InvalidStackException>(format!(
                "Root layer must be one of the {} layer(s) added to this StackBuilder.",
                self.layers.len()
            )));
        }

        self.root_layer = Some(layer);

        Ok(())
    }

    pub fn build(&self) -> PhpResult<Stack> {
        if self.layers.is_empty() {
            return Err(PhpException::from_class::<InvalidStackException>(
                "StackBuilder has no layers.".to_string(),
            ));
        }

        let mut layers = self.layers.clone();

        if let Some(ref root_layer) = self.root_layer {
            let Some(root_index) = layers
                .iter()
                .position(|candidate| candidate.is_identical(root_layer))
            else {
                return Err(PhpException::from_class::<InvalidStackException>(
                    "Root layer is not part of this StackBuilder.".to_string(),
                ));
            };

            layers.rotate_left(root_index);
        }

        Ok(Stack { layers })
    }
}

#[derive(Debug, Clone)]
#[php_class]
#[php(name = "Lattice\\Stack")]
pub struct Stack {
    #[php(prop)]
    layers: Vec<LayerRef>,
}

#[php_impl]
impl Stack {
    pub fn __construct(layers: Option<Vec<LayerRef>>) -> Self {
        Self {
            layers: layers.unwrap_or_default(),
        }
    }

    pub fn validate_graph(&self) -> PhpResult<bool> {
        self.try_to_core_graph()?;

        Ok(true)
    }

    pub fn process(&self, items: Vec<ItemRef>) -> PhpResult<Receipt> {
        self.process_items(items)
    }
}

struct BuiltGraph {
    graph: PromotionGraph<'static>,
    promotions: HashMap<PromotionKey, DirectDiscountPromotionRef>,
}

impl Stack {
    pub(crate) fn try_to_core_graph(&self) -> Result<PromotionGraph<'static>, PhpException> {
        Ok(self.try_build_graph()?.graph)
    }

    fn try_build_graph(&self) -> Result<BuiltGraph, PhpException> {
        if self.layers.is_empty() {
            return Err(PhpException::from_class::<InvalidStackException>(
                "Stack must contain at least one layer.".to_string(),
            ));
        }

        let mut builder = PromotionGraphBuilder::new();
        let mut promotion_keys = SlotMap::<PromotionKey, ()>::with_key();
        let mut promotion_ref_map = HashMap::new();
        let mut layer_nodes = Vec::with_capacity(self.layers.len());

        for (idx, layer_ref) in self.layers.iter().enumerate() {
            let layer: Layer = layer_ref.try_into()?;
            let output_mode = layer.output();

            if output_mode == LayerOutput::Split {
                return Err(PhpException::from_class::<InvalidStackException>(
                    "LayerOutput::Split is not supported in linear Stack yet.".to_string(),
                ));
            }

            let mut core_promotions = Vec::with_capacity(layer.promotions().len());

            for promo in layer.promotions() {
                let promotion_key = promotion_keys.insert(());
                let promotion_ref = promo.clone();
                let promo: DirectDiscountPromotion = promo.try_into()?;

                promotion_ref_map.insert(promotion_key, promotion_ref);

                core_promotions.push(promotion(promo.try_to_core_with_reference(promotion_key)?));
            }

            let node = builder
                .add_layer(format!("Layer {idx}"), core_promotions, output_mode.into())
                .map_err(graph_error_to_php_exception)?;

            layer_nodes.push(node);
        }

        if let Some(root) = layer_nodes.first().copied() {
            builder.set_root(root);
        }

        for edge in layer_nodes.windows(2) {
            builder
                .connect_pass_through(edge[0], edge[1])
                .map_err(graph_error_to_php_exception)?;
        }

        let graph = PromotionGraph::from_builder(builder).map_err(graph_error_to_php_exception)?;

        Ok(BuiltGraph {
            graph,
            promotions: promotion_ref_map,
        })
    }

    fn process_items(&self, items: Vec<ItemRef>) -> Result<Receipt, PhpException> {
        let built_graph = self.try_build_graph()?;
        let (item_group, php_items, subtotal) = build_item_group_and_subtotal(&items)?;

        let result = built_graph
            .graph
            .evaluate(&item_group)
            .map_err(graph_error_to_php_exception)?;

        let mut full_price_items = Vec::with_capacity(result.full_price_items.len());

        for item_idx in &result.full_price_items {
            let item = php_items.get(*item_idx).ok_or_else(|| {
                PhpException::from_class::<InvalidStackException>(format!(
                    "Internal error: full-price item index {item_idx} is out of bounds."
                ))
            })?;

            full_price_items.push(item.clone());
        }

        let mut promotion_applications = Vec::new();
        let mut application_item_indexes: Vec<_> =
            result.item_applications.keys().copied().collect();

        application_item_indexes.sort_unstable();

        for item_idx in application_item_indexes {
            let item = php_items.get(item_idx).ok_or_else(|| {
                PhpException::from_class::<InvalidStackException>(format!(
                    "Internal error: application item index {item_idx} is out of bounds."
                ))
            })?;

            let apps = result.item_applications.get(&item_idx).ok_or_else(|| {
                PhpException::from_class::<InvalidStackException>(format!(
                    "Internal error: missing applications for item index {item_idx}."
                ))
            })?;

            for app in apps {
                let promotion =
                    built_graph
                        .promotions
                        .get(&app.promotion_key)
                        .ok_or_else(|| {
                            PhpException::from_class::<InvalidStackException>(
                                "Internal error: application references unknown promotion object."
                                    .to_string(),
                            )
                        })?;

                let original_price = money_ref_from_core(app.original_price)?;
                let final_price = money_ref_from_core(app.final_price)?;

                let application = PromotionApplication::__construct(
                    promotion.clone(),
                    item.clone(),
                    app.bundle_id,
                    original_price,
                    final_price,
                );

                promotion_applications.push(PromotionApplicationRef::from_application(application));
            }
        }

        let total = money_ref_from_core(result.total)?;

        Ok(Receipt::__construct(
            subtotal,
            total,
            full_price_items,
            promotion_applications,
        ))
    }
}

fn build_item_group_and_subtotal(
    items: &[ItemRef],
) -> Result<(ItemGroup<'static>, Vec<ItemRef>, MoneyRef), PhpException> {
    if items.is_empty() {
        return Err(PhpException::from_class::<InvalidStackException>(
            "Stack::process requires at least one item so currency can be determined.".to_string(),
        ));
    }

    let mut product_keys = SlotMap::<ProductKey, ()>::with_key();
    let mut core_items: SmallVec<[CoreItem<'static, StringTagCollection>; 10]> =
        SmallVec::with_capacity(items.len());

    let mut subtotal_minor: i64 = 0;
    let mut currency: Option<&'static Currency> = None;

    let mut php_items = Vec::with_capacity(items.len());

    for (idx, item_ref) in items.iter().enumerate() {
        let item: Item = item_ref.clone().try_into()?;
        let price_ref = item.price();

        let price: RustyMoney<'static, Currency> = price_ref.clone().try_into().map_err(|e| {
            PhpException::from_class::<InvalidStackException>(format!(
                "Item {idx} price is invalid: {e}"
            ))
        })?;

        let item_currency = price.currency();

        match currency {
            Some(expected) if expected != item_currency => {
                return Err(PhpException::from_class::<InvalidStackException>(format!(
                    "Item {idx} has currency {}, expected {}.",
                    item_currency.iso_alpha_code, expected.iso_alpha_code,
                )));
            }
            None => currency = Some(item_currency),
            Some(_) => {}
        }

        subtotal_minor = subtotal_minor
            .checked_add(price.to_minor_units())
            .ok_or_else(|| {
                PhpException::from_class::<InvalidStackException>(
                    "Basket subtotal overflowed i64 minor units.".to_string(),
                )
            })?;

        let tags: SmallVec<[String; 5]> = item.tags().iter().cloned().collect();

        core_items.push(CoreItem::with_tags(
            product_keys.insert(()),
            price,
            StringTagCollection::new(tags),
        ));

        php_items.push(item_ref.clone());
    }

    let currency = currency.ok_or_else(|| {
        PhpException::from_class::<InvalidStackException>(
            "Unable to determine basket currency from items.".to_string(),
        )
    })?;

    let subtotal = money_ref_from_minor(subtotal_minor, currency)?;

    Ok((ItemGroup::new(core_items, currency), php_items, subtotal))
}

fn money_ref_from_core(money: RustyMoney<'static, Currency>) -> Result<MoneyRef, PhpException> {
    money_ref_from_minor(money.to_minor_units(), money.currency())
}

fn money_ref_from_minor(
    amount: i64,
    currency: &'static Currency,
) -> Result<MoneyRef, PhpException> {
    Money::__construct(amount, currency.iso_alpha_code.to_string()).map(MoneyRef::from_money)
}

fn graph_error_to_php_exception(error: GraphError) -> PhpException {
    PhpException::from_class::<InvalidStackException>(format!("Unable to process stack: {error}"))
}

#[derive(Debug)]
pub struct StackRef(Zval);

impl StackRef {
    pub fn from_stack(stack: Stack) -> Self {
        let mut zv = Zval::new();

        stack
            .set_zval(&mut zv, false)
            .expect("stack should always convert to object zval");

        Self(zv)
    }
}

impl<'a> FromZval<'a> for StackRef {
    const TYPE: DataType = DataType::Object(Some(<Stack as RegisteredClass>::CLASS_NAME));

    fn from_zval(zval: &'a Zval) -> Option<Self> {
        let obj = zval.object()?;

        if obj.is_instance::<Stack>() {
            Some(Self(zval.shallow_clone()))
        } else {
            None
        }
    }
}

impl Clone for StackRef {
    fn clone(&self) -> Self {
        Self(self.0.shallow_clone())
    }
}

impl IntoZval for StackRef {
    const TYPE: DataType = DataType::Object(Some(<Stack as RegisteredClass>::CLASS_NAME));
    const NULLABLE: bool = false;

    fn set_zval(self, zv: &mut Zval, persistent: bool) -> ext_php_rs::error::Result<()> {
        self.0.set_zval(zv, persistent)
    }
}

impl TryFrom<&StackRef> for Stack {
    type Error = PhpException;

    fn try_from(value: &StackRef) -> Result<Self, Self::Error> {
        let Some(obj) = value.0.object() else {
            return Err(PhpException::from_class::<InvalidStackException>(
                "Stack object is invalid.".to_string(),
            ));
        };

        let layers = obj.get_property::<Vec<LayerRef>>("layers").map_err(|_| {
            PhpException::from_class::<InvalidStackException>(
                "Stack layers property is invalid.".to_string(),
            )
        })?;

        Ok(Self { layers })
    }
}

impl TryFrom<StackRef> for Stack {
    type Error = PhpException;

    fn try_from(value: StackRef) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}
