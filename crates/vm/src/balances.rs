//! `MutBalancesPerLockup`, `MutBalances` and `MutBalanceState`. A balance
//! object is shared wherever the official VM shares it (`MutBalances.add`
//! of a new lockup script keeps the very object, which a contract's asset
//! status and a frame may both hold), so it lives behind `Rc<RefCell>`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use alephium_model::{Hash, LockupScript, U256};

use crate::compare_bytes;

/// `dustUtxoAmount`: 0.001 ALPH.
pub const DUST_UTXO_AMOUNT: U256 = U256::new(1_000_000_000_000_000);
/// `minimalAlphInContract` since Rhone: 0.1 ALPH.
pub const MINIMAL_ALPH_IN_CONTRACT: U256 = U256::new(100_000_000_000_000_000);
/// `maxTokenPerContractUtxo`.
pub const MAX_TOKEN_PER_CONTRACT_UTXO: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct PerLockup {
    pub alph: U256,
    pub tokens: HashMap<Hash, U256>,
    pub scope_depth: i32,
}

pub type Balances = Rc<RefCell<PerLockup>>;

impl PerLockup {
    #[must_use]
    pub fn alph(amount: U256) -> Self {
        Self {
            alph: amount,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn token(id: Hash, amount: U256) -> Self {
        let mut tokens = HashMap::new();
        tokens.insert(id, amount);
        Self {
            tokens,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn shared(self) -> Balances {
        Rc::new(RefCell::new(self))
    }

    /// `tokenVector`: non-zero tokens by id.
    #[must_use]
    pub fn token_vector(&self) -> Vec<(Hash, U256)> {
        let mut tokens: Vec<(Hash, U256)> = self
            .tokens
            .iter()
            .filter(|(_, amount)| **amount != U256::ZERO)
            .map(|(id, amount)| (*id, *amount))
            .collect();
        tokens.sort_by(|(a, _), (b, _)| compare_bytes(&a.0, &b.0));
        tokens
    }

    pub fn add_alph(&mut self, amount: U256) -> Option<()> {
        self.alph = self.alph.checked_add(amount)?;
        Some(())
    }

    pub fn sub_alph(&mut self, amount: U256) -> Option<()> {
        self.alph = self.alph.checked_sub(amount)?;
        Some(())
    }

    pub fn add_token(&mut self, id: Hash, amount: U256) -> Option<()> {
        match self.tokens.get_mut(&id) {
            Some(current) => *current = current.checked_add(amount)?,
            None => {
                self.tokens.insert(id, amount);
            }
        }
        Some(())
    }

    pub fn sub_token(&mut self, id: Hash, amount: U256) -> Option<()> {
        let current = self.tokens.get_mut(&id)?;
        *current = current.checked_sub(amount)?;
        Some(())
    }

    /// `add`: all or nothing.
    pub fn add(&mut self, other: &Self) -> Option<()> {
        let mut result = self.clone();
        result.alph = result.alph.checked_add(other.alph)?;
        for (id, amount) in &other.tokens {
            result.add_token(*id, *amount)?;
        }
        self.alph = result.alph;
        self.tokens = result.tokens;
        Some(())
    }

    /// `utxoMinimalAlphAmountToCover`.
    #[must_use]
    pub fn minimal_alph_to_cover(&self, lockup: &LockupScript) -> Option<U256> {
        if lockup.is_asset() {
            let token_dust = DUST_UTXO_AMOUNT * U256::from(self.tokens.len() as u64);
            if self.alph > token_dust {
                let needed = token_dust + DUST_UTXO_AMOUNT;
                (self.alph < needed).then(|| needed - self.alph)
            } else if self.alph == token_dust {
                None
            } else {
                token_dust.checked_sub(self.alph)
            }
        } else {
            (self.alph < MINIMAL_ALPH_IN_CONTRACT).then(|| MINIMAL_ALPH_IN_CONTRACT - self.alph)
        }
    }
}

/// `MutBalances`: balances per lockup script, in insertion order.
#[derive(Debug, Clone, Default)]
pub struct MutBalances {
    pub all: Vec<(LockupScript, Balances)>,
}

impl MutBalances {
    #[must_use]
    pub fn get(&self, lockup: &LockupScript) -> Option<Balances> {
        self.all
            .iter()
            .find(|(script, _)| script == lockup)
            .map(|(_, balances)| Rc::clone(balances))
    }

    pub fn add_alph(&mut self, lockup: &LockupScript, amount: U256) -> Option<()> {
        match self.get(lockup) {
            Some(balances) => balances.borrow_mut().add_alph(amount),
            None => {
                self.all
                    .push((lockup.clone(), PerLockup::alph(amount).shared()));
                Some(())
            }
        }
    }

    pub fn add_token(&mut self, lockup: &LockupScript, id: Hash, amount: U256) -> Option<()> {
        match self.get(lockup) {
            Some(balances) => balances.borrow_mut().add_token(id, amount),
            None => {
                self.all
                    .push((lockup.clone(), PerLockup::token(id, amount).shared()));
                Some(())
            }
        }
    }

    pub fn sub_alph(&self, lockup: &LockupScript, amount: U256) -> Option<()> {
        self.get(lockup)?.borrow_mut().sub_alph(amount)
    }

    pub fn sub_token(&self, lockup: &LockupScript, id: Hash, amount: U256) -> Option<()> {
        self.get(lockup)?.borrow_mut().sub_token(id, amount)
    }

    /// `add`: merged into the held object, or the object itself is kept.
    pub fn add(&mut self, lockup: &LockupScript, balances: &Balances) -> Option<()> {
        match self.get(lockup) {
            Some(held) if Rc::ptr_eq(&held, balances) => {
                let copy = balances.borrow().clone();
                held.borrow_mut().add(&copy)
            }
            Some(held) => held.borrow_mut().add(&balances.borrow()),
            None => {
                self.all.push((lockup.clone(), Rc::clone(balances)));
                Some(())
            }
        }
    }

    /// `use`: copies one scope deeper; this one is emptied.
    pub fn use_all_scoped(&mut self) -> Self {
        let all = std::mem::take(&mut self.all)
            .into_iter()
            .map(|(lockup, balances)| {
                let mut copy = balances.borrow().clone();
                copy.scope_depth += 1;
                (lockup, copy.shared())
            })
            .collect();
        Self { all }
    }

    /// `useAll(lockupScript)`.
    pub fn take(&mut self, lockup: &LockupScript) -> Option<Balances> {
        let index = self.all.iter().position(|(script, _)| script == lockup)?;
        Some(self.all.remove(index).1)
    }

    /// `useForChainedInput`.
    pub fn take_for_chained_input(&mut self, lockup: &LockupScript) -> Option<Balances> {
        let index = self.all.iter().position(|(script, _)| script == lockup)?;
        let taken = std::mem::replace(&mut self.all[index].1, PerLockup::default().shared());
        Some(taken)
    }

    /// `useAll()`: everything summed into one, this emptied.
    pub fn take_sum(&mut self) -> Option<PerLockup> {
        if self.all.is_empty() {
            return None;
        }
        let mut sum = PerLockup::default();
        for (_, balances) in std::mem::take(&mut self.all) {
            // The official accumulator ignores an overflow here.
            let _ = sum.add(&balances.borrow());
        }
        Some(sum)
    }

    /// `merge`: every entry added, then the other emptied.
    pub fn merge(&mut self, other: &mut Self) -> Option<()> {
        for (lockup, balances) in &other.all {
            self.add(lockup, balances)?;
        }
        other.all.clear();
        Some(())
    }

    /// `MutBalances.from(inputs, outputs)`.
    pub fn from_io(
        inputs: &[alephium_model::AssetOutput],
        outputs: &[alephium_model::AssetOutput],
    ) -> Option<Self> {
        let mut balances = Self::default();
        for input in inputs {
            balances.add(&input.lockup_script, &per_lockup_of_asset(input).shared())?;
        }
        for output in outputs {
            let held = balances.get(&output.lockup_script)?;
            let mut held = held.borrow_mut();
            held.alph = held.alph.checked_sub(output.amount)?;
            for (id, amount) in &output.tokens {
                held.sub_token(*id, *amount)?;
            }
        }
        Some(balances)
    }
}

/// `MutBalancesPerLockup.from(output)`.
#[must_use]
pub fn per_lockup_of_asset(output: &alephium_model::AssetOutput) -> PerLockup {
    let mut balances = PerLockup::alph(output.amount);
    for (id, amount) in &output.tokens {
        balances.tokens.insert(*id, *amount);
    }
    balances
}

/// `MutBalanceState`: what a frame may still use and what it approved for
/// the next call.
#[derive(Debug, Clone, Default)]
pub struct BalanceState {
    pub remaining: MutBalances,
    pub approved: MutBalances,
}

pub type SharedBalanceState = Rc<RefCell<BalanceState>>;

impl BalanceState {
    pub fn approve_alph(&mut self, lockup: &LockupScript, amount: U256) -> Option<()> {
        self.remaining.sub_alph(lockup, amount)?;
        self.approved.add_alph(lockup, amount)
    }

    pub fn approve_token(&mut self, lockup: &LockupScript, id: Hash, amount: U256) -> Option<()> {
        self.remaining.sub_token(lockup, id, amount)?;
        self.approved.add_token(lockup, id, amount)
    }

    #[must_use]
    pub fn alph_remaining(&self, lockup: &LockupScript) -> Option<U256> {
        self.remaining
            .get(lockup)
            .map(|balances| balances.borrow().alph)
    }

    #[must_use]
    pub fn token_remaining(&self, lockup: &LockupScript, id: &Hash) -> Option<U256> {
        self.remaining
            .get(lockup)
            .and_then(|balances| balances.borrow().tokens.get(id).copied())
    }

    #[must_use]
    pub fn is_paying(&self, lockup: &LockupScript) -> bool {
        self.remaining
            .all
            .iter()
            .any(|(script, _)| script == lockup)
    }

    /// `useApproved`.
    pub fn use_approved(&mut self) -> Self {
        Self {
            remaining: self.approved.use_all_scoped(),
            approved: MutBalances::default(),
        }
    }
}
