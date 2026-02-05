//! ILP Typst Renderer
//!
//! This module provides a renderer that captures ILP formulations and outputs
//! them as Typst mathematical documents.
//!
//! # Example
//!
//! ```rust,no_run
//! use dante::solvers::ilp::{ILPSolver, renderers::typst::TypstRenderer};
//! use std::path::PathBuf;
//! # use dante::{fixtures::Fixture, items::groups::ItemGroup};
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let fixture = Fixture::from_set("example_direct_discounts")?;
//! # let basket = fixture.basket(Some(10))?;
//! # let item_group = ItemGroup::from(&basket);
//! # let promotions = fixture.promotions();
//!
//! let mut renderer = TypstRenderer::new(PathBuf::from("formulation.typ"));
//!
//! let _result = ILPSolver::solve_with_observer(promotions, &item_group, &mut renderer)?;
//!
//! renderer.write()?;
//! # Ok(())
//! # }
//! ```

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use good_lp::{Expression, IntoAffineExpression, Variable};
use rustc_hash::FxHashMap;
use slotmap::SlotMap;
use smallvec::SmallVec;

use crate::{
    items::groups::ItemGroup,
    products::{Product, ProductKey},
    promotions::{PromotionKey, PromotionMeta},
    solvers::ilp::ILPObserver,
};

/// Type alias for a promotion constraint tuple.
type PromotionConstraint = (PromotionKey, String, Expression, String, f64);

/// Captured auxiliary/internal variable metadata.
#[derive(Debug, Clone)]
pub struct AuxiliaryVariable {
    /// Promotion key associated with the variable.
    pub promotion_key: PromotionKey,

    /// Variable associated with the auxiliary variable.
    pub var: Variable,

    /// Role of the auxiliary variable.
    pub role: String,

    /// Position of the auxiliary variable.
    pub position: Option<usize>,

    /// State of the auxiliary variable.
    pub state: Option<usize>,
}

/// Errors that can occur during Typst rendering.
#[derive(Debug, thiserror::Error)]
pub enum TypstRenderError {
    /// Failed to write to the output file.
    #[error("Failed to write to output file: {0}")]
    IoError(#[from] std::io::Error),
}

/// Captured ILP formulation data.
///
/// This structure holds all the information about an ILP problem as it's built:
/// variables (presence and promotion), objective function, and constraints.
#[derive(Debug, Clone)]
pub struct ILPFormulation {
    /// Presence variables: `item_idx` -> (`var`, `price_minor`)
    pub presence_vars: FxHashMap<usize, (Variable, i64)>,

    /// Promotion variables captured from observer callbacks.
    pub promotion_vars: SmallVec<[PromotionVariable; 20]>,

    /// Auxiliary/internal variables (e.g., DFA state and transitions)
    pub auxiliary_vars: SmallVec<[AuxiliaryVariable; 20]>,

    /// Variable display labels (e.g., `x_1`, `y_2`, `s_1`, `t_1`)
    pub var_labels: FxHashMap<Variable, String>,

    /// Per-prefix counters for assigning labels.
    pub var_counters: FxHashMap<String, usize>,

    /// Exclusivity constraints: `item_idx` -> expression
    pub exclusivity_constraints: FxHashMap<usize, Expression>,

    /// Promotion constraints: (key, type, expr, relation, rhs)
    pub promotion_constraints: SmallVec<[PromotionConstraint; 20]>,

    /// Objective terms: `var` -> coefficient (minor units)
    pub objective_terms: FxHashMap<Variable, f64>,
}

/// Promotion decision variable captured by the observer.
#[derive(Debug, Clone)]
pub struct PromotionVariable {
    /// Promotion that generated this variable.
    pub promotion_key: PromotionKey,

    /// Basket item index the variable refers to.
    pub item_idx: usize,

    /// ILP decision variable.
    pub var: Variable,

    /// Price in minor units (full price for participation, discounted for discount vars).
    pub price_minor: i64,

    /// Optional metadata tag (e.g., "participation", "discount").
    pub metadata: Option<String>,
}

impl ILPFormulation {
    /// Create a new empty formulation.
    pub fn new() -> Self {
        Self {
            presence_vars: FxHashMap::default(),
            promotion_vars: SmallVec::new(),
            auxiliary_vars: SmallVec::new(),
            var_labels: FxHashMap::default(),
            var_counters: FxHashMap::default(),
            exclusivity_constraints: FxHashMap::default(),
            promotion_constraints: SmallVec::new(),
            objective_terms: FxHashMap::default(),
        }
    }
}

impl Default for ILPFormulation {
    fn default() -> Self {
        Self::new()
    }
}

/// Typst renderer that implements `ILPObserver`.
///
/// This renderer captures the ILP formulation as it's being built and can
/// render it to a Typst document showing the mathematical formulation.
#[derive(Debug, Clone)]
pub struct TypstRenderer {
    /// Captured formulation
    formulation: Arc<Mutex<ILPFormulation>>,

    /// Output path for the .typ file
    output_path: PathBuf,

    /// Item index -> product name
    item_names: Vec<Option<String>>,

    /// Promotion key -> promotion name
    promotion_names: FxHashMap<PromotionKey, String>,
}

impl TypstRenderer {
    /// Create a new Typst renderer.
    pub fn new(output_path: PathBuf) -> Self {
        Self {
            formulation: Arc::new(Mutex::new(ILPFormulation::new())),
            output_path,
            item_names: Vec::new(),
            promotion_names: FxHashMap::default(),
        }
    }

