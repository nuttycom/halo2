use super::{ProvingKey, VerifyingKey};
use crate::{
    arithmetic::{Curve, CurveAffine, Field},
    plonk::{circuit::ConstraintSystem, Error},
    poly::{
        commitment::{Blind, Params},
        EvaluationDomain, Rotation,
    },
};

pub(crate) struct Assembly {
    mapping: Vec<Vec<Vec<(usize, usize)>>>,
    aux: Vec<Vec<Vec<(usize, usize)>>>,
    sizes: Vec<Vec<Vec<usize>>>,
}

impl Assembly {
    pub(crate) fn new<C: CurveAffine>(
        params: &Params<C>,
        cs: &ConstraintSystem<C::Scalar>,
    ) -> Self {
        let mut assembly = Assembly {
            mapping: vec![],
            aux: vec![],
            sizes: vec![],
        };

        // Initialize the copy vector to keep track of copy constraints in all
        // the permutation arguments.
        for p in &cs.permutations {
            let mut columns = vec![];
            for i in 0..p.columns.len() {
                // Computes [(i, 0), (i, 1), ..., (i, n - 1)]
                columns.push((0..params.n).map(|j| (i, j as usize)).collect());
            }
            assembly.mapping.push(columns.clone());
            assembly.aux.push(columns);
            assembly
                .sizes
                .push(vec![vec![1usize; params.n as usize]; p.columns.len()]);
        }

        assembly
    }

    pub(crate) fn copy(
        &mut self,
        permutation: usize,
        left_column: usize,
        left_row: usize,
        right_column: usize,
        right_row: usize,
    ) -> Result<(), Error> {
        // Check bounds first
        if permutation >= self.mapping.len()
            || left_column >= self.mapping[permutation].len()
            || left_row >= self.mapping[permutation][left_column].len()
            || right_column >= self.mapping[permutation].len()
            || right_row >= self.mapping[permutation][right_column].len()
        {
            return Err(Error::BoundsFailure);
        }

        let mut left_cycle = self.aux[permutation][left_column][left_row];
        let mut right_cycle = self.aux[permutation][right_column][right_row];

        if left_cycle == right_cycle {
            return Ok(());
        }

        if self.sizes[permutation][left_cycle.0][left_cycle.1]
            < self.sizes[permutation][right_cycle.0][right_cycle.1]
        {
            std::mem::swap(&mut left_cycle, &mut right_cycle);
        }

        self.sizes[permutation][left_cycle.0][left_cycle.1] +=
            self.sizes[permutation][right_cycle.0][right_cycle.1];
        let mut i = right_cycle;
        loop {
            self.aux[permutation][i.0][i.1] = left_cycle;
            i = self.mapping[permutation][i.0][i.1];
            if i == right_cycle {
                break;
            }
        }

        let tmp = self.mapping[permutation][left_column][left_row];
        self.mapping[permutation][left_column][left_row] =
            self.mapping[permutation][right_column][right_row];
        self.mapping[permutation][right_column][right_row] = tmp;

        Ok(())
    }

    pub(crate) fn build_keys<C: CurveAffine>(
        self,
        params: &Params<C>,
        cs: &ConstraintSystem<C::Scalar>,
        domain: &EvaluationDomain<C::Scalar>,
    ) -> (Vec<ProvingKey<C>>, Vec<VerifyingKey<C>>) {
        // Get the largest permutation argument length in terms of the number of
        // advice columns involved.
        let largest_permutation_length = cs
            .permutations
            .iter()
            .map(|p| p.columns.len())
            .max()
            .unwrap_or_default();

        // Compute [omega^0, omega^1, ..., omega^{params.n - 1}]
        let mut omega_powers = Vec::with_capacity(params.n as usize);
        {
            let mut cur = C::Scalar::one();
            for _ in 0..params.n {
                omega_powers.push(cur);
                cur *= &domain.get_omega();
            }
        }

        // Compute [omega_powers * \delta^0, omega_powers * \delta^1, ..., omega_powers * \delta^m]
        let mut deltaomega = Vec::with_capacity(largest_permutation_length);
        {
            let mut cur = C::Scalar::one();
            for _ in 0..largest_permutation_length {
                let mut omega_powers = omega_powers.clone();
                for o in &mut omega_powers {
                    *o *= &cur;
                }

                deltaomega.push(omega_powers);

                cur *= &C::Scalar::DELTA;
            }
        }

        // Compute permutation polynomials, convert to coset form and
        // pre-compute commitments for the SRS.
        let mut pks = vec![];
        let mut vks = vec![];
        for (p, mapping) in cs.permutations.iter().zip(self.mapping.iter()) {
            let mut commitments = vec![];
            let mut permutations = vec![];
            let mut polys = vec![];
            let mut cosets = vec![];
            for i in 0..p.columns.len() {
                // Computes the permutation polynomial based on the permutation
                // description in the assembly.
                let mut permutation_poly = domain.empty_lagrange();
                for (j, p) in permutation_poly.iter_mut().enumerate() {
                    let (permuted_i, permuted_j) = mapping[i][j];
                    *p = deltaomega[permuted_i][permuted_j];
                }

                // Compute commitment to permutation polynomial
                commitments.push(
                    params
                        .commit_lagrange(&permutation_poly, Blind::default())
                        .to_affine(),
                );
                // Store permutation polynomial and precompute its coset evaluation
                permutations.push(permutation_poly.clone());
                let poly = domain.lagrange_to_coeff(permutation_poly);
                polys.push(poly.clone());
                cosets.push(domain.coeff_to_extended(poly, Rotation::default()));
            }
            vks.push(VerifyingKey { commitments });
            pks.push(ProvingKey {
                permutations,
                polys,
                cosets,
            });
        }

        (pks, vks)
    }
}
