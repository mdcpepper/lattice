//! ILP State

use std::fmt;

use good_lp::{Expression, ProblemVariables, Variable};
use smallvec::SmallVec;

use crate::{
    items::groups::ItemGroup,
    solvers::{
        SolverError,
        ilp::{
            build_presence_variables_and_objective,
            observer::{ILPObserver, NoopObserver},
        },
    },
};

/// Builder state for ILP problem variables and objective
pub struct ILPState {
    pb: ProblemVariables,
    cost: Expression,
    item_presence: SmallVec<[Variable; 10]>,
}

impl fmt::Debug for ILPState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ILPState")
            .field("pb", &"<ProblemVariables>")
            .field("cost", &"<Expression>")
            .field(
                "item_presence",
                &format!("[{} variables]", self.item_presence.len()),
            )
            .finish()
    }
}

impl ILPState {
    /// Create a new ILP state from problem variables and cost expression
    pub fn new(pb: ProblemVariables, cost: Expression) -> Self {
        Self {
            pb,
            cost,
            item_presence: SmallVec::new(),
        }
    }

    /// Create ILP state with presence variables for baseline full-price items
    ///
    /// Creates binary decision variables for each item at full price and initializes
    /// the objective expression with their costs. This establishes the baseline that
    /// promotion variables will compete against.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if any item's price cannot be represented exactly as
    /// a solver coefficient.
    pub fn with_presence_variables(item_group: &ItemGroup<'_>) -> Result<Self, SolverError> {
        let mut observer = NoopObserver;

        Self::with_presence_variables_and_observer(item_group, &mut observer)
    }

    /// Create ILP state with presence variables and an observer.
    ///
    /// This variant allows attaching an observer to capture the formulation
    /// as it's being built.
    ///
    /// # Errors
    ///
    /// Returns [`SolverError`] if any item's price cannot be represented exactly as
    /// a solver coefficient.
    pub fn with_presence_variables_and_observer<O: ILPObserver + ?Sized>(
        item_group: &ItemGroup<'_>,
        observer: &mut O,
    ) -> Result<Self, SolverError> {
        let mut pb = ProblemVariables::new();

        // Build presence variables with observer
        let (item_presence, cost) =
            build_presence_variables_and_objective(item_group, &mut pb, observer)?;

        Ok(Self {
            pb,
            cost,
            item_presence,
        })
    }

    /// Extract the problem variables, cost expression, and item presence variables
    pub fn into_parts(self) -> (ProblemVariables, Expression, SmallVec<[Variable; 10]>) {
        (self.pb, self.cost, self.item_presence)
    }

    /// Add a term to the objective function (cost expression)
    ///
    /// Tells the solver "if you choose this option (set this variable to 1), add this
    /// cost to the total". The solver compares all options and picks the combination
    /// that minimizes the item group total.
    pub fn add_to_objective(&mut self, var: Variable, coefficient: f64) {
        self.cost += var * coefficient;
    }

    /// Get mutable access to the problem variables
    ///
    /// Used to add new decision variables to the ILP problem.
    pub fn problem_variables_mut(&mut self) -> &mut ProblemVariables {
        &mut self.pb
    }
}

#[cfg(test)]
mod tests {
    use good_lp::{Expression, ProblemVariables};

    use super::*;

    #[test]
    fn debug_includes_item_presence_len() {
        let state = ILPState::new(ProblemVariables::new(), Expression::default());

        let formatted = format!("{state:?}");

        assert!(formatted.contains("ILPState"));
        assert!(formatted.contains("item_presence"));
        assert!(formatted.contains("0 variables"));
    }
}