    /// Create a renderer and attach product/promotion metadata for naming.
    pub fn new_with_metadata<'a>(
        output_path: PathBuf,
        item_group: &ItemGroup<'a>,
        product_meta: &SlotMap<ProductKey, Product<'a>>,
        promotion_meta: &SlotMap<PromotionKey, PromotionMeta>,
    ) -> Self {
        let item_names = item_group
            .iter()
            .map(|item| {
                product_meta
                    .get(item.product())
                    .map(|product| product.name.clone())
            })
            .collect();

        let promotion_names = promotion_meta
            .iter()
            .map(|(key, meta)| (key, meta.name.clone()))
            .collect();

        Self {
            formulation: Arc::new(Mutex::new(ILPFormulation::new())),
            output_path,
            item_names,
            promotion_names,
        }
    }

    /// Get a reference to the captured formulation.
    pub fn formulation(&self) -> ILPFormulation {
        self.formulation.lock().map_or_else(
            |poisoned| poisoned.into_inner().clone(),
            |formulation| formulation.clone(),
        )
    }

    /// Get the output path.
    pub fn output_path(&self) -> &PathBuf {
        &self.output_path
    }

    /// Render the captured ILP formulation to Typst syntax.
    pub fn render(&self) -> String {
        let mut output = String::new();
        let mut formulation = self.formulation();
        self.assign_promotion_labels(&mut formulation);

        output.push_str("= ILP Formulation for Basket Pricing\n\n");

        output.push_str("== Decision Variables\n\n");
        self.render_variables(&formulation, &mut output);

        output.push_str("\n== Objective Function\n\n");
        Self::render_objective(&formulation, &mut output);

        output.push_str("\n== Constraints\n\n");
        self.render_constraints(&formulation, &mut output);

        output.push_str("\n== Full ILP in Standard Form\n\n");
        Self::render_standard_form(&formulation, &mut output);

        output
    }

    /// Write the rendered formulation to the output file.
    ///
    /// # Errors
    ///
    /// Returns [`TypstRenderError::IoError`] if the file cannot be created or written.
    pub fn write(&self) -> Result<(), TypstRenderError> {
        let content = self.render();
        let mut file = File::create(&self.output_path)?;

        file.write_all(content.as_bytes())?;

        Ok(())
    }

    /// Convenience method: render and write in one call.
    ///
    /// # Errors
    ///
    /// Returns [`TypstRenderError::IoError`] if the file cannot be created or written.
    pub fn render_to_file(&self) -> Result<(), TypstRenderError> {
        self.write()
    }

    /// Convert a variable to a Typst variable name using the default prefix.
    fn var_name(var: Variable) -> String {
        Self::var_name_with_prefix(var, "x")
    }

    fn var_name_with_prefix(var: Variable, prefix: &str) -> String {
        if let Some(index) = Self::var_index(var) {
            format!("{prefix}_{index}")
        } else {
            let debug = format!("{var:?}");

            let cleaned: String = debug
                .chars()
                .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
                .collect();

            format!("{prefix}_{cleaned}")
        }
    }

    fn var_index(var: Variable) -> Option<usize> {
        let debug = format!("{var:?}");
        let mut digits = String::new();
        let mut in_digits = false;

        for ch in debug.chars() {
            if ch.is_ascii_digit() {
                digits.push(ch);
                in_digits = true;
            } else if in_digits {
                break;
            }
        }

        if digits.is_empty() {
            None
        } else {
            digits.parse().ok()
        }
    }

    fn item_label(&self, item_idx: usize, with_name: bool) -> String {
        let display_idx = item_idx + 1;

        if with_name {
            match self.item_names.get(item_idx).and_then(|name| name.as_ref()) {
                Some(name) => format!("Item {display_idx} ({name})"),
                None => format!("Item {display_idx}"),
            }
        } else {
            format!("Item {display_idx}")
        }
    }

    fn promotion_label(&self, promotion_key: PromotionKey) -> String {
        self.promotion_names
            .get(&promotion_key)
            .cloned()
            .unwrap_or_else(|| format!("{promotion_key:?}"))
    }

    /// Render minor units directly (used in ILP formulation output).
    fn render_minor_units(minor_units: i64) -> String {
        let sign = if minor_units < 0 { "-" } else { "" };
        let abs = minor_units.unsigned_abs();
        format!("{sign}{abs}")
    }

    fn render_price(minor_units: i64) -> String {
        Self::render_minor_units(minor_units)
    }

    fn render_number(value: f64) -> String {
        if (value - value.round()).abs() < 1e-9 {
            format!("{value:.0}")
        } else {
            format!("{value}")
        }
    }

    fn render_objective_coeff(value: f64) -> String {
        Self::render_number(value)
    }

    fn var_label(formulation: &ILPFormulation, var: Variable) -> String {
        formulation
            .var_labels
            .get(&var)
            .cloned()
            .unwrap_or_else(|| Self::var_name(var))
    }

    fn label_order(label: &str) -> Option<(usize, usize)> {
        let (prefix, idx_str) = label.split_once('_')?;
        let idx = idx_str.parse().ok()?;
        let prefix_order = match prefix {
            "x" => 0,
            "y" => 1,
            "s" => 2,
            "t" => 3,
            "a" => 4,
            _ => 5,
        };

        Some((prefix_order, idx))
    }

