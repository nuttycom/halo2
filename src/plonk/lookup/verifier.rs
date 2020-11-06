use super::super::ConstraintSystem;
use super::{Lookup, Proof};
use crate::arithmetic::{CurveAffine, Field};

impl<C: CurveAffine> Proof<C> {
    pub fn check_lookup_constraints(
        &self,
        cs: &ConstraintSystem<C::Scalar>,
        beta: C::Scalar,
        gamma: C::Scalar,
        theta: C::Scalar,
        l_0: C::Scalar,
        lookup: &Lookup,
        advice_evals: &[C::Scalar],
        fixed_evals: &[C::Scalar],
    ) -> Vec<C::Scalar> {
        let mut constraints = Vec::with_capacity(4);
        // l_0(X) * (1 - z'(X)) = 0
        {
            let first_product_constraint = l_0 * &(C::Scalar::one() - &self.product_eval);
            constraints.push(first_product_constraint);
        }

        // z'(X) (a'(X) + \beta) (s'(X) + \gamma)
        // - z'(\omega^{-1} X) (a_1(X) + \theta a_2(X) + ... + \beta) (s_1(X) + \theta s_2(X) + ... + \gamma)
        {
            let left = self.product_eval
                * &(self.permuted_input_eval + &beta)
                * &(self.permuted_table_eval + &gamma);

            let mut right = self.product_inv_eval;
            let mut input_term = C::Scalar::zero();
            for &input in lookup.input_columns.iter() {
                let eval = advice_evals[cs.get_any_query_index(input, 0)].clone();
                input_term *= &theta;
                input_term += &eval;
            }
            input_term += &beta;

            let mut table_term = C::Scalar::zero();
            for &table in lookup.table_columns.iter() {
                let eval = fixed_evals[cs.get_any_query_index(table, 0)].clone();
                table_term *= &theta;
                table_term += &eval;
            }
            table_term += &gamma;

            right *= &(input_term * &table_term);

            constraints.push(left - &right);
        }

        // Check that the first values in the permuted input column and permuted
        // fixed column are the same.
        // l_0(X) * (a'(X) - s'(X)) = 0
        {
            let first_lookup_constraint =
                l_0 * &(self.permuted_input_eval - &self.permuted_table_eval);
            constraints.push(first_lookup_constraint);
        }

        // Check that each value in the permuted lookup input column is either
        // equal to the value above it, or the value at the same index in the
        // permuted table column.
        // (a′(X)−s′(X))⋅(a′(X)−a′(\omega{-1} X)) = 0
        {
            let lookup_constraint = (self.permuted_input_eval - &self.permuted_table_eval)
                * &(self.permuted_input_eval - &self.permuted_input_inv_eval);
            constraints.push(lookup_constraint);
        }

        constraints
    }
}