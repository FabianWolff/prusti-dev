// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use encoder::vir;
use encoder::vir::PermAmount;
use std::fmt;
use std::fmt::Display;
use std::iter::FlatMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_set;
//use std::ops::Mul;

/// An access or predicate permission to a place
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Perm {
    Acc(vir::Expr, PermAmount),
    Pred(vir::Expr, PermAmount)
}

impl Perm {
    pub fn acc(place: vir::Expr, perm_amount: PermAmount) -> Self {
        Perm::Acc(place, perm_amount)
    }

    pub fn pred(place: vir::Expr, perm_amount: PermAmount) -> Self {
        Perm::Pred(place, perm_amount)
    }

    pub fn is_acc(&self) -> bool {
        match self {
            Perm::Acc(_, _) => true,
            _ => false,
        }
    }

    pub fn is_pred(&self) -> bool {
        match self {
            Perm::Pred(_, _) => true,
            _ => false,
        }
    }

    pub fn is_old(&self) -> bool {
        self.get_place().is_old()
    }

    pub fn is_curr(&self) -> bool {
        self.get_place().is_curr()
    }

    pub fn is_local(&self) -> bool {
        self.get_place().is_local()
    }

    pub fn is_simple_place(&self) -> bool {
        self.get_place().is_simple_place()
    }

    pub fn typed_ref_name(&self) -> Option<String> {
        self.get_place().typed_ref_name()
    }

    pub fn get_type(&self) -> &vir::Type {
        self.get_place().get_type()
    }

    pub fn get_label(&self) -> Option<&String> {
        self.get_place().get_label()
    }

    pub fn get_perm_amount(&self) -> PermAmount {
        match self {
            Perm::Acc(_, p) => *p,
            Perm::Pred(_, p) => *p,
        }
    }

    pub fn get_place(&self) -> &vir::Expr {
        match self {
            &Perm::Acc(ref place, _) |
            &Perm::Pred(ref place, _) => place,
        }
    }

    pub fn place_as_mut_ref(&mut self) -> &mut vir::Expr {
        match self {
            &mut Perm::Acc(ref mut place, _) |
            &mut Perm::Pred(ref mut place, _) => place,
        }
    }

    pub fn unwrap_place(self) -> vir::Expr {
        match self {
            Perm::Acc(place, _) |
            Perm::Pred(place, _) => place,
        }
    }

    pub fn map_place<F>(self, f: F) -> Self
        where F: Fn(vir::Expr) -> vir::Expr
    {
        match self {
            Perm::Acc(place, fr) => Perm::Acc(f(place), fr),
            Perm::Pred(place, fr) => Perm::Pred(f(place), fr),
        }
    }

    pub fn old<S: ToString + Clone + Display>(self, label: S) -> Self {
        self.map_place(|p| p.old(label.clone()))
    }

    pub fn has_proper_prefix(&self, other: &vir::Expr) -> bool {
        self.get_place().has_proper_prefix(other)
    }

    pub fn has_prefix(&self, other: &vir::Expr) -> bool {
        self.get_place().has_prefix(other)
    }

    pub fn init_perm_amount(self, new_perm: PermAmount) -> Self {
        assert!(new_perm.is_valid_for_specs());
        match self {
            Perm::Acc(expr, PermAmount::Unset) => Perm::Acc(expr, new_perm),
            Perm::Pred(expr, PermAmount::Unset) => Perm::Pred(expr, new_perm),
            Perm::Acc(_, perm) if perm == new_perm => self,
            Perm::Pred(_, perm) if perm == new_perm => self,
            x => unreachable!("{} new_perm={}", x, new_perm),
        }
    }

    pub fn update_perm_amount(self, new_perm: PermAmount) -> Self {
        assert!(self.get_perm_amount().is_valid_for_specs());  // Just a sanity check.
        assert!(new_perm.is_valid_for_specs());
        match self {
            Perm::Acc(expr, _) => Perm::Acc(expr, new_perm),
            Perm::Pred(expr, _) => Perm::Pred(expr, new_perm),
        }
    }
}

impl fmt::Display for Perm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Perm::Acc(ref place, perm_amount) =>
                write!(f, "Acc({}, {})", place, perm_amount),
            &Perm::Pred(ref place, perm_amount) =>
                write!(f, "Pred({}, {})", place, perm_amount),
        }
    }
}

impl fmt::Debug for Perm {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &Perm::Acc(ref place, perm_amount) =>
                write!(f, "Acc({:?}, {})", place, perm_amount),
            &Perm::Pred(ref place, perm_amount) =>
                write!(f, "Pred({:?}, {})", place, perm_amount),
        }
    }
}


/// A set of permissions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermSet {
    acc_perms: HashMap<vir::Expr, PermAmount>,
    pred_perms: HashMap<vir::Expr, PermAmount>,
}

impl PermSet {
    pub fn empty() -> Self {
        PermSet {
            acc_perms: HashMap::new(),
            pred_perms: HashMap::new()
        }
    }