    fn var_sort_key(formulation: &ILPFormulation, var: Variable) -> (usize, usize, usize, String) {
        if let Some(label) = formulation.var_labels.get(&var) {
            if let Some((prefix_order, idx)) = Self::label_order(label) {
                return (0, prefix_order, idx, label.clone());
            }

            return (0, 99, usize::MAX, label.clone());
        }

        if let Some(idx) = Self::var_index(var) {
            return (1, 99, idx, String::new());
        }

        (2, 99, usize::MAX, format!("{var:?}"))
    }

    fn assign_label(formulation: &mut ILPFormulation, var: Variable, prefix: &str) -> String {
        if let Some(label) = formulation.var_labels.get(&var) {
            return label.clone();
        }

        let counter = formulation
            .var_counters
            .entry(prefix.to_string())
            .or_insert(0);

        *counter += 1;

        let label = format!("{prefix}_{}", *counter);

        formulation.var_labels.insert(var, label.clone());

        label
    }

    fn metadata_rank(metadata: Option<&str>) -> usize {
        match metadata {
            None => 0,
            Some("participation") => 1,
            Some("discount") => 2,
            Some(_) => 3,
        }
    }

    fn assign_promotion_labels(&self, formulation: &mut ILPFormulation) {
        for promo in &formulation.promotion_vars {
            formulation.var_labels.remove(&promo.var);
        }

        formulation.var_counters.insert(String::from("y"), 0);

        let mut promos = formulation.promotion_vars.clone();

        promos.sort_by(|a, b| {
            self.promotion_label(a.promotion_key)
                .cmp(&self.promotion_label(b.promotion_key))
                .then_with(|| a.item_idx.cmp(&b.item_idx))
                .then_with(|| {
                    Self::metadata_rank(a.metadata.as_deref())
                        .cmp(&Self::metadata_rank(b.metadata.as_deref()))
                })
                .then_with(|| {
                    let a_idx = Self::var_index(a.var).unwrap_or(usize::MAX);
                    let b_idx = Self::var_index(b.var).unwrap_or(usize::MAX);
                    a_idx.cmp(&b_idx)
                })
        });

        for promo in promos {
            let _ = Self::assign_label(formulation, promo.var, "y");
        }
    }

    /// Render an Expression to Typst math notation.
    fn render_expression(formulation: &ILPFormulation, expr: &Expression) -> String {
        let mut terms: Vec<(Variable, f64)> = expr.linear_coefficients().collect();

        terms.sort_by(|(a_var, _), (b_var, _)| {
            Self::var_sort_key(formulation, *a_var).cmp(&Self::var_sort_key(formulation, *b_var))
        });

        let mut out = String::new();
        let mut first = true;

        for (var, coeff) in terms {
            if coeff.abs() < f64::EPSILON {
                continue;
            }

            let coeff_abs = coeff.abs();

            let term = if (coeff_abs - 1.0).abs() < f64::EPSILON {
                Self::var_label(formulation, var)
            } else {
                format!(
                    "{} {}",
                    Self::render_number(coeff_abs),
                    Self::var_label(formulation, var)
                )
            };

            if first {
                if coeff < 0.0 {
                    out.push_str("- ");
                }

                out.push_str(&term);

                first = false;
            } else if coeff < 0.0 {
                out.push_str(" - ");
                out.push_str(&term);
            } else {
                out.push_str(" + ");
                out.push_str(&term);
            }
        }

        let constant = expr.constant();

        if constant.abs() >= f64::EPSILON {
            let constant_abs = constant.abs();
            let constant_str = Self::render_number(constant_abs);

            if first {
                if constant < 0.0 {
                    out.push('-');
                }

                out.push_str(&constant_str);
            } else if constant < 0.0 {
                out.push_str(" - ");
                out.push_str(&constant_str);
            } else {
                out.push_str(" + ");
                out.push_str(&constant_str);
            }
        }

        if out.is_empty() {
            out.push('0');
        }

        out
    }

    fn objective_terms(formulation: &ILPFormulation) -> Vec<(Variable, f64)> {
        if formulation.objective_terms.is_empty() {
            let mut terms = Vec::new();

            for (var, price_minor) in formulation.presence_vars.values() {
                #[expect(clippy::cast_precision_loss, reason = "Rendering-only fallback")]
                let coeff = *price_minor as f64;

                terms.push((*var, coeff));
            }

            for promo in &formulation.promotion_vars {
                #[expect(clippy::cast_precision_loss, reason = "Rendering-only fallback")]
                let coeff = promo.price_minor as f64;

                terms.push((promo.var, coeff));
            }

            terms
        } else {
            formulation
                .objective_terms
                .iter()
                .map(|(var, coeff)| (*var, *coeff))
                .collect()
        }
    }

