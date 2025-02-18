//! Clause database reduction.

use starlit_macros::Bitfield;

use crate::{search::Search, trail::Reason};

/// References to all data used during clause database reduction.
#[allow(missing_docs)]
pub struct ReduceOps<'a> {
    pub search: &'a mut Search,
}

/// Reduction score.
#[derive(PartialEq, Eq, PartialOrd, Ord, Bitfield)]
struct Score(
    // LSB to MSB, so lexicographic order of (used, glue, len)
    #[bitfield(
        24 clamp => len: usize,
        6 => glue: usize,
        2 => inv_used: usize,
    )]
    u32,
);

impl<'a> ReduceOps<'a> {
    /// Performs clause database reduction.
    pub fn reduce(&mut self) {
        tracing::debug!("reduce");
        self.protect_clauses(true);

        let mut deletion_candidates = vec![];

        let long_clauses = &mut self.search.clauses.long;
        let mut clause_iter = None;
        while let Some(clause) = long_clauses.next_clause(&mut clause_iter) {
            let len = long_clauses.clause_len(clause);
            let data = long_clauses.data_mut(clause);
            if data.protected() {
                continue;
            }
            if !data.redundant() {
                continue;
            }

            // The code below implements a variant of the three tiered clause reduction strategy
            // introduced by Chanseok Oh in ["Between SAT and UNSAT: The Fundamental Difference in
            // CDCL SAT"](https://doi.org/10.1007/978-3-319-24318-4_23).

            // Clauses with a very small glue are core clauses and always kept
            if data.glue() <= 2 {
                continue;
            }

            // Decrement the used counter, and for clauses with a medium glue, keep them if they
            // were used since the last reduction.
            if data.used() > 0 {
                data.set_used(data.used() - 1);
                if data.glue() <= 5 {
                    continue;
                }
            }

            let mut score = Score(0);
            score.set_inv_used(!data.used()); // Prefer deleting clauses not recently used
            score.set_glue(data.glue()); // Then prefer higher glue
            score.set_len(len); // And finally longer clauses

            deletion_candidates.push((score, clause));
        }

        // Delete half the candidates, selecting those with a higher score.
        let candidate_count = deletion_candidates.len();
        if !deletion_candidates.is_empty() {
            let (_lower, _nth, higher) =
                deletion_candidates.select_nth_unstable(candidate_count / 2);

            for &(_, clause) in higher.iter() {
                long_clauses.delete_clause(clause);
            }
        }

        self.protect_clauses(false);
    }

    /// Sets or resets the protected bit for all currently propagating long clauses.
    fn protect_clauses(&mut self, protected: bool) {
        for step in self.search.trail.steps() {
            if let Reason::Long(clause) = step.reason {
                self.search
                    .clauses
                    .long
                    .data_mut(clause)
                    .set_protected(protected);
            }
        }
    }
}