    /// Corresponds to an `inhale`
    /// Note: the amount of the permission is actually ignored
    pub fn add(&mut self, perm: Perm) {
        match perm {
            Perm::Acc(place, perm_amount) =>
                self.acc_perms.insert(place, perm_amount),
            Perm::Pred(place, perm_amount) =>
                self.pred_perms.insert(place, perm_amount),
        };
    }

    pub fn add_all(&mut self, mut perms: Vec<Perm>) {
        for perm in perms.drain(..) {
            self.add(perm);
        }
    }

    /// Corresponds to an `exhale`
    /// Note: the amount of the permission is actually ignored
    pub fn remove(&mut self, perm: &Perm) {
        match perm {
            Perm::Acc(..) => self.acc_perms.remove(perm.get_place()),
            Perm::Pred(..) => self.pred_perms.remove(perm.get_place()),
        };
    }

    pub fn remove_all(&mut self, mut perms: Vec<&Perm>) {
        for perm in perms.drain(..) {
            self.remove(perm);
        }
    }

    /// Corresponds to an `assert`
    /// Note: the amount of the permission is actually ignored
    pub fn contains(&self, perm: &Perm) -> bool {
        match perm {
            Perm::Acc(..) => self.acc_perms.contains_key(perm.get_place()),
            Perm::Pred(..) => self.pred_perms.contains_key(perm.get_place()),
        }
    }

    pub fn contains_all(&self, perms: Vec<&Perm>) -> bool {
        perms.iter().all(|x| self.contains(x))
    }

    pub fn perms(mut self) -> Vec<Perm> {
        let mut perms = vec![];
        for (place, perm_amount) in self.acc_perms.drain() {
            perms.push(Perm::acc(place, perm_amount));
        }
        for (place, perm_amount) in self.pred_perms.drain() {
            perms.push(Perm::pred(place, perm_amount));
        }
        perms
    }
}

impl fmt::Display for PermSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{")?;
        let mut first = true;
        for perm in self.clone().perms().iter() {
            if !first {
                write!(f, ", ")?;
            }
            first = false;
            write!(f, "{}", perm)?;
        }
        write!(f, "}}")
    }
}


pub trait PermIterator {
    fn collect_curr(&mut self) -> Vec<Perm>;
    fn group_by_label(&mut self) -> HashMap<Option<String>, Vec<Perm>>;
}

impl<T> PermIterator for T where T: Iterator<Item = Perm> {
    fn collect_curr(&mut self) -> Vec<Perm> {
        self.filter(|perm| perm.is_curr()).collect()
    }

    fn group_by_label(&mut self) -> HashMap<Option<String>, Vec<Perm>> {
        let mut res_perms = HashMap::new();
        for perm in self {
            res_perms.entry(perm.get_label().cloned()).or_insert(vec![]).push(perm.clone());
        }
        res_perms
    }
}

/// Note: since this function performs set difference, it does **not**
/// panic if `left` has less permission than `right`.
fn place_perm_difference(
    mut left: HashMap<vir::Expr, PermAmount>,
    mut right: HashMap<vir::Expr, PermAmount>
) -> HashMap<vir::Expr, PermAmount> {
    for (place, right_perm_amount) in right.drain() {
        match left.get(&place) {
            Some(left_perm_amount) => {
                match (*left_perm_amount, right_perm_amount) {
                    (PermAmount::Read, PermAmount::Read) |
                    (PermAmount::Read, PermAmount::Write) |
                    (PermAmount::Write, PermAmount::Write) => {
                        left.remove(&place);
                    },
                    _ => {
                        unreachable!("left={} right={}", left_perm_amount, right_perm_amount)
                    },
                }
            },
            None => {}
        }
    }
    left
}

/// Set difference that takes into account that removing `x.f` also removes any `x.f.g.h`
pub fn perm_difference(mut left: HashSet<Perm>, mut right: HashSet<Perm>) -> HashSet<Perm> {
    trace!("[enter] perm_difference(left={:?}, right={:?})", left, right);
    let left_acc = left.iter().filter(|x| x.is_acc()).cloned();
    let left_pred = left.iter().filter(|x| x.is_pred()).cloned();
    let right_acc = right.iter().filter(|x| x.is_acc()).cloned();
    let right_pred = right.iter().filter(|x| x.is_pred()).cloned();
    let mut res = vec![];
    res.extend(
        place_perm_difference(
            left_acc.map(|p| (p.get_place().clone(), p.get_perm_amount())).collect(),
            right_acc.map(|p| (p.get_place().clone(), p.get_perm_amount())).collect(),
        ).drain().map(|(place, amount)| Perm::Acc(place, amount)).collect::<Vec<_>>()
    );
    res.extend(
        place_perm_difference(
            left_pred.map(|p| (p.get_place().clone(), p.get_perm_amount())).collect(),
            right_pred.map(|p| (p.get_place().clone(), p.get_perm_amount())).collect(),
        ).drain().map(|(place, amount)| Perm::Pred(place, amount)).collect::<Vec<_>>()
    );
    res.into_iter().collect()
}