    fn render_objective_lines(
        formulation: &ILPFormulation,
        terms: &[(Variable, f64)],
        per_line: usize,
    ) -> Vec<String> {
        let mut sorted: Vec<(Variable, f64)> = terms.to_vec();

        sorted.sort_by(|(a_var, _), (b_var, _)| {
            Self::var_sort_key(formulation, *a_var).cmp(&Self::var_sort_key(formulation, *b_var))
        });

        let mut tokens = Vec::new();

        let mut first = true;

        for (var, coeff) in sorted {
            if coeff.abs() < f64::EPSILON {
                continue;
            }

            let term = format!(
                "{} dot {}",
                Self::render_objective_coeff(coeff.abs()),
                Self::var_label(formulation, var)
            );

            let token = if first {
                if coeff < 0.0 {
                    format!("- {term}")
                } else {
                    term
                }
            } else if coeff < 0.0 {
                format!("- {term}")
            } else {
                format!("+ {term}")
            };

            tokens.push(token);
            first = false;
        }

        if tokens.is_empty() {
            return vec![String::from("0")];
        }

        let mut lines = Vec::new();
        let per_line = per_line.max(1);

        for chunk in tokens.chunks(per_line) {
            lines.push(chunk.join(" "));
        }

        lines
    }

    fn render_variables(&self, formulation: &ILPFormulation, output: &mut String) {
        output.push_str("All decision variables are binary.\n\n");

        output.push_str("=== Presence Variables (Full Price)\n\n");

        let mut presence_items: Vec<_> = formulation.presence_vars.iter().collect();

        presence_items.sort_by_key(|(item_idx, _)| *item_idx);

        for (item_idx, (var, price_minor)) in presence_items {
            output.push_str("- $");
            output.push_str(&Self::var_label(formulation, *var));
            output.push_str("$: ");
            output.push_str(&self.item_label(*item_idx, true));
            output.push_str(" at full price (");
            output.push_str(&Self::render_price(*price_minor));
            output.push_str(")\n");
        }

        output.push_str("\n=== Promotion Variables (Participation & Discounts)\n\n");

        let mut promotion_items: Vec<_> = formulation.promotion_vars.iter().collect();

        promotion_items.sort_by(|a, b| {
            Self::var_sort_key(formulation, a.var).cmp(&Self::var_sort_key(formulation, b.var))
        });

        for promo in promotion_items {
            let meta_str = promo
                .metadata
                .as_ref()
                .map(|m| format!(" [{m}]"))
                .unwrap_or_default();

            output.push_str("- $");
            output.push_str(&Self::var_label(formulation, promo.var));
            output.push_str("$: ");
            output.push_str(&self.item_label(promo.item_idx, false));
            output.push_str(" with promotion \"");
            output.push_str(&self.promotion_label(promo.promotion_key));
            output.push_str("\" (");
            output.push_str(&Self::render_price(promo.price_minor));
            output.push(')');
            output.push_str(&meta_str);
            output.push('\n');
        }

        if !formulation.auxiliary_vars.is_empty() {
            output.push_str("\n=== Auxiliary Variables (DFA)\n\n");

            output.push_str(
                "DFA positions index eligible items (sorted by price desc, then index asc).\n\n",
            );

            let mut aux_items = formulation.auxiliary_vars.to_vec();

            aux_items.sort_by(|a, b| {
                let a_key = format!("{:?}", a.promotion_key);
                let b_key = format!("{:?}", b.promotion_key);
                let a_idx = Self::var_index(a.var).unwrap_or(usize::MAX);
                let b_idx = Self::var_index(b.var).unwrap_or(usize::MAX);

                a_key.cmp(&b_key).then_with(|| a_idx.cmp(&b_idx))
            });

            for aux in aux_items {
                let mut meta = String::new();

                if let Some(pos) = aux.position {
                    meta.push_str("pos=");
                    meta.push_str(&pos.to_string());
                }

                if let Some(state) = aux.state {
                    if !meta.is_empty() {
                        meta.push_str(", ");
                    }

                    meta.push_str("state=");
                    meta.push_str(&state.to_string());
                }

                let meta_str = if meta.is_empty() {
                    String::new()
                } else {
                    format!(" ({meta})")
                };

                output.push_str("- $");
                output.push_str(&Self::var_label(formulation, aux.var));
                output.push_str("$: ");
                output.push_str(&aux.role);
                output.push_str(" for promotion \"");
                output.push_str(&self.promotion_label(aux.promotion_key));
                output.push('"');
                output.push_str(&meta_str);
                output.push('\n');
            }
        }
    }

    fn render_objective(formulation: &ILPFormulation, output: &mut String) {
        output.push_str("Minimize:\n\n");

        let terms = Self::objective_terms(formulation);
        let lines = Self::render_objective_lines(formulation, &terms, 3);

        if let Some(first) = lines.first() {
            output.push_str("$ \"minimize\" quad ");
            output.push_str(first);
            output.push_str(" $\n");
        }

        for line in lines.iter().skip(1) {
            output.push_str("$ quad ");
            output.push_str(line);
            output.push_str(" $\n");
        }
    }

    fn render_constraints(&self, formulation: &ILPFormulation, output: &mut String) {
        output.push_str("=== Exclusivity Constraints\n\n");

        output.push_str("Each item must be purchased exactly once (at full price OR discounted by a single promotion):\n\n");

        let mut exclusivity_items: Vec<_> = formulation.exclusivity_constraints.iter().collect();

        exclusivity_items.sort_by_key(|(item_idx, _)| *item_idx);

        for (item_idx, expr) in exclusivity_items {
            output.push_str("$ ");
            output.push_str(&Self::render_expression(formulation, expr));
            output.push_str(" = 1 $ (");
            output.push_str(&self.item_label(*item_idx, true));
            output.push_str(")\n\n");
        }

        if !formulation.promotion_constraints.is_empty() {
            output.push_str("\n=== Promotion Constraints\n\n");

            for (promo_key, constraint_type, expr, relation, rhs) in
                &formulation.promotion_constraints
            {
                output.push_str("$ ");
                output.push_str(&Self::render_expression(formulation, expr));
                output.push(' ');
                output.push_str(relation);
                output.push(' ');
                output.push_str(&Self::render_number(*rhs));
                output.push_str(" $ (");
                output.push_str(constraint_type);
                output.push_str(" for promotion \"");
                output.push_str(&self.promotion_label(*promo_key));
                output.push_str("\")\n\n");
            }
        }
    }

    fn render_standard_form(formulation: &ILPFormulation, output: &mut String) {
        let objective_terms = Self::objective_terms(formulation);
        let objective_lines = Self::render_objective_lines(formulation, &objective_terms, 3);

        if let Some(first) = objective_lines.first() {
            output.push_str("$ \"minimize\" quad ");
            output.push_str(first);
            output.push_str(" $\n");
        }

        for line in objective_lines.iter().skip(1) {
            output.push_str("$ quad ");
            output.push_str(line);
            output.push_str(" $\n");
        }

        output.push_str("\n$ \"subject to\" quad ");

        let mut wrote_constraint = false;

        let mut exclusivity_items: Vec<_> = formulation.exclusivity_constraints.iter().collect();
        exclusivity_items.sort_by_key(|(item_idx, _)| *item_idx);

        for (_item_idx, expr) in exclusivity_items {
            let line = format!("{} = 1", Self::render_expression(formulation, expr));

            if wrote_constraint {
                output.push_str("$ quad ");
                output.push_str(&line);
                output.push_str(" $\n");
            } else {
                output.push_str(&line);
                output.push_str(" $\n");
                wrote_constraint = true;
            }
        }

        for (_promo_key, _constraint_type, expr, relation, rhs) in
            &formulation.promotion_constraints
        {
            let line = format!(
                "{} {} {}",
                Self::render_expression(formulation, expr),
                relation,
                Self::render_number(*rhs)
            );

            if wrote_constraint {
                output.push_str("$ quad ");
                output.push_str(&line);
                output.push_str(" $\n");
            } else {
                output.push_str(&line);
                output.push_str(" $\n");
                wrote_constraint = true;
            }
        }

        if !wrote_constraint {
            output.push_str("0 = 0 $\n");
        }

        output.push_str("\n$ x_i in {0,1} $\n");
    }
}

impl ILPObserver for TypstRenderer {
    fn on_presence_variable(&mut self, item_idx: usize, var: Variable, price_minor: i64) {
        let mut formulation = self
            .formulation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        let _ = Self::assign_label(&mut formulation, var, "x");

        formulation
            .presence_vars
            .insert(item_idx, (var, price_minor));
    }

    fn on_promotion_variable(
        &mut self,
        promotion_key: PromotionKey,
        item_idx: usize,
        var: Variable,
        discounted_price_minor: i64,
        metadata: Option<&str>,
    ) {
        let mut formulation = self
            .formulation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        let _ = Self::assign_label(&mut formulation, var, "y");

        formulation.promotion_vars.push(PromotionVariable {
            promotion_key,
            item_idx,
            var,
            price_minor: discounted_price_minor,
            metadata: metadata.map(String::from),
        });
    }

    fn on_auxiliary_variable(
        &mut self,
        promotion_key: PromotionKey,
        var: Variable,
        role: &str,
        position: Option<usize>,
        state: Option<usize>,
    ) {
        let mut formulation = self
            .formulation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        formulation.auxiliary_vars.push(AuxiliaryVariable {
            promotion_key,
            var,
            role: role.to_string(),
            position,
            state,
        });

        let prefix = match role {
            "DFA state" => "s",
            "DFA take" => "t",
            _ => "a",
        };

        let _ = Self::assign_label(&mut formulation, var, prefix);
    }

    fn on_objective_term(&mut self, var: Variable, coefficient: f64) {
        let mut formulation = self
            .formulation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        let entry = formulation.objective_terms.entry(var).or_insert(0.0);

        *entry += coefficient;
    }

    fn on_exclusivity_constraint(&mut self, item_idx: usize, constraint_expr: &Expression) {
        let mut formulation = self
            .formulation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        formulation
            .exclusivity_constraints
            .insert(item_idx, constraint_expr.clone());
    }

    fn on_promotion_constraint(
        &mut self,
        promotion_key: PromotionKey,
        constraint_type: &str,
        constraint_expr: &Expression,
        relation: &str,
        rhs: f64,
    ) {
        let mut formulation = self
            .formulation
            .lock()
            .unwrap_or_else(PoisonError::into_inner);

        formulation.promotion_constraints.push((
            promotion_key,
            constraint_type.to_string(),
            constraint_expr.clone(),
            relation.to_string(),
            rhs,
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use decimal_percentage::Percentage;
    use good_lp::{Expression, ProblemVariables, SolverModel, variable};
    use rusty_money::{Money, iso::GBP};
    use slotmap::SlotMap;
    use smallvec::{SmallVec, smallvec};
    use tempfile::tempdir;
    use testresult::TestResult;

    use crate::{
        discounts::SimpleDiscount,
        items::{Item, groups::ItemGroup},
        products::{Product, ProductKey},
        promotions::{
            Promotion, PromotionKey, PromotionMeta, direct_discount::DirectDiscountPromotion,
        },
        solvers::ilp::{ensure_presence_vars_len, promotions::PromotionInstances, state::ILPState},
        tags::string::StringTagCollection,
    };

    use super::*;

    #[test]
    fn renderer_captures_presence_variables() {
        let mut renderer = TypstRenderer::new(PathBuf::from("test.typ"));
        let mut pb = ProblemVariables::new();

        let var1 = pb.add(variable().binary());
        let var2 = pb.add(variable().binary());

        renderer.on_presence_variable(0, var1, 100);
        renderer.on_presence_variable(1, var2, 200);

        assert_eq!(renderer.formulation().presence_vars.len(), 2);
        assert_eq!(
            renderer.formulation().presence_vars.get(&0),
            Some(&(var1, 100))
        );
        assert_eq!(
            renderer.formulation().presence_vars.get(&1),
            Some(&(var2, 200))
        );
    }

    #[test]
    fn renderer_captures_promotion_variables() {
        let mut renderer = TypstRenderer::new(PathBuf::from("test.typ"));
        let mut pb = ProblemVariables::new();

        let var1 = pb.add(variable().binary());
        let promotion_key = PromotionKey::default();

        renderer.on_promotion_variable(promotion_key, 0, var1, 80, Some("discount"));

        assert_eq!(renderer.formulation().promotion_vars.len(), 1);

        let promo = &renderer.formulation().promotion_vars[0];

        assert_eq!(promo.promotion_key, promotion_key);
        assert_eq!(promo.item_idx, 0);
        assert_eq!(promo.var, var1);
        assert_eq!(promo.price_minor, 80);
        assert_eq!(promo.metadata.as_deref(), Some("discount"));
    }

    #[test]
    fn output_path_and_render_to_file_roundtrip() -> TestResult {
        let dir = tempdir()?;
        let path = dir.path().join("formulation.typ");
        let renderer = TypstRenderer::new(path.clone());

        assert_eq!(renderer.output_path(), &path);

        renderer.render_to_file()?;

        let contents = fs::read_to_string(&path)?;
        assert!(contents.contains("ILP Formulation"));

        Ok(())
    }

    #[test]
    fn var_helper_paths_cover_indexed_and_ordering() {
        let mut pb = ProblemVariables::new();
        let var = pb.add(variable().binary());

        let name = TypstRenderer::var_name(var);
        let alt = TypstRenderer::var_name_with_prefix(var, "z");
        assert!(name.starts_with("x_"));
        assert!(alt.starts_with("z_"));
        assert!(TypstRenderer::var_index(var).is_some());

        assert_eq!(TypstRenderer::label_order("x_2"), Some((0, 2)));
        assert_eq!(TypstRenderer::label_order("z_3"), Some((5, 3)));
        assert_eq!(TypstRenderer::label_order("not_a_label"), None);

        let mut formulation = ILPFormulation::new();
        formulation.var_labels.insert(var, String::from("weird"));
        let sort_key = TypstRenderer::var_sort_key(&formulation, var);
        assert_eq!(sort_key.1, 99);
    }

    #[test]
    fn assign_label_reuses_existing_value() {
        let mut pb = ProblemVariables::new();
        let var = pb.add(variable().binary());
        let mut formulation = ILPFormulation::new();

        let first = TypstRenderer::assign_label(&mut formulation, var, "x");
        let second = TypstRenderer::assign_label(&mut formulation, var, "x");

        assert_eq!(first, second);
        assert_eq!(formulation.var_counters.get("x"), Some(&1));
    }

    #[test]
    fn metadata_rank_orders_values() {
        assert_eq!(TypstRenderer::metadata_rank(None), 0);
        assert_eq!(TypstRenderer::metadata_rank(Some("participation")), 1);
        assert_eq!(TypstRenderer::metadata_rank(Some("discount")), 2);
        assert_eq!(TypstRenderer::metadata_rank(Some("other")), 3);
    }

    #[test]
    fn render_full_formulation_includes_aux_and_constraints() {
        let mut products = SlotMap::<ProductKey, Product<'_>>::with_key();
        let item_a = products.insert(Product {
            name: "Alpha".to_string(),
            tags: StringTagCollection::from_strs(&["tag"]),
            price: Money::from_minor(100, GBP),
        });
        let item_b = ProductKey::default();

        let items = SmallVec::from_vec(vec![
            Item::new(item_a, Money::from_minor(100, GBP)),
            Item::new(item_b, Money::from_minor(200, GBP)),
        ]);
        let item_group = ItemGroup::new(items, GBP);

        let mut promotions = SlotMap::<PromotionKey, PromotionMeta>::with_key();
        let promo_a = promotions.insert(PromotionMeta {
            name: "A promo".to_string(),
        });
        let promo_b = promotions.insert(PromotionMeta {
            name: "B promo".to_string(),
        });

        let mut renderer = TypstRenderer::new_with_metadata(
            PathBuf::from("test.typ"),
            &item_group,
            &products,
            &promotions,
        );

        let mut pb = ProblemVariables::new();
        let x1 = pb.add(variable().binary());
        let x2 = pb.add(variable().binary());
        let y1 = pb.add(variable().binary());
        let y2 = pb.add(variable().binary());
        let s1 = pb.add(variable().binary());
        let t1 = pb.add(variable().binary());

        renderer.on_presence_variable(0, x1, 100);
        renderer.on_presence_variable(1, x2, 200);
        renderer.on_promotion_variable(promo_a, 0, y1, 80, None);
        renderer.on_promotion_variable(promo_b, 1, y2, 150, Some("discount"));
        renderer.on_auxiliary_variable(promo_b, s1, "DFA state", Some(0), Some(1));
        renderer.on_auxiliary_variable(promo_b, t1, "DFA take", None, None);

        renderer.on_objective_term(x1, 100.0);
        renderer.on_objective_term(x2, 200.0);
        renderer.on_objective_term(y1, 80.0);
        renderer.on_objective_term(y2, -50.0);

        let exclusivity_expr = Expression::from(x1) + y1;
        renderer.on_exclusivity_constraint(0, &exclusivity_expr);

        let promo_expr = Expression::from(y2) - 1.0;
        renderer.on_promotion_constraint(promo_b, "promo", &promo_expr, "<=", 0.0);

        let output = renderer.render();

        assert!(output.contains("=== Promotion Variables"));
        assert!(output.contains("Auxiliary Variables"));
        assert!(output.contains("DFA state"));
        assert!(output.contains("\"subject to\""));
        assert!(output.contains("<="));
        assert!(output.contains("Item 2"));
        assert!(output.contains("Item 1 (Alpha)"));
        assert!(output.contains("promotion \"A promo\""));
    }

    #[test]
    fn formulation_default_is_empty() {
        let formulation = ILPFormulation::default();

        assert!(formulation.presence_vars.is_empty());
        assert!(formulation.promotion_vars.is_empty());
        assert!(formulation.auxiliary_vars.is_empty());
        assert!(formulation.objective_terms.is_empty());
        assert!(formulation.exclusivity_constraints.is_empty());
        assert!(formulation.promotion_constraints.is_empty());
    }

    #[test]
    fn render_helpers_cover_common_paths() {
        let mut formulation = ILPFormulation::new();
        let mut pb = ProblemVariables::new();
        let var1 = pb.add(variable().binary());
        let var2 = pb.add(variable().binary());

        let _ = TypstRenderer::assign_label(&mut formulation, var1, "x");
        let _ = TypstRenderer::assign_label(&mut formulation, var2, "x");

        let expr = Expression::from(var1) * 2 - var2 + 5.0;
        let rendered = TypstRenderer::render_expression(&formulation, &expr);

        assert!(rendered.contains("2 x_1"));
        assert!(rendered.contains("- x_2"));
        assert!(rendered.contains("+ 5"));

        let zero_expr = Expression::default();
        let zero_rendered = TypstRenderer::render_expression(&formulation, &zero_expr);
        assert_eq!(zero_rendered, "0");

        let empty_lines = TypstRenderer::render_objective_lines(&formulation, &[], 0);
        assert_eq!(empty_lines, vec![String::from("0")]);

        assert_eq!(TypstRenderer::render_number(10.0), "10");
        assert_eq!(TypstRenderer::render_number(10.5), "10.5");
    }

    #[test]
    fn renderer_orders_promotions_by_name_in_render_output() {
        let mut products = SlotMap::<ProductKey, Product<'_>>::with_key();
        let item_a = products.insert(Product {
            name: "Alpha".to_string(),
            tags: StringTagCollection::from_strs(&[]),
            price: Money::from_minor(100, GBP),
        });
        let item_b = products.insert(Product {
            name: "Beta".to_string(),
            tags: StringTagCollection::from_strs(&[]),
            price: Money::from_minor(200, GBP),
        });

        let items = SmallVec::from_vec(vec![
            Item::new(item_a, Money::from_minor(100, GBP)),
            Item::new(item_b, Money::from_minor(200, GBP)),
        ]);

        let item_group = ItemGroup::new(items, GBP);

        let mut promotions = SlotMap::<PromotionKey, PromotionMeta>::with_key();
        let promo_b = promotions.insert(PromotionMeta {
            name: "B promo".to_string(),
        });
        let promo_a = promotions.insert(PromotionMeta {
            name: "A promo".to_string(),
        });

        let mut renderer = TypstRenderer::new_with_metadata(
            PathBuf::from("test.typ"),
            &item_group,
            &products,
            &promotions,
        );

        let mut pb = ProblemVariables::new();
        let var_b = pb.add(variable().binary());
        let var_a = pb.add(variable().binary());

        renderer.on_promotion_variable(promo_b, 0, var_b, 100, None);
        renderer.on_promotion_variable(promo_a, 0, var_a, 90, None);

        let output = renderer.render();

        let a_idx = output.find("A promo").expect("A promo missing");
        let b_idx = output.find("B promo").expect("B promo missing");

        assert!(a_idx < b_idx);

        let expected_price = TypstRenderer::render_price(90);
        let expected_fragment = format!("Item 1 with promotion \"A promo\" ({expected_price})");
        assert!(output.contains(&expected_fragment));
    }

    #[test]
    fn render_standard_form_includes_zero_constraint_when_empty() {
        let renderer = TypstRenderer::new(PathBuf::from("test.typ"));
        let output = renderer.render();

        assert!(output.contains("$ \"subject to\" quad 0 = 0 $"));
        assert!(output.contains("$ x_i in {0,1} $"));
    }

    #[test]
    fn renderer_captures_exclusivity_constraints() {
        let mut renderer = TypstRenderer::new(PathBuf::from("test.typ"));
        let mut pb = ProblemVariables::new();

        let var1 = pb.add(variable().binary());
        let expr = Expression::from(var1);

        renderer.on_exclusivity_constraint(0, &expr);

        assert_eq!(renderer.formulation().exclusivity_constraints.len(), 1);
        assert!(
            renderer
                .formulation()
                .exclusivity_constraints
                .contains_key(&0)
        );
    }

    #[test]
    fn renderer_captures_promotion_constraints() {
        let mut renderer = TypstRenderer::new(PathBuf::from("test.typ"));
        let mut pb = ProblemVariables::new();

        let var1 = pb.add(variable().binary());
        let expr = Expression::from(var1);
        let promotion_key = PromotionKey::default();

        renderer.on_promotion_constraint(promotion_key, "minimum_quantity", &expr, ">=", 3.0);

        let formulation = renderer.formulation();

        assert_eq!(formulation.promotion_constraints.len(), 1);

        let constraint = formulation.promotion_constraints.first();

        assert!(constraint.is_some());

        if let Some((key, ctype, _expr, rel, rhs)) = constraint {
            assert_eq!(*key, promotion_key);
            assert_eq!(ctype, "minimum_quantity");
            assert_eq!(rel, ">=");
            assert!((rhs - 3.0).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn observer_captures_full_formulation_from_solve() -> TestResult {
        #[cfg(feature = "solver-highs")]
        use good_lp::solvers::highs::highs as default_solver;
        #[cfg(all(not(feature = "solver-highs"), feature = "solver-microlp"))]
        use good_lp::solvers::microlp::microlp as default_solver;

        // Create a simple basket with items
        let items: SmallVec<[Item<'_>; 10]> = smallvec![
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(100, GBP),
                StringTagCollection::from_strs(&["fruit"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(200, GBP),
                StringTagCollection::from_strs(&["fruit"]),
            ),
            Item::with_tags(
                ProductKey::default(),
                Money::from_minor(300, GBP),
                StringTagCollection::from_strs(&["vegetable"]),
            ),
        ];

        let item_group = ItemGroup::new(items, GBP);

        // Create a promotion
        let promotions = [Promotion::DirectDiscount(DirectDiscountPromotion::new(
            PromotionKey::default(),
            StringTagCollection::from_strs(&["fruit"]),
            SimpleDiscount::PercentageOff(Percentage::from(0.25)),
        ))];

        // Create a renderer as observer
        let mut renderer = TypstRenderer::new(PathBuf::from("test.typ"));

        // Manually run the solve with observer
        let mut state = ILPState::with_presence_variables_and_observer(&item_group, &mut renderer)?;

        let promotion_instances = PromotionInstances::from_promotions(
            &promotions,
            &item_group,
            &mut state,
            &mut renderer,
        )?;

        // Extract state for model creation
        let (pb, cost, item_presence) = state.into_parts();

        // Create the solver model
        let mut model = pb.minimise(cost).using(default_solver);

        ensure_presence_vars_len(item_presence.len(), item_group.len())?;

        // Add constraints with observer
        for (item_idx, z_i) in item_presence.iter().copied().enumerate() {
            let constraint_expr =
                promotion_instances.add_item_presence_term(Expression::from(z_i), item_idx);

            renderer.on_exclusivity_constraint(item_idx, &constraint_expr);

            model = model.with(constraint_expr.eq(1));
        }

        let _ = promotion_instances.add_constraints(model, &item_group, &mut renderer);

        // Verify captures
        assert_eq!(
            renderer.formulation().presence_vars.len(),
            3,
            "Should capture 3 presence variables"
        );
        assert!(
            !renderer.formulation().promotion_vars.is_empty(),
            "Should capture promotion variables"
        );
        assert_eq!(
            renderer.formulation().exclusivity_constraints.len(),
            3,
            "Should capture 3 exclusivity constraints"
        );

        Ok(())
    }

    #[test]
    fn render_produces_valid_typst_syntax() {
        let mut renderer = TypstRenderer::new(PathBuf::from("test.typ"));
        let mut pb = ProblemVariables::new();

        let var1 = pb.add(variable().binary());
        let var2 = pb.add(variable().binary());

        renderer.on_presence_variable(0, var1, 100);
        renderer.on_presence_variable(1, var2, 200);

        let output = renderer.render();
        let formulation = renderer.formulation();
        let var1_label = TypstRenderer::var_label(&formulation, var1);
        let var2_label = TypstRenderer::var_label(&formulation, var2);

        assert!(output.contains("= ILP Formulation"));
        assert!(output.contains("== Decision Variables"));
        assert!(output.contains("== Objective Function"));
        assert!(output.contains("== Constraints"));

        assert!(output.contains(&var1_label));
        assert!(output.contains(&var2_label));

        assert!(output.contains("100"));
        assert!(output.contains("200"));
    }

    #[test]
    fn write_creates_file() -> TestResult {
        let dir = tempdir()?;
        let file_path = dir.path().join("formulation.typ");

        let mut renderer = TypstRenderer::new(file_path.clone());
        let mut pb = ProblemVariables::new();

        let var1 = pb.add(variable().binary());

        renderer.on_presence_variable(0, var1, 100);

        renderer.write()?;

        assert!(file_path.exists());

        let content = fs::read_to_string(file_path)?;

        assert!(content.contains("ILP Formulation"));

        Ok(())
    }
}
